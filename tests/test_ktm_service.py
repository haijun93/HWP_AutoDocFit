"""몽땅검색(ktm.exe) 앱 연결: 수명 명령 선택·응답 해석·상태 문구. 실제 exe는 띄우지 않는다."""
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from docfit_app.ktm_service import KtmService, status_text


class FakeRunner:
    def __init__(self, replies):
        self.replies = replies  # 인자 튜플 → dict 또는 bytes
        self.calls = []

    def __call__(self, argv, **kwargs):
        args = tuple(argv[1:])
        self.calls.append(args)
        reply = self.replies.get(args, {"ok": True})
        out = reply if isinstance(reply, bytes) else (json.dumps(reply, ensure_ascii=False) + "\n").encode()
        return subprocess.CompletedProcess(argv, 0, out, b"")


class KtmServiceTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.exe = Path(self.tmp.name) / "ktm.exe"
        self.exe.write_bytes(b"new")
        self.installed = Path(self.tmp.name) / "bin" / "ktm.exe"
        self.installed.parent.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def service(self, status, **more):
        replies = {("--status",): {"ok": True, "installed_path": str(self.installed), **status}, **more}
        runner = FakeRunner(replies)
        return KtmService(bundled=lambda: self.exe, runner=runner), runner

    def test_sync_installs_when_missing(self):
        service, runner = self.service({"installed": False, "running": False, "disabled": False})
        service.sync(True)
        self.assertEqual(runner.calls[-1], ("--install", "--respect-disabled"))

    def test_sync_does_nothing_when_running_same_build(self):
        self.installed.write_bytes(b"new")
        service, runner = self.service({"installed": True, "running": True, "disabled": False})
        result = service.sync(True)
        self.assertEqual(result["skipped"], "up-to-date")
        self.assertEqual(runner.calls, [("--status",)])

    def test_sync_replaces_older_build_even_when_running(self):
        self.installed.write_bytes(b"old")
        service, runner = self.service({"installed": True, "running": True, "disabled": False})
        service.sync(True)
        self.assertEqual(runner.calls[-1], ("--install", "--respect-disabled"))

    def test_sync_off_disables_once(self):
        service, runner = self.service({"installed": True, "running": True, "autostart": True, "disabled": False})
        service.sync(False)
        self.assertEqual(runner.calls[-1], ("--disable",))
        service, runner = self.service({"installed": True, "running": False, "autostart": False, "disabled": True})
        self.assertEqual(service.sync(False)["skipped"], "already-off")
        self.assertEqual(runner.calls, [("--status",)])

    def test_setting_on_overrides_stale_disabled_marker(self):
        service, runner = self.service({"installed": True, "running": False, "disabled": True})
        service.sync(True)
        self.assertEqual(runner.calls[-1], ("--install",))

    def test_apply_setting_maps_to_commands(self):
        service, runner = self.service({})
        service.apply_setting(True)
        service.apply_setting(False)
        self.assertEqual(runner.calls, [("--install",), ("--disable",)])

    def test_missing_exe_and_bad_output_are_errors_not_exceptions(self):
        self.assertFalse(KtmService(bundled=lambda: None, runner=FakeRunner({})).status()["ok"])
        self.assertTrue(KtmService(bundled=lambda: None, runner=FakeRunner({})).sync(True)["ok"])
        runner = FakeRunner({("--status",): b"not json"})
        self.assertFalse(KtmService(bundled=lambda: self.exe, runner=runner).status()["ok"])

        def boom(*a, **k):
            raise subprocess.TimeoutExpired("ktm", 20)
        self.assertFalse(KtmService(bundled=lambda: self.exe, runner=boom).status()["ok"])

    def test_status_text(self):
        self.assertIn("켜짐", status_text({"ok": True, "running": True, "autostart": True}))
        self.assertIn("자동 실행 꺼짐", status_text({"ok": True, "running": True, "autostart": False}))
        self.assertIn("단축키", status_text({"ok": True, "running": True, "hotkey_error": "중복"}))
        self.assertEqual(status_text({"ok": True, "running": False, "disabled": True}), "꺼짐")
        self.assertEqual(status_text({"ok": True}), "설치 안 됨")
        self.assertTrue(status_text({"ok": False, "error": "x"}).startswith("오류"))

    def test_setting_default_on_and_labeled(self):
        from docfit_app.configuration_service import fields
        from docfit_app.settings_schema import DEFAULTS
        self.assertIs(DEFAULTS["mongttang_search"], True)
        field = next(f for f in fields() if f["key"] == "mongttang_search")
        self.assertEqual((field["type"], field["group"]), ("boolean", "몽땅검색"))


if __name__ == "__main__":
    unittest.main()
