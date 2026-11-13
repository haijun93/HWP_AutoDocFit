"""업무자료함 출처 변경분 색인(NBIDX1): 쉬는 시간 작업·출처 다시 색인 중계·앱 배선."""
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from docfit_app.idle_gate import IdleGate, IdleSignals
from docfit_app.idle_scheduler import IdleContext, IdleIndexScheduler
from docfit_app.library_bridge import LibraryBridgeError
from docfit_app.notebook_idle_refresh import NOTEBOOK_IDLE_JOB, NotebookSourceRefreshJob
from docfit_app.notebook_service import NOTEBOOK_COMMANDS, NotebookService

QUIET = IdleSignals(input_idle_seconds=600, cpu_percent=0)


class Probe:
    def __init__(self, signals):
        self.signals = signals

    def sample(self):
        return self.signals


class CheckBridge:
    def __init__(self, reply):
        self.reply = reply
        self.requests = []

    def request(self, request):
        self.requests.append(request)
        if isinstance(self.reply, Exception):
            raise self.reply
        return self.reply


class FakeService:
    def __init__(self, outcomes):
        self.outcomes = list(outcomes)
        self.calls = []

    def refresh_source(self, notebook_id, source_id):
        self.calls.append((notebook_id, source_id))
        outcome = self.outcomes.pop(0)
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


class NotebookSourceRefreshJobTests(unittest.TestCase):
    def setUp(self):
        self.ctx = IdleContext(IdleGate(Probe(QUIET), sample_interval=0), NOTEBOOK_IDLE_JOB, poll_seconds=0.01)

    def job(self, bridge, service):
        return NotebookSourceRefreshJob(lambda: bridge, lambda _bridge: service)

    def test_pending_only_after_klib_used(self):
        self.assertFalse(NotebookSourceRefreshJob(lambda: None, lambda b: None).pending())
        self.assertTrue(NotebookSourceRefreshJob(lambda: object(), lambda b: None).pending())

    def test_refreshes_only_targets_from_klib_check(self):
        bridge = CheckBridge({"ok": True, "checked": 4, "counts": {"ready": 1, "stale": 2, "missing": 1},
                              "refresh": [{"notebook_id": 3, "source_id": 5}, {"notebook_id": 3, "source_id": 7}]})
        service = FakeService([{"ok": True, "relinked": 2, "unresolved": 1}, {"ok": False, "message": "본문 없음"}])
        job = self.job(bridge, service)
        job.run(self.ctx)
        self.assertEqual(bridge.requests, [{"cmd": "nb_source_check"}])
        self.assertEqual(service.calls, [(3, 5), (3, 7)])
        self.assertEqual(job.last_result, {"checked": 4, "changed": 2, "refreshed": 1, "errors": 1, "missing": 1,
                                           "relinked": 2, "unresolved": 1, "unavailable": False})

    def test_bridge_error_on_one_source_does_not_stop_others(self):
        bridge = CheckBridge({"ok": True, "checked": 2, "refresh": [{"notebook_id": 1, "source_id": 1}, {"notebook_id": 1, "source_id": 2}]})
        service = FakeService([LibraryBridgeError("끊김"), {"ok": True}])
        job = self.job(bridge, service)
        job.run(self.ctx)
        self.assertEqual((job.last_result["refreshed"], job.last_result["errors"]), (1, 1))

    def test_stops_when_checkpoint_says_no(self):
        bridge = CheckBridge({"ok": True, "refresh": [{"notebook_id": 1, "source_id": 1}]})
        service = FakeService([{"ok": True}])
        ctx = SimpleNamespace(checkpoint=lambda: False, progress=lambda *a: None)
        job = self.job(bridge, service)
        job.run(ctx)
        self.assertEqual(service.calls, [])
        self.assertEqual(job.last_result["changed"], 1)

    def test_unavailable_store_is_reported_not_error(self):
        job = self.job(CheckBridge({"ok": False, "code": "NB_UNAVAILABLE", "message": "옮기기 실패"}), FakeService([]))
        job.run(self.ctx)
        self.assertTrue(job.last_result["unavailable"])
        self.assertEqual(job.last_result["errors"], 0)
        job = self.job(CheckBridge(LibraryBridgeError("끊김")), FakeService([]))
        job.run(self.ctx)
        self.assertEqual(job.last_result["errors"], 1)


class RecordingBridge:
    def __init__(self, replies):
        self.replies = list(replies)
        self.requests = []

    def request(self, request):
        self.requests.append(request)
        return self.replies.pop(0)


class RefreshSourceServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.pdf = Path(self.temp.name) / "보고서.pdf"
        self.pdf.write_bytes(b"%PDF-1.4")

    def tearDown(self):
        self.temp.cleanup()

    def test_commands_are_allowed(self):
        for cmd in ("nb_source_check", "nb_source_refresh", "nb_status", "nb_migrate"):
            self.assertIn(cmd, NOTEBOOK_COMMANDS)

    def test_klib_direct_formats_pass_through(self):
        bridge = RecordingBridge([{"ok": True, "source_id": 4, "relinked": 0}])
        service = NotebookService(bridge, extractor=lambda path: self.fail("추출하지 않아야 함"))
        self.assertEqual(service.handle({"cmd": "nb_source_refresh", "notebook_id": 1, "source_id": 4})["source_id"], 4)
        self.assertEqual(bridge.requests, [{"cmd": "nb_source_refresh", "notebook_id": 1, "source_id": 4}])

    def test_python_formats_are_sent_in_parts_with_same_source_id(self):
        paragraphs = [{"text": "가" * 3000, "level": 0} for _ in range(30)]  # 40 KiB 넘음 → 여러 묶음
        bridge = RecordingBridge([{"ok": False, "code": "NEEDS_PARAGRAPHS", "path": str(self.pdf)}]
                                 + [{"ok": True, "staged": 1}] * 10)
        service = NotebookService(bridge, extractor=lambda path: paragraphs)
        bridge.replies[-1] = {"ok": True, "source_id": 9, "paragraph_count": 30}
        result = service.refresh_source(2, 9)
        sent = bridge.requests[1:]
        self.assertGreater(len(sent), 1)
        self.assertEqual([r["part"] for r in sent], list(range(len(sent))))
        self.assertEqual([r["last"] for r in sent], [False] * (len(sent) - 1) + [True])
        self.assertTrue(all(r["source_id"] == 9 and r["notebook_id"] == 2 for r in sent))
        self.assertEqual(sum(len(r["paragraphs"]) for r in sent), 30)
        self.assertTrue(result["ok"])

    def test_extraction_failure_and_missing_file(self):
        bridge = RecordingBridge([{"ok": False, "code": "NEEDS_PARAGRAPHS", "path": str(self.pdf)}])
        def broken(path):
            raise ValueError("깨진 파일")
        result = NotebookService(bridge, extractor=broken).refresh_source(1, 1)
        self.assertFalse(result["ok"])
        self.assertIn("깨진 파일", result["message"])
        bridge = RecordingBridge([{"ok": False, "code": "NEEDS_PARAGRAPHS", "path": str(self.pdf) + ".없음"}])
        self.assertEqual(NotebookService(bridge, extractor=broken).refresh_source(1, 1)["code"], "MISSING")

    def test_failed_part_stops_and_is_returned(self):
        paragraphs = [{"text": "가" * 3000} for _ in range(30)]
        bridge = RecordingBridge([{"ok": False, "code": "NEEDS_PARAGRAPHS", "path": str(self.pdf)},
                                  {"ok": True}, {"ok": False, "message": "순서 오류"}])
        result = NotebookService(bridge, extractor=lambda p: paragraphs).refresh_source(1, 1)
        self.assertEqual(result, {"ok": False, "message": "순서 오류"})
        self.assertEqual(len(bridge.requests), 3)


class Owner:
    """DesktopWebBridge의 쉬는 시간 배선 메서드만 빌려 쓰는 가짜."""

    def __init__(self, folder):
        store = SimpleNamespace(_path=Path(folder) / "settings.json")
        self._settings_store = lambda: store

    from desktop_web_ui import DesktopWebBridge as _B
    _idle_scheduler = _B._idle_scheduler
    _register_notebook_idle_job = _B._register_notebook_idle_job
    _notebook_service_for = _B._notebook_service_for
    _get_library_bridge = _B._get_library_bridge
    del _B


class NotebookIdleWiringTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.scheduler = IdleIndexScheduler(IdleGate(Probe(IdleSignals(input_idle_seconds=0)), sample_interval=0), poll_seconds=0.01)

    def tearDown(self):
        self.scheduler.close()
        self.temp.cleanup()

    def test_registered_once_when_klib_bridge_is_first_made(self):
        owner = Owner(self.temp.name)
        fake_bridge = object()
        with patch("docfit_app.idle_scheduler.shared_scheduler", return_value=self.scheduler), \
                patch("docfit_app.library_bridge.LibraryBridge", return_value=fake_bridge):
            self.assertIs(owner._get_library_bridge(), fake_bridge)
            owner._register_notebook_idle_job()
            owner._get_library_bridge()
        jobs = self.scheduler.status()["jobs"]
        self.assertEqual(sorted(jobs), sorted(["library-items", NOTEBOOK_IDLE_JOB]))
        self.assertEqual(jobs[NOTEBOOK_IDLE_JOB]["every_seconds"], 1800)
        job = self.scheduler._entries[NOTEBOOK_IDLE_JOB].job
        self.assertTrue(job.pending())
        service = job._service_factory(fake_bridge)
        self.assertIsInstance(service, NotebookService)
        self.assertIs(service, owner._notebook_service_for(fake_bridge))  # 화면과 같은 서비스


if __name__ == "__main__":
    unittest.main()
