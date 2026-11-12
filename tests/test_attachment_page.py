"""붙임 쪽 경계 감지(문단 글·머리 표 첫 칸·글자처럼 취급하는 도형)."""
from pathlib import Path
import unittest

from docfit_core.attachment_page import (
    KIND_SHAPE, KIND_TABLE, KIND_TEXT, body_paragraphs, find_attachment_starts,
    is_attachment_label, pull_up_blocked, table_paragraph_indices,
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
    def test_last_page_starting_with_attachment_is_blocked(self):
        self.assertTrue(pull_up_blocked(3, [(2, True), (3, True)]))

    def test_attachment_mid_page_or_earlier_is_not_blocked(self):
        self.assertFalse(pull_up_blocked(3, [(3, False)]))
        self.assertFalse(pull_up_blocked(3, [(2, True)]))
        self.assertFalse(pull_up_blocked(None, [(3, True)]))
        self.assertFalse(pull_up_blocked(3, []))


if __name__ == "__main__":
    unittest.main()
