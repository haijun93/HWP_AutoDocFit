"""Rust/Python 스캐너 비교기 임시 트리 시험 (Windows 전용)."""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "nt", "Windows 전용 Rust FindFirstFileExW 비교 시험")
class ScanCompareTests(unittest.TestCase):
    def test_temporary_tree_matches_python_and_rust(self):
        binary = Path(os.environ.get("SCANRUST_KLIB", os.environ.get("KLIB_EXE", ROOT / "klib" / "target" / "release" / "klib.exe")))
        if not binary.is_file():
            self.skipTest(f"Rust binary is not built: {binary}")
        with tempfile.TemporaryDirectory(prefix="scan-compare-test-") as temp:
            root = Path(temp) / "한글 루트"
            (root / "문서" / ("긴 경로_" * 5)).mkdir(parents=True)
            (root / "문서" / ("긴 경로_" * 5) / "자료.pdf").write_bytes(b"sample")
            (root / "문서" / "표.HWPX").write_bytes(b"sample2")
            (root / "문서" / ".숨김.pdf").touch()
            (root / "~$임시.xlsx").touch()
            (root / "Temp" / "nested").mkdir(parents=True)
            (root / "Temp" / "nested" / "숨김.pdf").touch()
            (root / "node_modules" / "nested").mkdir(parents=True)
            (root / "node_modules" / "nested" / "숨김.pdf").touch()
            env = os.environ.copy()
            env["PYTHONIOENCODING"] = "utf-8"
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "scan_compare.py"), "--root", str(root),
                 "--binary", str(binary), "--mode", "docs"],
                cwd=ROOT, env=env, capture_output=True, text=True, encoding="utf-8", timeout=90,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("differences=0", result.stdout)


if __name__ == "__main__":
    unittest.main()
