import json
import tempfile
import unittest
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from scripts.allqa_compare import qa_checks, render, xml_hashes
from scripts.allqa_run import enabled_stage_check, read_docset


def make_hwpx(path: Path, text: str = "alpha", bold: str = "0") -> None:
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr("Contents/header.xml", '<h><charPr id="0" bold="' + bold + '"/></h>')
        archive.writestr("Contents/section0.xml", '<s><p><run charPrIDRef="0"><t>' + text + '</t></run></p></s>')


class AllqaToolsTests(unittest.TestCase):
    def test_docset_reads_only_marked_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "set.md"
            path.write_text("<!-- ALLQA-DATA -->\n| id | source | hash | kind |\n|---|---|---|---|\n| a | tests/a.txt | deadbeef | TXT |\n<!-- /ALLQA-DATA -->", encoding="utf-8")
            self.assertEqual(read_docset(path), [{"id": "a", "source": "tests/a.txt", "sha256": "deadbeef", "kind": "TXT"}])

    def test_xml_hashes_separate_text_and_format(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b, c = (Path(tmp) / name for name in ("a.hwpx", "b.hwpx", "c.hwpx"))
            make_hwpx(a, "alpha", "0"); make_hwpx(b, "alpha", "1"); make_hwpx(c, "beta", "0")
            self.assertEqual(xml_hashes(a)["text_xml_sha256"], xml_hashes(b)["text_xml_sha256"])
            self.assertNotEqual(xml_hashes(a)["format_xml_sha256"], xml_hashes(b)["format_xml_sha256"])
            self.assertNotEqual(xml_hashes(a)["text_xml_sha256"], xml_hashes(c)["text_xml_sha256"])

    def test_qa_records_missing_page_count_as_unmeasured(self):
        row = {"source_sha256_before": "x", "source_sha256_after": "x", "copy_sha256": "y", "copy_sha256_after": "y", "returncode": 0,
               "errors": [], "saved": 1, "text_preserved_ignoring_whitespace": [True],
               "paragraph_counts": [1], "page_count": None, "stages_selected": {}, "result_files": []}
        checks = dict(qa_checks(row, Path("made-up.hwpx")))
        self.assertEqual(checks["원본 해시 불변"], "PASS")
        self.assertEqual(checks["쪽 수 기록"], "UNMEASURED")

    def test_enabled_stages_are_checked_against_measured_stages(self):
        result = enabled_stage_check({"body_spacing": True, "style_unify": False},
                                     [{"name": "본문 자간·단어 분리 조정"}])
        self.assertTrue(result["pass"])
        self.assertEqual(result["executed"], ["body_spacing"])

    def test_render_reports_paragraph_differences_and_defects(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); a = root / "a.hwpx"; b = root / "b.hwpx"
            make_hwpx(a, "alpha"); make_hwpx(b, "beta")
            row_a = {"id": "x", "com": {"total_seconds": 2, "com_calls": 5, "stages": []},
                     "source_sha256_before": "a", "source_sha256_after": "a", "returncode": 0,
                     "copy_sha256": "a", "copy_sha256_after": "a",
                     "errors": [], "saved": 1, "text_preserved_ignoring_whitespace": [True],
                     "paragraph_counts": [1], "page_count": 1, "stages_selected": {}, "log_file": "",
                     "result_files": [str(a)]}
            row_b = dict(row_a, com={"total_seconds": 1, "com_calls": 4, "stages": []}, result_files=[str(b)])
            report = render({"label": "before", "results": [row_a]}, {"label": "after", "results": [row_b]}, root, root)
            self.assertIn("XML 해시", report)
            self.assertIn("결함 목록", report)
            self.assertIn("x: 결과 HWPX 차이", report)


if __name__ == "__main__":
    unittest.main()
