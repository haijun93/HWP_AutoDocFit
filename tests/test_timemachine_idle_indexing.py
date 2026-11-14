"""타임머신 본문 자동 재색인을 쉬는 시간 스케줄러로 돌리는 연결 시험(IDLE1). 파일은 임시 폴더 안에서만."""
import json
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.idle_gate import IdleGate, IdleSignals
from docfit_app.idle_scheduler import IdleIndexScheduler
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

    def make_service(self, scheduler="default"):
        return TimemachineService(self.base / "설정", db_path=self.base / "timeline.db", agent=Agent(),
                                  scanner=stub_scanner, thumb_dir=self.base / "thumbs",
                                  idle_scheduler=self.scheduler if scheduler == "default" else scheduler)

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
