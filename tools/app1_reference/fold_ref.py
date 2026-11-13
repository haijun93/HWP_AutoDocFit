"""kfind-core 비교형 규칙의 Python 기준 구현(규칙 문서: docs/KC1_KFIND_CORE_2026-11-12.md 1절).

Rust `kfind_core::fold`와 글자 단위로 같아야 한다. 규칙 버전이 바뀌면 `FOLD_RULE_VERSION`을 양쪽에서 함께 올린다.
"""
import re
import unicodedata

FOLD_RULE_VERSION = 1
# 고정표(src/fold_table.rs)를 만든 유니코드 판. 다른 판의 Python에서는 그 뒤 새로 배정된 글자만 다를 수 있다.
TABLE_UNICODE_VERSION = "16.0.0"
# 낱말 구분 공백: 유니코드 White_Space 속성 25자(Rust `char::is_whitespace`와 같은 집합). Python `str.split()`은
# U+001C~001F도 공백으로 보므로 쓰지 않는다.
WHITE_SPACE = "\t\n\x0b\x0c\r \x85\xa0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000"
_SPLIT = re.compile(f"[{WHITE_SPACE}]+")
MIN_WORD_CHARS = 2


def fold(s):
    """비교형: NFC → casefold → 글자·숫자(isalnum)만 남김."""
    return "".join(c for c in unicodedata.normalize("NFC", s).casefold() if c.isalnum())


def tokens(s):
    """공백으로 나눈 낱말의 비교형(빈 것 제외). fold(s) == "".join(tokens(s))."""
    return [f for f in (fold(t) for t in _SPLIT.split(s)) if f]


def parse_query(q):
    """(검색 낱말, 너무 짧아 뺀 낱말). 2글자 미만은 빼고 같은 낱말은 한 번만."""
    words, dropped = [], []
    for f in tokens(q):
        if len(f) < MIN_WORD_CHARS:
            if f not in dropped:
                dropped.append(f)
        elif f not in words:
            words.append(f)
    return words, dropped
