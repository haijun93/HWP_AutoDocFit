# -*- coding: utf-8 -*-
"""세미콜론(;) 연관 검색어 순수 파서 및 일치 판정 단위 시험."""
import unittest

from docfit_core.timemachine.related_query import (
    QueryGroup,
    matches,
    normalized,
    parse_related_query,
)


class RelatedQueryParserTests(unittest.TestCase):
    def test_basic_semicolon_split(self):
        groups = parse_related_query("2026; 공사 계약")
        self.assertEqual(len(groups), 2)
        self.assertEqual(groups[0].query, "2026")
        self.assertEqual(groups[0].terms, ("2026",))
        self.assertEqual(groups[1].query, "공사 계약")
        self.assertEqual(groups[1].terms, ("공사", "계약"))

    def test_whitespace_and_empty_groups(self):
        groups = parse_related_query(" ; 2026;; 공사  계약 ; \"\"; ")
        self.assertEqual(len(groups), 2)
        self.assertEqual(groups[0].query, "2026")
        self.assertEqual(groups[0].terms, ("2026",))
        self.assertEqual(groups[1].query, "공사  계약")
        self.assertEqual(groups[1].terms, ("공사", "계약"))

    def test_quotes_preserve_spaces_and_semicolons(self):
        groups = parse_related_query('"2026; 공사" 계약; 업무')
        self.assertEqual(len(groups), 2)
        self.assertEqual(groups[0].terms, ("2026; 공사", "계약"))
        self.assertEqual(groups[1].terms, ("업무",))

    def test_curly_quotes_supported(self):
        groups = parse_related_query("“2026; 공사” 계약")
        self.assertEqual(len(groups), 1)
        self.assertEqual(groups[0].terms, ("2026; 공사", "계약"))

    def test_escaped_quotes(self):
        groups = parse_related_query(r'"공사\"계약" 지원')
        self.assertEqual(len(groups), 1)
        self.assertEqual(groups[0].terms, ('공사"계약', "지원"))

    def test_ampersand_and_near_treated_as_plain_chars(self):
        # &는 R&D 등 오인 방지를 위해 일반 문자로 취급
        # ~N 근접 연산자도 일반 문자
        groups = parse_related_query("R&D 예산; 계약~3")
        self.assertEqual(len(groups), 2)
        self.assertEqual(groups[0].terms, ("R&D", "예산"))
        self.assertEqual(groups[1].terms, ("계약~3",))

    def test_term_deduplication_within_group(self):
        groups = parse_related_query("공사 공사 계약; 계약 지원")
        self.assertEqual(len(groups), 2)
        self.assertEqual(groups[0].terms, ("공사", "계약"))
        self.assertEqual(groups[1].terms, ("계약", "지원"))

    def test_unclosed_quote_raises_value_error(self):
        with self.assertRaises(ValueError) as ctx:
            parse_related_query('"공사 계약')
        self.assertIn("따옴표를 닫아", str(ctx.exception))

        with self.assertRaises(ValueError) as ctx:
            parse_related_query("“공사 계약")
        self.assertIn("따옴표를 닫아", str(ctx.exception))

    def test_trailing_escape_raises_value_error(self):
        with self.assertRaises(ValueError) as ctx:
            parse_related_query(r'"공사 계약\"')
        self.assertIn("따옴표를 닫아", str(ctx.exception))

    def test_length_limit_exceeded_raises_value_error(self):
        long_query = "가" * 201
        with self.assertRaises(ValueError) as ctx:
            parse_related_query(long_query)
        self.assertIn("200자 이내", str(ctx.exception))

    def test_non_string_raises_value_error(self):
        with self.assertRaises(ValueError):
            parse_related_query(12345)  # type: ignore

    def test_nfc_normalization(self):
        import unicodedata
        nfd_text = unicodedata.normalize("NFD", "한글; 계약")
        groups = parse_related_query(nfd_text)
        self.assertEqual(groups[0].terms[0], "한글")
        self.assertEqual(groups[1].terms[0], "계약")

    def test_matches_function(self):
        group = QueryGroup(query="공사 계약", terms=("공사", "계약"))
        self.assertTrue(matches("2026년 공사 계약 체결 완료", group))
        self.assertTrue(matches("계약 관련 공사 내역", group))
        # 한 단어 누락 시 일치하지 않음
        self.assertFalse(matches("2026년 공사 내역 보고", group))
        self.assertFalse(matches("계약 체결 내역", group))
        self.assertFalse(matches("공사 일정. 계약 검토", group))
        self.assertFalse(matches("공사 일정\n계약 검토", group))
        self.assertTrue(matches("공사 계약.pdf", group, single_unit=True))
        # 대소문자 무시
        eng_group = QueryGroup(query="AI Cloud", terms=("AI", "Cloud"))
        self.assertTrue(matches("ai cloud service", eng_group))


if __name__ == "__main__":
    unittest.main()
