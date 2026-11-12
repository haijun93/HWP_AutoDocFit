"""붙임 쪽 경계 감지(문단 글·머리 표 첫 칸·글자처럼 취급하는 도형)."""
from pathlib import Path
import unittest

from docfit_core.attachment_page import (
    KIND_SHAPE, KIND_TABLE, KIND_TEXT, body_paragraphs, find_attachment_starts,
    is_attachment_label, pull_up_blocked, report_fit_impossible, table_paragraph_indices,
)

FIXTURES = Path(__file__).resolve().parent / "fixtures"


class AttachmentLabelTest(unittest.TestCase):
    def test_label_forms(self):
        for value in ("붙임", "붙임 1", "[붙임 1]", "[ 붙임 ]", "〈별첨 2〉", "첨부1", "[붙임 1] 세부 일정"):
            self.assertTrue(is_attachment_label(value), value)

    def test_not_labels(self):
        for value in ("붙임  계획서 1부.  끝.", "관련 자료는 붙임 참고", "붙임 1  세부 계획",
                      "참고 1", "", None, "붙임 " + "가" * 70):
            self.assertFalse(is_attachment_label(value), value)

    def test_loose_label_needs_page_break(self):
        self.assertTrue(is_attachment_label("붙임 1  세부 계획", page_break=True))
        self.assertTrue(is_attachment_label("붙임1 | 제목", page_break=True))
        self.assertFalse(is_attachment_label("붙임  계획서 1부.  끝.", page_break=True))


class AttachmentStartsFixtureTest(unittest.TestCase):
    def starts(self, name):
        return find_attachment_starts(body_paragraphs(FIXTURES / name))

    def test_text_label(self):
        self.assertEqual(self.starts("attp_text.hwpx"), [(6, KIND_TEXT), (8, KIND_TEXT)])

    def test_head_table_first_cell(self):
        self.assertEqual(self.starts("attp_table.hwpx"), [(5, KIND_TABLE)])

    def test_inline_shape_text(self):
        self.assertEqual(self.starts("attp_shape.hwpx"), [(5, KIND_SHAPE)])

    def test_table_positions_for_number_check(self):
        self.assertEqual(table_paragraph_indices(body_paragraphs(FIXTURES / "attp_table.hwpx")), {5, 7})
        self.assertEqual(table_paragraph_indices(body_paragraphs(FIXTURES / "attp_shape.hwpx")), set())


class PullUpBlockedTest(unittest.TestCase):
    def test_page_break_attachment_on_last_page_with_full_page_before(self):
        self.assertTrue(pull_up_blocked(3, 3, True, 20, 4))

    def test_not_blocked_cases(self):
        self.assertFalse(pull_up_blocked(3, 3, True, 3, 4))      # 붙임 앞 쪽 줄이 적으면 당겨 볼 수 있다
        self.assertFalse(pull_up_blocked(3, 3, False, 20, 4))    # 쪽 나눔 없는 붙임은 함께 올라온다
        self.assertFalse(pull_up_blocked(3, 2, True, 20, 4))     # 붙임이 마지막 쪽이 아니다
        self.assertFalse(pull_up_blocked(3, 3, True, None, 4))   # 줄 수를 모름
        self.assertFalse(pull_up_blocked(None, 3, True, 20, 4))


class ReportFitImpossibleTest(unittest.TestCase):
    def test_bounds(self):
        self.assertTrue(report_fit_impossible(1, 1))
        self.assertTrue(report_fit_impossible(2, 2))
        self.assertFalse(report_fit_impossible(2, 1))
        self.assertFalse(report_fit_impossible(1, 0))
        self.assertFalse(report_fit_impossible(3, 2))


if __name__ == "__main__":
    unittest.main()
