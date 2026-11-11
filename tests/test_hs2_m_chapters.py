"""TXT/MD 제N장 판정의 연속성·형식 경계 시험."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_core.timemachine.content_search import (
    ContentSearchIndex, ParagraphRecord, classify_paragraph_levels, extract_paragraphs,
)


class PlainChapterTests(unittest.TestCase):
    def levels(self, lines, suffix=".txt", engine=None):
        records = [ParagraphRecord(i, text, "body", 8, False) for i, text in enumerate(lines)]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=engine):
            return [p.level for p in classify_paragraph_levels("sample" + suffix, records)]

    def test_sequence_even_after_body(self):
        for suffix in (".txt", ".md"):
            self.assertEqual(["body", "chapter", "body", "chapter"], self.levels(
                ["서문입니다", "제1장 시설 관리", "설명입니다", "제2장 운영 관리"], suffix))

    def test_isolated_reversed_and_skipped_numbers(self):
        for suffix in (".txt", ".md"):
            for lines in (["제1장 시설 관리"], ["제2장 운영", "제1장 시설"],
                          ["제1장 시설", "제3장 운영"]):
                self.assertEqual(["body"] * len(lines), self.levels(lines, suffix))

    def test_long_line_is_not_evidence(self):
        for suffix in (".txt", ".md"):
            self.assertEqual(["body", "body"], self.levels(
                ["제1장 " + "가" * 57, "제2장 운영"], suffix))
            self.assertEqual(["chapter", "chapter"], self.levels(
                ["제1장 " + "가" * 56, "제2장 운영"], suffix))

    def test_explicit_markdown_and_hwpx_are_preserved(self):
        self.assertEqual(["document_title", "body"],
                         self.levels(["# 제1장 시설", "제2장 운영"], ".md"))
        self.assertEqual(["chapter"], self.levels(["제1장 운영 지침"], ".hwpx"))

    def test_engine_result_also_obeys_plain_chapter_rule(self):
        self.assertEqual(["body"], self.levels(["제1장 시설"], engine={"levels": [1]}))
        self.assertEqual(["chapter", "chapter"], self.levels(
            ["제1장 시설", "제2장 운영"], engine={"levels": [8, 8]}))

    def test_real_txt_and_md_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            index = ContentSearchIndex(Path(tmp) / "content.sqlite3")
            try:
                for suffix in (".txt", ".md"):
                    path = Path(tmp) / ("chapters" + suffix)
                    path.write_text("서문\n제1장 시설\n삼체 행성\n제2장 운영", encoding="utf-8")
                    with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                        self.assertEqual(["body", "chapter", "body", "chapter"],
                                         [p.level for p in extract_paragraphs(path)])
                        self.assertTrue(index.index_file(path)["indexed"])
                self.assertTrue(all(item["breadcrumb"] == ["제1장 시설"]
                                    for item in index.keyword_search("삼체 행성")))
            finally:
                index.close()
