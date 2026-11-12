"""kfind-core 규칙의 Python 기준 구현과 Rust 시험이 읽는 대조 표가 서로 맞는지 본다(호스트 경로·COM 없음).

Rust 쪽은 `cargo test --manifest-path kfind/core/Cargo.toml`이 같은 표(kfind/core/tests/data)를 읽는다.
"""
import sys
import unicodedata
import unittest
from pathlib import Path

CORE = Path(__file__).resolve().parents[1] / "kfind" / "core"
sys.path.insert(0, str(CORE / "tools"))
import fold_ref  # noqa: E402
import gen_fold_table  # noqa: E402
import oracle  # noqa: E402

SAME_UNICODE = unicodedata.unidata_version == fold_ref.TABLE_UNICODE_VERSION


def from_hex(text):
    return "" if text == "-" else "".join(chr(int(h, 16)) for h in text.split(" "))


def rows(name):
    lines = (CORE / "tests" / "data" / name).read_text(encoding="utf-8").splitlines()
    return [line.split("\t") for line in lines if line and not line.startswith("#")]


class FoldRuleTests(unittest.TestCase):
    def test_fold_table_matches_reference(self):
        table = rows("fold_cases.tsv")
        self.assertGreaterEqual(len(table), 30)
        for source, expected, note in table:
            self.assertEqual(fold_ref.fold(from_hex(source)), from_hex(expected), note)

    def test_composed_and_decomposed_hangul_are_equal(self):
        composed = "예산 집행 계획"
        decomposed = unicodedata.normalize("NFD", composed)
        self.assertNotEqual(composed, decomposed)
        self.assertEqual(fold_ref.fold(composed), fold_ref.fold(decomposed))
        self.assertEqual(fold_ref.parse_query(composed), fold_ref.parse_query(decomposed))

    def test_tokens_concat_equals_fold(self):
        for text in ["예산  집행\t계획　수립", " a b ", "Café X", "", "가\x1c나"]:
            self.assertEqual("".join(fold_ref.tokens(text)), fold_ref.fold(text))
        # U+001C는 Python str.split()에는 공백이지만 규칙의 공백(White_Space)이 아니다.
        self.assertEqual(fold_ref.tokens("가\x1c나"), ["가나"])

    def test_parse_query_drops_short_words(self):
        self.assertEqual(fold_ref.parse_query("예산 및 집행 예산"), (["예산", "집행"], ["및"]))
        self.assertEqual(fold_ref.parse_query("ㅇ"), ([], ["ㅇ"]))

    def test_rule_version_matches_rust_table(self):
        text = (CORE / "src" / "fold_table.rs").read_text(encoding="utf-8")
        self.assertIn(f"pub const FOLD_RULE_VERSION: u32 = {fold_ref.FOLD_RULE_VERSION};", text)
        self.assertIn(f'pub const UNICODE_VERSION: &str = "{fold_ref.TABLE_UNICODE_VERSION}";', text)

    @unittest.skipUnless(SAME_UNICODE, "이 Python의 유니코드 판이 고정표 판과 다르다")
    def test_generated_tables_are_current(self):
        self.assertEqual((CORE / "src" / "fold_table.rs").read_text(encoding="utf-8"), gen_fold_table.table_rs())
        self.assertEqual((CORE / "tests" / "data" / "fold_cases.tsv").read_text(encoding="utf-8"), gen_fold_table.cases_tsv())


class OracleTests(unittest.TestCase):
    def test_split_rules(self):
        vocab = {"예산", "예산집", "집행", "예산집행", "행계획", "계획", "주민센터", "주민", "센터", "프로그램"}
        self.assertEqual(oracle.split_bigram("예산을집행"), ["예산", "을집행"])
        self.assertIsNone(oracle.split_bigram("예산을"))
        self.assertEqual(oracle.split_dict("예산집행계획", vocab), ["예산집행", "계획"])
        self.assertEqual(oracle.split_deep("예산집행계획", vocab), ["예산", "집행", "계획"])
        self.assertEqual(oracle.split_deep("주민센터프로그램", vocab), ["주민", "센터", "프로그램"])
        self.assertIsNone(oracle.split_deep("프로그램", vocab))
        self.assertIsNone(oracle.split_deep("예산을집행", vocab))

    def test_split_accepts_stems_of_inflected_words(self):
        # KB3: 조각 낱말이 조사·어미가 붙은 꼴로만 색인에 있어도 줄기로 알아본다(Rust와 같은 꼬리 목록 파일).
        vocab = {"예산을", "집행한다", "계획의", "집행부"}
        self.assertEqual(oracle.split_deep("예산집행", vocab), ["예산", "집행"])
        self.assertEqual(oracle.split_deep("예산집행계획", vocab), ["예산", "집행", "계획"])
        self.assertEqual(oracle.split_deep("집행부예산", vocab), ["집행부", "예산"])
        self.assertIsNone(oracle.split_deep("행부예산", vocab))
        self.assertIn("을", oracle.STEM_SUFFIXES)
        self.assertIn("한다", oracle.STEM_SUFFIXES)

    def test_broad_match_and_adjacent_first(self):
        docs = [
            {"doc": 1, "date": 1, "sents": [(0, 8, "예산 편성과 집행 계획을 세운다.")]},
            {"doc": 2, "date": 1, "sents": [(0, 8, "예산집행 결과 보고."), (1, 8, "집행 잔액과 예산 현황.")]},
            {"doc": 3, "date": 1, "sents": [(0, 1, "예산을 본다."), (1, 1, "집행을 본다.")]},
        ]
        found = oracle.Oracle(docs).search("예산집행", "dict")
        self.assertEqual(found["words"], "예산집행=예산+집행")
        # 붙어 있는 일치가 있는 문서 2가 앞, 떨어져 있는 문서 1이 뒤. 문장이 다른 문서 3은 빠진다.
        self.assertEqual(found["top"], "2:1:8:2;1:0:8:1")
        self.assertEqual((found["sentences"], found["docs"], found["adjacent_docs"]), (3, 2, 1))
        self.assertEqual(oracle.Oracle(docs).search("예산집행", "off")["top"], "2:1:8:1")
        # 비교형은 공백을 지우므로 "예산 집행"처럼 띄어 썼어도 이웃해 있으면 붙어 있는 일치다(Q5).
        spaced = [{"doc": 4, "date": 1, "sents": [(0, 8, "예산 집행 계획을 세운다.")]}]
        self.assertEqual(oracle.Oracle(spaced).search("예산집행", "off")["top"], "4:1:8:1")

    @unittest.skipUnless(SAME_UNICODE, "이 Python의 유니코드 판이 고정표 판과 다르다")
    def test_expected_tables_are_current(self):
        for path, text in oracle.outputs().items():
            self.assertEqual(path.read_text(encoding="utf-8"), text, path.name)


if __name__ == "__main__":
    unittest.main()
