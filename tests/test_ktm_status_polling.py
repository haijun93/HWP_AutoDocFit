"""KTM 상태 반복 조회 회귀: 합성 설정·가짜 runner만 사용한다."""
import json
import subprocess
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from desktop_web_ui import DesktopWebBridge
from docfit_app.ktm_service import KtmService, KtmStatusCache, status_text


class CountingRunner:
    def __init__(self):
        self.calls = []
        self.running = True

    def __call__(self, argv, **kwargs):
        self.calls.append(tuple(argv[1:]))
        if argv[1] in ("--install", "--disable"):
            self.running = argv[1] == "--install"
        result = {"ok": True, "running": self.running, "disabled": not self.running,
                  "autostart": self.running, "installed": True}
        return subprocess.CompletedProcess(argv, 0, json.dumps(result).encode(), b"")


class KtmPollingTests(unittest.TestCase):
    def setUp(self):
        self.runner = CountingRunner()
        service = KtmService(bundled=lambda: "synthetic-ktm.exe", runner=self.runner)
        factory = patch("docfit_app.ktm_service.KtmService", return_value=service)
        factory.start()
        self.addCleanup(factory.stop)
        self.bridge = DesktopWebBridge(SimpleNamespace(running=False))
        self.bridge._configuration = lambda: SimpleNamespace(snapshot=lambda **kw: {
            "values": {}, "fields": [{"key": "mongttang_search"}]})
        shell = patch("docfit_app.shell_menu.status", return_value={"registered": False})
        shell.start()
        self.addCleanup(shell.stop)

    def test_repeated_settings_snapshots_launch_status_once(self):
        for _ in range(120):
            result = self.bridge.get_settings()
            self.assertTrue(result["ok"], result)
            self.assertIn("켜짐", result["data"]["fields"][0]["status"])
        self.assertEqual(self.runner.calls, [("--status",)])

    def test_concurrent_settings_readers_share_one_query(self):
        from concurrent.futures import ThreadPoolExecutor
        with ThreadPoolExecutor(max_workers=8) as pool:
            results = list(pool.map(lambda _: self.bridge.get_settings(), range(40)))
        self.assertTrue(all(result["ok"] for result in results))
        self.assertEqual(self.runner.calls, [("--status",)])

    def test_unrelated_snapshot_and_empty_handoff_never_query_ktm(self):
        self.bridge._attach_ktm_status({"fields": []})
        with patch("docfit_app.screen_mode.current", return_value=13), patch(
                "docfit_app.handoff.take", return_value={"paths": []}):
            for _ in range(120):
                self.assertTrue(self.bridge.handoff({"action": "take"})["ok"])
        self.assertEqual(self.runner.calls, [])

    def test_one_second_polling_for_two_minutes_launches_24_times(self):
        now = [0.0]
        self.bridge._ktm_status._clock = lambda: now[0]
        for second in range(120):
            now[0] = float(second)
            self.assertTrue(self.bridge.get_settings()["ok"])
        self.assertEqual(self.runner.calls, [("--status",)] * 24)

    def test_save_immediately_replaces_cached_state_and_completion_refreshes(self):
        from threading import Event
        from tempfile import TemporaryDirectory
        from pathlib import Path
        from docfit_app.configuration_service import ConfigurationService
        from docfit_app.settings_service import SettingsService

        entered, release = Event(), Event()
        service = self.bridge._ktm_status._service
        original = service.apply_setting

        def blocked(enabled):
            entered.set()
            if not release.wait(5):
                raise RuntimeError("test worker was not released")
            return original(enabled)

        service.apply_setting = blocked
        with TemporaryDirectory() as folder:
            config = ConfigurationService(SettingsService(Path(folder) / "settings.json"))
            self.bridge._configuration = lambda: config
            self.bridge._tk = lambda callback, **kw: callback()
            self.bridge.gui._Flutter설정_반영 = lambda values: None
            before = self.bridge.get_settings()["data"]
            self.assertEqual(self.runner.calls, [("--status",)])
            started = []
            apply = self.bridge._ktm_status.apply_in_background

            def record(enabled):
                thread = apply(enabled)
                started.append(thread)
                return thread

            self.bridge._ktm_status.apply_in_background = record
            try:
                saved = self.bridge.save_settings({"mongttang_search": False}, before["revision"])
                self.assertTrue(saved["ok"], saved)
                field = next(f for f in saved["data"]["fields"] if f["key"] == "mongttang_search")
                self.assertEqual(field["status"], "끄는 중…")
                self.assertTrue(entered.wait(5))
                # TTL이 지나도 적용 중에는 별도 조회·옛 상태 반환이 없다.
                self.bridge._ktm_status._clock = lambda: 1000.0
                for _ in range(20):
                    self.assertTrue(self.bridge._ktm_status.status()["applying"])
            finally:
                release.set()
                for thread in started:
                    thread.join(5)
                    self.assertFalse(thread.is_alive())
            after = self.bridge.get_settings()["data"]
            field = next(f for f in after["fields"] if f["key"] == "mongttang_search")
            self.assertEqual(field["status"], "꺼짐")
            self.assertEqual(self.runner.calls, [("--status",), ("--disable",), ("--status",)])
            thread = apply(True)
            thread.join(5)
            self.assertFalse(thread.is_alive())
            self.assertIn("켜짐", status_text(self.bridge._ktm_status.status()))
            self.assertEqual(self.runner.calls[-2:], [("--install",), ("--status",)])

    def test_failed_status_is_cached_but_retried_at_expiry(self):
        now = [0.0]
        service = self.bridge._ktm_status._service
        with patch.object(service, "status", return_value={"ok": False, "error": "timeout"}) as query:
            cache = KtmStatusCache(service=service, clock=lambda: now[0])
            for _ in range(20):
                self.assertFalse(cache.status()["ok"])
            self.assertEqual(query.call_count, 1)
            now[0] = 5.0
            cache.status()
            self.assertEqual(query.call_count, 2)

    def test_failed_apply_replaces_successful_cached_state(self):
        cache = self.bridge._ktm_status
        self.assertTrue(cache.status()["running"])
        with patch.object(cache._service, "apply_setting", return_value={"ok": False, "error": "failed"}):
            thread = cache.apply_in_background(False)
            thread.join(5)
        self.assertFalse(thread.is_alive())
        self.assertEqual(cache.status(), {"ok": False, "error": "failed"})
        self.assertEqual(self.runner.calls, [("--status",)])

    def test_caller_cannot_mutate_cached_result(self):
        cache = self.bridge._ktm_status
        value = cache.status()
        value["running"] = False
        self.assertTrue(cache.status()["running"])
        self.assertEqual(self.runner.calls, [("--status",)])

    def test_inflight_old_query_cannot_overwrite_new_setting(self):
        from threading import Event, Thread
        cache = self.bridge._ktm_status
        entered, release = Event(), Event()
        query = cache._service.status

        def blocked():
            value = query()
            entered.set()
            if not release.wait(5):
                raise RuntimeError("test query was not released")
            return value

        cache._service.status = blocked
        values = []
        reader = Thread(target=lambda: values.append(cache.status()))
        worker = None
        reader.start()
        try:
            self.assertTrue(entered.wait(5))
            worker = cache.apply_in_background(False)
            self.assertEqual(status_text(cache.status()), "끄는 중…")
        finally:
            release.set()
            reader.join(5)
            if worker is not None:
                worker.join(5)
                self.assertFalse(worker.is_alive())
        self.assertFalse(reader.is_alive())
        self.assertEqual(status_text(values[0]), "끄는 중…")
        self.assertEqual(status_text(cache.status()), "꺼짐")

    def test_rapid_setting_changes_finish_with_latest_choice(self):
        from threading import Event
        cache = self.bridge._ktm_status
        entered, release = Event(), Event()
        apply = cache._service.apply_setting

        def blocked(enabled):
            if not enabled:
                entered.set()
                if not release.wait(5):
                    raise RuntimeError("test command was not released")
            return apply(enabled)

        cache._service.apply_setting = blocked
        workers = [cache.apply_in_background(False)]
        try:
            self.assertTrue(entered.wait(5))
            workers.append(cache.apply_in_background(True))
            self.assertEqual(status_text(cache.status()), "켜는 중…")
        finally:
            release.set()
            for worker in workers:
                worker.join(5)
                self.assertFalse(worker.is_alive())
        self.assertTrue(cache.status()["running"])
        self.assertEqual(self.runner.calls, [
            ("--disable",), ("--status",), ("--install",), ("--status",)])
