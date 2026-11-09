"""오늘 날짜 넣기: 표기 방식 3가지 × 구분 방식 2가지 × 앞/뒤 2가지 = 12가지 조합 및 호환성 단위 시험."""
import unittest
from datetime import datetime

from docfit_core.file_rename import Entry, apply_rules

FIXED_DATE = datetime(2026, 11, 9)


def rename_file(name, *rules, today=FIXED_DATE):
    entry = Entry("1", "C:/test/" + name, 1, 0.0, 0.0)
    return apply_rules([entry], list(rules), today=today)[0][0]


class RenameDateFormatTests(unittest.TestCase):
    """3표기 × 2구분 × 앞/뒤 = 12경우 단위 시험."""

    def test_12_combinations(self):
        cases = [
            # 1~4: YYYY.MM.DD (2026.11.09)
            ("YYYY.MM.DD", "_", "prefix", "2026.11.09_본문.hwpx"),
            ("YYYY.MM.DD", "_", "suffix", "본문_2026.11.09.hwpx"),
            ("YYYY.MM.DD", "()", "prefix", "(2026.11.09)본문.hwpx"),
            ("YYYY.MM.DD", "()", "suffix", "본문(2026.11.09).hwpx"),
            # 5~8: YYMMDD (261109)
            ("YYMMDD", "_", "prefix", "261109_본문.hwpx"),
            ("YYMMDD", "_", "suffix", "본문_261109.hwpx"),
            ("YYMMDD", "()", "prefix", "(261109)본문.hwpx"),
            ("YYMMDD", "()", "suffix", "본문(261109).hwpx"),
            # 9~12: YYYYMMDD (20261109)
            ("YYYYMMDD", "_", "prefix", "20261109_본문.hwpx"),
            ("YYYYMMDD", "_", "suffix", "본문_20261109.hwpx"),
            ("YYYYMMDD", "()", "prefix", "(20261109)본문.hwpx"),
            ("YYYYMMDD", "()", "suffix", "본문(20261109).hwpx"),
        ]

        self.assertEqual(len(cases), 12, "정확히 12가지 경우여야 합니다.")

        for fmt, delim, pos, expected in cases:
            with self.subTest(fmt=fmt, delim=delim, pos=pos):
                rule = {
                    "kind": "date",
                    "source": "today",
                    "format": fmt,
                    "position": pos,
                    "delimiter": delim,
                }
                actual = rename_file("본문.hwpx", rule)
                self.assertEqual(actual, expected)

    def test_legacy_format_compatibility(self):
        """기존 저장된 설정(옛 형식) 호환 시험."""
        # 옛 형식: 끝에 점 있는 형식과 빈 sep
        legacy_prefix = {
            "kind": "date",
            "source": "today",
            "format": "YYYY.MM.DD.",
            "position": "prefix",
            "sep": "",
        }
        self.assertEqual(rename_file("보고서.hwpx", legacy_prefix), "2026.11.09.보고서.hwpx")

        legacy_suffix = {
            "kind": "date",
            "source": "today",
            "format": "YYYY.MM.DD",
            "position": "suffix",
            "sep": "",
        }
        self.assertEqual(rename_file("보고서.hwpx", legacy_suffix), "보고서2026.11.09.hwpx")

        legacy_yymmdd = {
            "kind": "date",
            "source": "today",
            "format": "YYMMDD",
            "position": "suffix",
            "sep": "",
        }
        self.assertEqual(rename_file("보고서.hwpx", legacy_yymmdd), "보고서261109.hwpx")

    def test_delimiter_bracket_or_underscore(self):
        """delimiter 옵션으로 밑줄 및 괄호 처리 확인."""
        rule_under = {"kind": "date", "source": "today", "format": "YYYYMMDD", "position": "prefix", "delimiter": "_"}
        self.assertEqual(rename_file("문서.hwpx", rule_under), "20261109_문서.hwpx")

        rule_paren = {"kind": "date", "source": "today", "format": "YYYYMMDD", "position": "prefix", "delimiter": "()"}
        self.assertEqual(rename_file("문서.hwpx", rule_paren), "(20261109)문서.hwpx")


if __name__ == "__main__":
    unittest.main()
