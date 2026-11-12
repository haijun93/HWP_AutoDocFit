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

