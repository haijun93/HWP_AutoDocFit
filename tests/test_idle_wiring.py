"""앱 진입점이 쉬는 시간 스케줄러를 한 번만 등록하는지(IDLE1)."""
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from desktop_web_ui import DesktopWebBridge
from docfit_app.idle_gate import IdleGate, IdleSignals
from docfit_app.idle_scheduler import IdleIndexScheduler
from docfit_app.library_idle_refresh import LIBRARY_IDLE_JOB


class Probe:
    def sample(self):
        return IdleSignals(input_idle_seconds=0)  # 사용 중: 시험 동안 작업이 돌지 않게


class IdleWiringTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.scheduler = IdleIndexScheduler(IdleGate(Probe(), sample_interval=0), poll_seconds=0.01)
        store = SimpleNamespace(_path=Path(self.temp.name) / "settings.json")
        self.owner = SimpleNamespace(_settings_store=lambda: store)

    def tearDown(self):
        self.scheduler.close()
        self.temp.cleanup()

    def test_library_job_registered_once_and_waits_for_bridge(self):
        with patch("docfit_app.idle_scheduler.shared_scheduler", return_value=self.scheduler):
            first = DesktopWebBridge._idle_scheduler(self.owner)
            second = DesktopWebBridge._idle_scheduler(self.owner)
        self.assertIs(first, self.scheduler)
        self.assertIs(second, self.scheduler)
        jobs = self.scheduler.status()["jobs"]
        self.assertEqual(list(jobs), [LIBRARY_IDLE_JOB])
        self.assertEqual(jobs[LIBRARY_IDLE_JOB]["every_seconds"], 1800)
        job = self.scheduler._entries[LIBRARY_IDLE_JOB].job
        self.assertFalse(job.pending())  # klib를 아직 안 썼으면 일하지 않음
        self.owner._library_bridge = object()
        self.assertTrue(job.pending())
        self.assertEqual(job.state_path, Path(self.temp.name) / "library-idle.json")


if __name__ == "__main__":
    unittest.main()
