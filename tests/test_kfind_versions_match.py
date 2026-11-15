"""Python 검색 클라이언트가 기대하는 판 번호 = Rust kfind_core 상수(2026-11-15 재발 방지).

v2.2.67에서 색인 형식이 3→4로 바뀌었는데 Python 쪽 기대값은 3으로 남아, 앱의 모든 검색 서비스 요청이
version_mismatch로 실패했다(앱 검색은 옛 방식으로만, 업데이트 뒤 색인 복구도 안 됨). 형식을 바꾸면 이 시험이 막는다.
"""
import re
import unittest
from pathlib import Path

from docfit_app.kfind_client import EXPECTED_VERSIONS

ROOT = Path(__file__).resolve().parents[1]


def _const(path, name):
    text = (ROOT / path).read_text(encoding="utf-8")
    match = re.search(rf"pub const {name}: u32 = (\d+);", text)
    assert match, f"{name} 상수를 {path}에서 찾지 못함"
    return int(match.group(1))


class KfindVersionsMatchTests(unittest.TestCase):
    def test_python_expected_versions_equal_rust_constants(self):
        rust = {
            "format": _const("kfind/core/src/segment.rs", "FORMAT_VERSION"),
            "fold_rule": _const("kfind/core/src/fold_table.rs", "FOLD_RULE_VERSION"),
            "sentence_rule": _const("kfind/core/src/sentence.rs", "SENTENCE_RULE_VERSION"),
        }
        self.assertEqual(EXPECTED_VERSIONS, rust)


if __name__ == "__main__":
    unittest.main()
