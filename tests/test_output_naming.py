"""한번에 적용 결과 파일은 기존 파일을 덮어쓰지 않는다."""
import os
import tempfile
import unittest
from pathlib import Path

from docfit_core.output_naming import available_output_path, latest_output_path

ROOT = Path(__file__).resolve().parents[1]


class OutputNamingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name) / "보고(서식적용).hwpx"

    def test_free_name_is_kept(self):
        self.assertEqual(available_output_path(self.base), str(self.base))

    def test_existing_results_are_never_reused(self):
        self.base.write_bytes(b"edited by user")
        second = Path(available_output_path(self.base))
        self.assertEqual(second.name, "보고(서식적용) (2).hwpx")
        second.write_bytes(b"x")
        self.assertEqual(Path(available_output_path(self.base)).name, "보고(서식적용) (3).hwpx")
        self.assertEqual(self.base.read_bytes(), b"edited by user")

    def test_existing_folder_with_the_same_name_is_skipped(self):
        self.base.mkdir()
        self.assertEqual(Path(available_output_path(self.base)).name, "보고(서식적용) (2).hwpx")

    def test_resume_picks_the_newest_numbered_result(self):
        self.assertEqual(latest_output_path(self.base), str(self.base))
        self.base.write_bytes(b"old")
        newer = self.base.with_name("보고(서식적용) (2).hwpx")
        newer.write_bytes(b"new")
        self.base.with_name("보고(서식적용) 메모.hwpx").write_bytes(b"other")
        os.utime(self.base, ns=(1_000_000_000, 1_000_000_000))
        os.utime(newer, ns=(2_000_000_000, 2_000_000_000))
        self.assertEqual(latest_output_path(self.base), str(newer))

    def test_pipeline_saves_through_the_collision_guard(self):
        source = (ROOT / "docfit_engine" / "pipeline.py").read_text(encoding="utf-8-sig")
        self.assertIn("저장파일 = available_output_path(기본_저장파일)", source)
        self.assertNotRegex(source, r"(?m)^\s*저장파일 = 저장파일명\(파일\)")


if __name__ == "__main__":
    unittest.main()
