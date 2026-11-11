"""TXT/MD의 Python 번호 제목 판정 회귀 시험."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_core.timemachine.content_search import ContentSearchIndex, extract_paragraphs
from scripts.hs2_d2_probe import CASES


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

    def test_hangul_sequence(self):
        self.check_family("가. 세부 기준", "나. 추가 기준")

    def test_hangul_before_numeric_and_roman(self):
        self.assertEqual(self.levels(["가. 세부 기준", "1. 추진 계획", "Ⅰ. 추진 배경",
                                      "Ⅱ. 운영 배경", "2. 실행 계획", "나. 추가 기준"]),
                         ["chapter", "section", "topic", "topic", "section", "chapter"])

    def test_exactly_sixty_characters(self):
        self.assertEqual(self.levels(["가. " + "기" * 57, "나. 추가 기준"]), ["chapter", "chapter"])

    def test_three_synthetic_documents_are_indexed_with_python_levels(self):
        expected = {
            "consistent": ["section", "topic", "topic", "topic", "topic", "section"],
            "isolated": ["body"] * 3,
            "body_list": ["body"] * 7,
        }
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            root = Path(tmp)
            index = ContentSearchIndex(root / "content.sqlite")
            try:
                with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                    for name, content in CASES.items():
                        path = root / (name + ".txt")
                        path.write_text(content, encoding="utf-8")
                        self.assertTrue(index.index_file(path)["indexed"])
                        rows = index.db.execute(
                            "SELECT body,level FROM content_paragraph_fts WHERE path=? ORDER BY paragraph_index",
                            (str(path.resolve()),),
                        ).fetchall()
                        # 복원한 장 계열의 미해결 동작을 신규 기대값으로 고정하지 않는다.
                        self.assertEqual([row[1] for row in rows if not row[0].startswith("제")], expected[name])
            finally:
                index.close()
