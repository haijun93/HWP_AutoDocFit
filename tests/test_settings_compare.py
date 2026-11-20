import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.settings_compare import compare_case, cases


class SettingsCompareTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "Windows-only settings parity integration")
    def test_python_and_rust_match_on_isolated_cases(self):
        root = Path(__file__).resolve().parents[1]
        target = Path(os.environ.get("CARGO_TARGET_DIR", "E:/cargo-target/SETTINGSRS1"))
        exe = target / "debug" / "settings-core.exe"
        if not exe.exists():
            self.skipTest(f"Rust CLI not built: {exe}")
        for name, value in cases().items():
            with self.subTest(case=name):
                result = compare_case(exe, name, value)
                self.assertTrue(result["equal"], json.dumps(result, ensure_ascii=False, indent=2))

    @unittest.skipUnless(os.name == "nt", "Windows lock interoperability")
    def test_python_and_rust_process_locks_exclude_each_other(self):
        root = Path(__file__).resolve().parents[1]
        exe = Path(os.environ.get("CARGO_TARGET_DIR", "E:/cargo-target/SETTINGSRS1")) / "debug" / "settings-core.exe"
        if not exe.exists(): self.skipTest("Rust CLI not built")
        with tempfile.TemporaryDirectory(prefix="settings-lock-") as folder:
            path = Path(folder) / "settings.json"
            lock_path = Path(str(path) + ".lock")
            lock_path.touch()
            # Python owns the shared byte-range lock first; non-blocking Rust acquisition must fail.
            from docfit_infra.storage.process_lock import ProcessLock
            with ProcessLock(lock_path):
                proc = subprocess.Popen([str(exe), "set", str(path), '{"developer_mode":true}'],
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
                try:
                    with self.assertRaises(subprocess.TimeoutExpired):
                        proc.communicate(timeout=0.5)
                finally:
                    proc.kill()
                    proc.communicate()


if __name__ == "__main__": unittest.main()
