import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

from docfit_app import kfind_process as kp


class KfindProcessTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        env = patch.dict(os.environ, {"KFIND_DATA": str(self.root / "kfind")})
        env.start()
        self.addCleanup(env.stop)
        kp._USER_PROCESS = None
        self.addCleanup(setattr, kp, "_USER_PROCESS", None)

    def test_find_kfind_exe_returns_existing_file_or_none(self):
        exe = kp.find_kfind_exe()
        self.assertTrue(exe is None or exe.is_file())

    def test_fallback_mode_matches_rust_values_only(self):
        self.assertIsNone(kp.get_fallback_mode())
        kp.set_fallback_mode("task")
        self.assertEqual(kp.get_fallback_mode(), "task")
        kp.set_fallback_mode("user")
        self.assertEqual(json.loads(kp.settings_path().read_text(encoding="utf-8")), {"fallback": "user"})
        for bad in ("service", "User", ""):
            with self.assertRaises(ValueError):
                kp.set_fallback_mode(bad)

    def test_fallback_mode_keeps_other_keys_and_ignores_unknown_value(self):
        kp.settings_path().parent.mkdir(parents=True)
        kp.settings_path().write_text('{"fallback": "service", "x": 1}', encoding="utf-8")
        self.assertIsNone(kp.get_fallback_mode())
        kp.set_fallback_mode("task")
        self.assertEqual(json.loads(kp.settings_path().read_text(encoding="utf-8")), {"fallback": "task", "x": 1})

    @unittest.skipUnless(os.name == "nt", "Windows 파이프")
    def test_missing_pipe_is_not_alive(self):
        self.assertFalse(kp.is_pipe_alive(r"\\.\pipe\HwpAutoDocFit.TestMissing.9f3c", timeout=0.01))

    def test_legacy_engine_does_not_start_kfind(self):
        with patch.object(kp, "is_pipe_alive") as alive, patch("subprocess.Popen") as popen:
            self.assertFalse(kp.ensure_kfind_running(engine_provider=lambda: "legacy"))
        alive.assert_not_called()
        popen.assert_not_called()

    def test_alive_pipe_needs_no_start(self):
        with patch.object(kp, "is_pipe_alive", return_value=True), patch("subprocess.Popen") as popen:
            self.assertTrue(kp.ensure_kfind_running(engine_provider=lambda: "kfind"))
        popen.assert_not_called()

    def test_missing_exe_returns_false(self):
        with patch.object(kp, "is_pipe_alive", return_value=False), \
                patch.object(kp, "find_kfind_exe", return_value=None):
            self.assertFalse(kp.ensure_kfind_running(engine_provider=lambda: "kfind"))

    def test_starts_user_mode_hidden_and_waits_for_pipe(self):
        exe = self.root / "kfind.exe"
        exe.write_bytes(b"")
        proc = MagicMock()
        proc.poll.return_value = None
        with patch.object(kp, "is_pipe_alive", side_effect=[False, False, True]), \
                patch.object(kp, "find_kfind_exe", return_value=exe), \
                patch("subprocess.Popen", return_value=proc) as popen:
            self.assertTrue(kp.ensure_kfind_running(timeout=1.0, engine_provider=lambda: "kfind"))
        args, kwargs = popen.call_args
        self.assertEqual(args[0], [str(exe), "--user"])
        self.assertIs(kwargs["stdin"], kp.subprocess.DEVNULL)

    def test_process_that_exits_reports_failure(self):
        exe = self.root / "kfind.exe"
        exe.write_bytes(b"")
        proc = MagicMock()
        proc.poll.return_value = 1
        proc.returncode = 1
        with patch.object(kp, "is_pipe_alive", return_value=False), \
                patch.object(kp, "find_kfind_exe", return_value=exe), \
                patch("subprocess.Popen", return_value=proc):
            self.assertFalse(kp.ensure_kfind_running(timeout=1.0, engine_provider=lambda: "kfind"))

    def test_start_errors_never_raise(self):
        with patch.object(kp, "is_pipe_alive", side_effect=OSError("x")):
            self.assertFalse(kp.ensure_kfind_running(engine_provider=lambda: "kfind"))

    def test_service_management_without_exe(self):
        with patch.object(kp, "find_kfind_exe", return_value=None):
            for call in (kp.install_service, kp.uninstall_service):
                ok, msg = call()
                self.assertFalse(ok)
                self.assertIn("찾을 수 없습니다", msg)

    def test_install_runs_elevated_install_then_start(self):
        exe = self.root / "kfind.exe"
        with patch.object(kp, "_run_elevated", return_value=(True, "")) as run, \
                patch.object(kp, "_stop_own_user_processes"):
            ok, _ = kp.install_service(exe)
        self.assertTrue(ok)
        self.assertEqual([c.args[1] for c in run.call_args_list], ["--install", "--start"])


class AppStartupWiringTests(unittest.TestCase):
    def test_entry_point_starts_kfind_in_background_and_keeps_crlf(self):
        raw = (Path(__file__).resolve().parent.parent / "hwp-auto-docfit.py").read_bytes()
        self.assertEqual(raw.count(b"\n"), raw.count(b"\r\n"))
        text = raw.decode("utf-8")
        self.assertIn("from docfit_app.kfind_process import ensure_kfind_running", text)
        self.assertIn('target=ensure_kfind_running, name="kfind-ensure", daemon=True', text)


class KfindPackagingTests(unittest.TestCase):
    ROOT = Path(__file__).resolve().parent.parent

    def test_ci_tests_builds_and_verifies_kfind_like_kvf(self):
        ci = (self.ROOT / ".github/workflows/build-windows-exe.yml").read_text("utf-8")
        for crate in ("kfind/core", "kfind/names", "crates/kfind"):
            self.assertIn(f"cargo test --release --locked --manifest-path {crate}/Cargo.toml", ci)
        self.assertIn("Copy-Item crates/kfind/target/release/kfind.exe resources/kfind.exe", ci)
        self.assertIn("resources/kfind.exe.sha256", ci)
        self.assertIn("kfind missing from frozen bundle", ci)

    def test_uninstall_stops_user_mode_removes_service_and_indexes(self):
        iss = (self.ROOT / "installer/setup.iss").read_text("utf-8")
        self.assertIn('/IM kfind.exe /FI ""USERNAME eq {username}""', iss)
        self.assertIn("ShellExec('runas', KfindExe, '--uninstall'", iss)
        for folder in (r"{commonappdata}\HwpAutoDocFit\index", r"{localappdata}\HwpAutoDocFit\index",
                       r"{localappdata}\HwpAutoDocFit\kfind"):
            self.assertIn(folder, iss)

    def test_license_notices_name_engine_folders(self):
        for name in ("LICENSE", "README.md", ".github/README.md"):
            text = (self.ROOT / name).read_text("utf-8")
            self.assertIn("PolyForm Strict", text, name)
            self.assertIn("crates/kfind/", text, name)
            self.assertIn("kfind/names/", text, name)
        self.assertIn("roaring 0.12.0", (self.ROOT / "THIRD_PARTY_NOTICES.md").read_text("utf-8"))


if __name__ == "__main__":
    unittest.main()
