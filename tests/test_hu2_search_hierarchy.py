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

    def test_hwpx_table_chapter_and_regular_table_exclusion(self):
        records = [
            ParagraphRecord(0, "제1장 총칙", "body", 8, True, table_cell_id=1,
                            table_id=1, table_rows=1, table_cols=1),
            ParagraphRecord(1, "제2장 적용", "body", 8, True, table_cell_id=2,
                            table_id=2, table_rows=1, table_cols=2),
            ParagraphRecord(2, "제3장 데이터", "body", 8, True, table_cell_id=3,
                            table_id=3, table_rows=2, table_cols=2),
            ParagraphRecord(3, "일반 표 문장", "body", 8, True, table_cell_id=3,
                            table_id=3, table_rows=2, table_cols=2),
            ParagraphRecord(4, "항목 값", "body", 8, True, table_cell_id=4,
                            table_id=4, table_rows=1, table_cols=2),
        ]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
            result = classify_paragraph_levels("table.hwpx", records, backend="python")
        self.assertEqual(["chapter", "chapter", "table_cell", "table_cell", "table_cell"],
                         [p.level for p in result])

    def test_prefixed_chapters_and_mid_sentence_citation(self):
        lines = ["[붙임 1] 제1장 총칙", "【별표】 제2장 산출기준"]
        self.assertEqual(["chapter", "chapter"], self.levels(lines))
        self.assertEqual(["body"], self.levels(["앞에서 제1장에 명시된 바와 같이 적용한다."]))

    def test_single_chapter_in_one_cell_table_needs_no_number_sequence(self):
        row = ParagraphRecord(0, "제1장 총칙", "body", 8, True, table_cell_id=1,
                              table_id=1, table_rows=1, table_cols=1)
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
            result = classify_paragraph_levels("single.hwpx", [row], backend="python")
        self.assertEqual("chapter", result[0].level)

    def test_twenty_existing_plain_text_cases_remain_body(self):
        examples = [
            "이 문서의 목적을 설명합니다.", "제1절의 내용은 다음과 같다.",
            "본문에서 제2장 적용 범위를 인용한다.", "별표를 참고하여 작성한다.",
            "붙임 자료는 뒤에 둔다.", "(별첨) 자료를 확인한다.",
            "문장 끝에서 제1장이라고 부른다.", "예시는 제3장의 규정을 따른다.",
            "장 번호는 본문 중간에만 있다.", "이 항목은 일반 설명이다.",
            "1장으로 구성된 안내입니다.", "제1장본문은 붙어 있어 제목이 아니다.",
            "[붙임 1]은 제출 서류다.", "【별표】는 참고 자료다.",
            "(별첨) 표기는 문장 안에서 쓰였다.", "가운데 제1장 총칙 표현이 나온다.",
            "검토 결과를 다음과 같이 설명한다.", "관련 기준을 함께 확인한다.",
            "인용: 제1장에 명시된 바와 같이", "계획을 순서대로 진행한다.",
        ]
        # 이전 규칙의 출력 고정: 접두 제거가 본문까지 훑거나 일반 분류를 바꾸지 않는지 확인.
        expected = ["section", "body", "body", "body", "body", "asterisk", "body", "body",
                    "section", "section", "body", "body", "body", "body", "asterisk", "body",
                    "body", "body", "body", "body"]
        self.assertEqual(expected, self.levels(examples))
