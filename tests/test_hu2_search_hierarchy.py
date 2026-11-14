"""HU2 검색 전용 위계 과승격 회귀 시험."""
import unittest
from unittest.mock import patch

from docfit_core.timemachine.content_search import ParagraphRecord, classify_paragraph_levels


class SearchHierarchyRegressionTests(unittest.TestCase):
    def levels(self, lines, suffix=".txt"):
        records = [ParagraphRecord(i, text, "body", 8, False) for i, text in enumerate(lines)]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
            return [p.level for p in classify_paragraph_levels("sample" + suffix, records)]

    def test_symbol_steps(self):
        self.assertEqual(["square", "square"], self.levels(["• 위치: ① 첫째", "• 위치: ② 둘째"]))
        self.assertEqual(["square", "circle", "dash", "asterisk", "asterisk"],
                         self.levels(["□ 시설", "ㅇ 관리", "- 내용", "• 위치: ① 첫째", "• 위치: ② 둘째"]))

    def test_markdown_numbered_list(self):
        for suffix in (".txt", ".md"):
            self.assertEqual(["chapter", "body", "body", "body"], self.levels(
                ["## 2. 계획", "1. 첫째", "2. 둘째", "3. 셋째"], suffix))

    def test_markdown_table_rows(self):
        for suffix in (".txt", ".md"):
            self.assertEqual(["table_cell"] * 3, self.levels(
                ["| 항목 | 값 |", "| --- | --- |", "| 시설 | 관리 |"], suffix))
