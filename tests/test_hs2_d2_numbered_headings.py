"""TXT/MD의 Python 번호 제목 판정 회귀 시험."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_core.timemachine.content_search import extract_paragraphs


class NumberedHeadingTests(unittest.TestCase):
    def levels(self, lines, suffix=".txt"):
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            path = Path(tmp) / ("sample" + suffix)
            path.write_text("\n".join(lines), encoding="utf-8")
            with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                return [p.level for p in extract_paragraphs(path)]

    def check_family(self, first, second):
        for suffix in (".txt", ".md"):
            self.assertEqual(self.levels([first, second], suffix), ["chapter", "chapter"])
            self.assertEqual(self.levels([first], suffix), ["body"])
            self.assertEqual(self.levels([second, first], suffix), ["body", "body"])
            self.assertEqual(self.levels([first + "가" * 61, second], suffix), ["body", "body"])
            self.assertEqual(self.levels(["다음 사항을 확인합니다.", first, second], suffix),
                             ["body", "body", "body"])

    def test_roman_sequence(self):
        self.check_family("Ⅰ. 추진 배경", "Ⅱ. 운영 배경")
        self.check_family("I. 추진 배경", "II. 운영 배경")

    def test_numeric_sequence(self):
        self.check_family("1. 추진 계획", "2. 운영 계획")

    def test_numeric_before_roman(self):
        self.assertEqual(self.levels(["1. 추진 계획", "Ⅰ. 추진 배경", "Ⅱ. 운영 배경", "2. 실행 계획"]),
                         ["chapter", "section", "section", "chapter"])
