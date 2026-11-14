"""검색 대상 형식 점검(2026-11-14): csv·xlsm 본문 추출, md 기본 포함."""
import shutil
import tempfile
import unittest
from pathlib import Path

from docfit_core.timemachine.extractors import extract_document
from docfit_core.timemachine.rules import DEFAULT_EXTENSIONS

FIXTURES = Path(__file__).parent / "fixtures" / "timemachine_extract"


class SearchFormatTests(unittest.TestCase):
    def test_md_is_indexed_by_default(self):
        self.assertIn("md", DEFAULT_EXTENSIONS)

    def test_csv_rows_become_paragraphs(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "budget.csv"
            p.write_text("항목,금액\n예산 집행,1500000\n", encoding="utf-8")
            texts = [r.text for r in extract_document(p)]
        self.assertTrue(any("예산 집행" in t for t in texts), texts)

    def test_xlsm_is_read_like_xlsx(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "book.xlsm"
            shutil.copy2(FIXTURES / "sample.xlsx", p)
            xlsm = [r.text for r in extract_document(p)]
        xlsx = [r.text for r in extract_document(FIXTURES / "sample.xlsx")]
        self.assertEqual(xlsm, xlsx)


if __name__ == "__main__":
    unittest.main()
