"""타임머신 본문 자동 재색인을 쉬는 시간 스케줄러로 돌리는 연결 시험(IDLE1). 파일은 임시 폴더 안에서만."""
import json
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.idle_gate import IdleGate, IdleSignals
from docfit_app.idle_scheduler import IdleContext, IdleIndexScheduler
from docfit_app.timemachine_service import CONTENT_IDLE_JOB, TimemachineService

QUIET = IdleSignals(input_idle_seconds=600, cpu_percent=0)
BUSY = IdleSignals(input_idle_seconds=0)


class Probe:
    def __init__(self, signals):
        self.signals = signals

    def sample(self):
        return self.signals


class Agent:
    def running(self):
        return False

    def status_file(self):
        return None

    def request(self, command, **payload):
        return {"ok": True}


class StubResult:
    seen = written = removed = 0
    errors = []


class FakeKfind:
    def __init__(self, count=0, generation=1):
        self.count = count
        self.generation = generation
        self.ingested = []
        self.removed = []
        self.stale_keys = []

    def state(self):
        return {"state": "connected", "reason": None}

    def status(self):
        return {"state": "ready", "generation": self.generation, "count": self.count, "docs": self.count}

    def stale(self, scope, level_rule, *, after=None, limit=1000):
        return {"doc_keys": self.stale_keys[:limit], "more": len(self.stale_keys) > limit}

    def ingest(self, doc):
        self.ingested.append(doc)
        return {"generation": self.generation}

    def remove(self, doc_keys):
        self.removed.extend(doc_keys)
        return {"removed": len(doc_keys)}

    def close(self):
        pass


def stub_scanner(store, roots, rules, progress=None):
    return StubResult()


def wait_for(predicate, timeout=5.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.01)
    return False


class TimemachineIdleIndexingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.root = self.base / "문서"
        self.root.mkdir()
        self.probe = Probe(BUSY)
        self.scheduler = IdleIndexScheduler(IdleGate(self.probe, sample_interval=0), poll_seconds=0.01,
                                            sleep_seconds=0.05)
        self.service = self.make_service()

    def make_service(self, scheduler="default", kfind_client=None):
        return TimemachineService(self.base / "설정", db_path=self.base / "timeline.db", agent=Agent(),
                                  scanner=stub_scanner, thumb_dir=self.base / "thumbs",
                                  idle_scheduler=self.scheduler if scheduler == "default" else scheduler,
                                  kfind_client=kfind_client)

    def tearDown(self):
        self.service.close()
        self.scheduler.close()
        self.temp.cleanup()

    def enable(self, **extra):
        payload = {"roots": [str(self.root)], "time_mode": "max", "enabled": True, **extra}
        return self.service.command("settings_set", payload)

    def seed(self, *names):
        """색인이 비어 있으면 자동 재색인을 하지 않으므로 먼저 하나씩 넣어 둔다."""
        files = []
        for name in names:
            path = self.root / name
            path.write_text(f"{name} 본문", encoding="utf-8")
            self.service.content_index.index_file(path)
            files.append(path)
        return files

    def job(self):
        return self.scheduler.status()["jobs"][CONTENT_IDLE_JOB]

    def test_refresh_waits_for_idle_then_runs_with_progress(self):
        self.enable()
        first, = self.seed("a.txt")
        first.write_text("바뀐 본문 내용", encoding="utf-8")
        self.service._schedule_content_refresh()
        time.sleep(0.15)
        self.assertFalse(self.service.content_status()["running"])  # 사용 중에는 시작하지 않음
        self.assertEqual(self.job()["state"], "waiting")
        self.probe.signals = QUIET
        self.assertTrue(self.scheduler.wait_idle_queue())
        self.assertTrue(wait_for(lambda: not self.service.content_status()["running"]))
        status = self.service.content_status()
        self.assertEqual((status["trigger"], status["phase"]), ("idle", "finished"))
        self.assertEqual(status["processed"], status["total"])
        self.assertGreaterEqual(status["total"], 1)
        self.assertEqual(status["idle"]["state"], "idle")
        self.assertEqual(self.job()["state"], "done")
        self.assertTrue(self.service.command("content_search", {"query": "바뀐 본문"})["items"])

    def test_user_input_yields_after_one_document_and_resumes(self):
        self.enable()
        self.seed("a.txt", "b.txt", "c.txt")
        original = self.service.content_index.index_file
        calls = []

        def flip(path):
            calls.append(path)
            result = original(path)
            if len(calls) == 1:
                self.probe.signals = BUSY  # 첫 문서를 처리하는 동안 사용자가 입력
            return result

        with patch.object(self.service.content_index, "index_file", side_effect=flip):
            self.probe.signals = QUIET
            self.scheduler.request(CONTENT_IDLE_JOB)
            self.assertTrue(wait_for(lambda: self.service.content_status().get("yielded")))
            status = self.service.content_status()
            self.assertTrue(status["running"])
            self.assertEqual(status["processed"], 1)
            self.assertEqual(len(calls), 1)
            self.assertEqual(self.job()["state"], "yielded")
            self.probe.signals = QUIET
            self.assertTrue(self.scheduler.wait_idle_queue())
        self.assertEqual(len(calls), 3)
        status = self.service.content_status()
        self.assertEqual((status["phase"], status["processed"], status["total"]), ("finished", 3, 3))

    def test_manual_start_promotes_yielded_idle_run(self):
        self.enable()
        self.seed("a.txt", "b.txt")
        original = self.service.content_index.index_file
        calls = []

        def flip(path):
            calls.append(path)
            self.probe.signals = BUSY
            return original(path)

        with patch.object(self.service.content_index, "index_file", side_effect=flip):
            self.probe.signals = QUIET
            self.scheduler.request(CONTENT_IDLE_JOB)
            self.assertTrue(wait_for(lambda: self.service.content_status().get("yielded")))
            reply = self.service.command("content_index_start")
            self.assertEqual(reply["trigger"], "manual")
            self.assertTrue(wait_for(lambda: not self.service.content_status()["running"]))
        self.assertEqual(len(calls), 2)
        self.assertEqual(self.service.content_status()["phase"], "finished")

    def test_idle_indexing_off_restores_immediate_refresh(self):
        result = self.enable(idle_indexing=False)
        self.assertFalse(result["settings"]["idle_indexing"])
        self.assertEqual(self.scheduler.status()["state"], "paused")
        self.seed("a.txt")
        self.service._schedule_content_refresh()
        self.assertTrue(wait_for(lambda: self.service.content_status()["phase"] == "finished"))
        self.assertEqual(self.service.content_status()["trigger"], "auto")

    def test_settings_defaults_validation_and_gate_configuration(self):
        settings = self.service.command("settings_get")["settings"]
        self.assertEqual((settings["idle_indexing"], settings["idle_seconds"]), (True, 120))
        bad = self.enable(idle_seconds=3)
        self.assertEqual(bad["code"], "INVALID_INPUT")
        good = self.enable(idle_seconds=300)
        self.assertEqual(good["settings"]["idle_seconds"], 300)
        self.assertEqual(self.scheduler.status()["idle_seconds"], 300)
        saved = json.loads((self.base / "설정" / "timemachine.json").read_text(encoding="utf-8"))
        self.assertEqual((saved["idle_indexing"], saved["idle_seconds"]), (True, 300))

    def test_invalid_saved_idle_value_keeps_other_settings(self):
        self.enable()
        path = self.base / "설정" / "timemachine.json"
        data = json.loads(path.read_text(encoding="utf-8"))
        data.update(idle_seconds="빠르게", idle_indexing="네")
        path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
        self.service.close()
        self.service = self.make_service()
        settings = self.service.command("settings_get")["settings"]
        self.assertEqual(settings["roots"], [str(self.root)])
        self.assertEqual((settings["idle_indexing"], settings["idle_seconds"]), (True, 120))

    def test_recovery_seeding_bypasses_idle_gate_when_kfind_empty(self):
        """가짜 kfind(status count 0)·content_meta 3건으로 유휴 아님 상태에서도 3건 ingest 되는지."""
        kfind = FakeKfind(count=0)
        service = self.make_service(kfind_client=kfind)
        self.service.close()
        self.service = service  # tearDown이 임시 폴더보다 먼저 닫는다
        service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": True})

        for name in ("doc1.txt", "doc2.txt", "doc3.txt"):
            p = self.root / name
            p.write_text(f"{name} 본문 내용", encoding="utf-8")
            service.content_index.index_file(p)

        self.probe.signals = BUSY
        ctx = IdleContext(self.scheduler.gate, CONTENT_IDLE_JOB, poll_seconds=0.01)
        service._content_idle_ctx = ctx
        cancel = threading.Event()

        with patch("time.sleep") as mock_sleep:
            service._seed_kfind(cancel)
            # 3건 ingest 사이에 time.sleep(0.01) 양보가 2회(idx 1, 2) 호출됨
            self.assertEqual(mock_sleep.call_count, 2)
            for call in mock_sleep.call_args_list:
                self.assertAlmostEqual(call.args[0], 0.01, places=3)

        self.assertEqual(len(kfind.ingested), 3)
        self.assertFalse(ctx.yielded)
        self.assertTrue(service._kfind_synced)

    def test_app_startup_index_check_runs_in_background_so_the_service_returns_fast(self):
        """앱(스케줄러 있음)은 색인 점검이 오래 걸려도 서비스 생성이 막히지 않는다(몽땅검색 첫 실행이 느림, 2026-11-28)."""
        started = threading.Event()
        release = threading.Event()
        from docfit_core.timemachine import content_search

        original = content_search.ContentSearchIndex.needs_reindex_count

        def slow_check(self_, roots=None):
            started.set()
            release.wait(timeout=5)
            return 0

        with patch.object(content_search.ContentSearchIndex, "needs_reindex_count", slow_check):
            begun = time.monotonic()
            service = self.make_service(kfind_client=FakeKfind(count=1))
            elapsed = time.monotonic() - begun
            try:
                self.assertLess(elapsed, 2.0)
                self.assertTrue(started.wait(timeout=2))
                self.assertTrue(service.command("settings_get")["ok"])
            finally:
                release.set()
                service.close()
        self.assertIs(content_search.ContentSearchIndex.needs_reindex_count, original)

    def test_unreachable_new_engine_is_retried_instead_of_staying_not_connected_forever(self):
        class Offline(FakeKfind):
            def stale(self, *args, **kwargs):
                raise OSError("pipe missing")

        service = self.make_service(kfind_client=Offline(count=0))
        self.service.close()
        self.service = service
        service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": True})
        scheduled = []

        class FakeTimer:
            def __init__(self, interval, function):
                scheduled.append((interval, function))
                self.daemon = False

            def start(self):
                pass

        with patch("docfit_app.timemachine_service.threading.Timer", FakeTimer):
            service._recover_empty_kfind()
        self.assertEqual([interval for interval, _ in scheduled], [60])
        self.assertEqual(service._engine_state["engine_reason"], "kfind_unreachable")
        # 열 번까지만 다시 시도한다.
        service._connect_tries = 10
        scheduled.clear()
        with patch("docfit_app.timemachine_service.threading.Timer", FakeTimer):
            service._recover_empty_kfind()
        self.assertEqual(scheduled, [])

    def test_startup_recovery_starts_seeding_only_when_kfind_empty(self):
        """앱 시작 복구: stale 문서가 있고 내용 색인이 있을 때만 seeding 시작."""
        for count, stale_keys, expected in ((0, ["old"], 1), (3, ["old"], 1), (3, [], 0)):
            kfind = FakeKfind(count=count)
            kfind.stale_keys = stale_keys
            service = self.make_service(kfind_client=kfind)
            self.service.close()
            self.service = service  # tearDown이 임시 폴더보다 먼저 닫는다
            service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": True})
            p = self.root / "doc.txt"
            p.write_text("본문", encoding="utf-8")
            service.content_index.index_file(p)
            with patch.object(service, "_start_kfind_seeding") as start:
                service._recover_empty_kfind()
            self.assertEqual(start.call_count, expected, (count, stale_keys))

    def test_startup_schedules_recovery_even_when_reindex_pending(self):
        """재색인할 문서가 남아 있어도(쉬는 시간 예약) 시작 때 복구 점검 타이머를 건다(v2.2.69 후보 회귀)."""
        import docfit_app.timemachine_service as tm
        started = []
        real_timer = threading.Timer
        def fake_timer(delay, fn, *a, **k):
            t = real_timer(3600, lambda: None)
            started.append(getattr(fn, "__name__", ""))
            return t
        with patch.object(tm.TimemachineService, "_schedule_content_refresh"),              patch("docfit_app.timemachine_service.ContentSearchIndex.needs_reindex_count", return_value=5),              patch("threading.Timer", side_effect=fake_timer):
            service = self.make_service(kfind_client=FakeKfind(count=0))
        self.service.close()
        self.service = service  # tearDown이 임시 폴더보다 먼저 닫는다
        self.assertIn("_recover_empty_kfind", started)

    def test_normal_seeding_respects_idle_gate_when_kfind_has_index(self):
        """평소(색인 있음: count > 0)엔 기존처럼 유휴 조건을 지키는지."""
        kfind = FakeKfind(count=3)
        service = self.make_service(kfind_client=kfind)
        self.service.close()
        self.service = service  # tearDown이 임시 폴더보다 먼저 닫는다
        service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": True})

        for name in ("doc1.txt", "doc2.txt", "doc3.txt"):
            p = self.root / name
            p.write_text(f"{name} 본문 내용", encoding="utf-8")
            service.content_index.index_file(p)

        self.probe.signals = BUSY
        ctx = IdleContext(self.scheduler.gate, CONTENT_IDLE_JOB, poll_seconds=0.01)
        service._content_idle_ctx = ctx
        cancel = threading.Event()

        # 평소 상태에서 _seed_kfind를 돌리면 BUSY이므로 checkpoint에서 yielded=True가 되고 대기해야 함
        worker = threading.Thread(target=lambda: service._seed_kfind(cancel))
        worker.start()
        try:
            self.assertTrue(wait_for(lambda: ctx.yielded, timeout=2.0))
            self.assertEqual(len(kfind.ingested), 0)
        finally:
            cancel.set()
            worker.join(timeout=2.0)

    def test_recovery_seeding_respects_cancel_and_closing(self):
        """복구 seeding 중에도 취소 신호를 존중하여 즉시 중단되는지."""
        kfind = FakeKfind(count=0)
        service = self.make_service(kfind_client=kfind)
        self.service.close()
        self.service = service  # tearDown이 임시 폴더보다 먼저 닫는다
        service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": True})

        for name in ("doc1.txt", "doc2.txt", "doc3.txt"):
            p = self.root / name
            p.write_text(f"{name} 본문 내용", encoding="utf-8")
            service.content_index.index_file(p)

        self.probe.signals = BUSY
        ctx = IdleContext(self.scheduler.gate, CONTENT_IDLE_JOB, poll_seconds=0.01)
        service._content_idle_ctx = ctx
        cancel = threading.Event()
        cancel.set()

        service._seed_kfind(cancel)
        self.assertEqual(len(kfind.ingested), 0)

    def test_recovery_seeding_uses_sqlite_snapshot_without_reopening_documents(self):
        """복구 seeding은 사용자 문서를 다시 열지 않고 SQLite 스냅숏만 사용한다."""
        kfind = FakeKfind(count=0)
        service = self.make_service(kfind_client=kfind)
        self.service.close()
        self.service = service  # tearDown이 임시 폴더보다 먼저 닫는다
        service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": True})

        files = []
        for name in ("doc1.txt", "doc2.txt", "doc3.txt"):
            p = self.root / name
            p.write_text(f"{name} 원본 본문", encoding="utf-8")
            service.content_index.index_file(p)
            files.append(p)

        # 원본 문서를 디스크에서 삭제해도 SQLite 스냅숏으로 ingest됨
        for p in files:
            p.unlink()

        self.probe.signals = BUSY
        ctx = IdleContext(self.scheduler.gate, CONTENT_IDLE_JOB, poll_seconds=0.01)
        service._content_idle_ctx = ctx
        cancel = threading.Event()

        service._seed_kfind(cancel)
        self.assertEqual(len(kfind.ingested), 3)
        self.assertEqual(kfind.ingested[0]["paras"][0]["t"], "doc1.txt 원본 본문")
        self.assertTrue(service._kfind_synced)

    def test_close_unregisters_job(self):
        self.service.close()
        self.assertNotIn(CONTENT_IDLE_JOB, self.scheduler.status()["jobs"])

    def test_without_scheduler_status_has_no_idle_block(self):
        service = self.make_service(scheduler=None)
        try:
            self.assertIsNone(service.content_status()["idle"])
            self.assertIsNone(service.content_status()["trigger"])
        finally:
            service.close()


if __name__ == "__main__":
    unittest.main()
