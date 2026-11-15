"""한컴오피스 전용 사용자 정의 영역(PUA) 글자를 유니코드 및 표시용 문자로 변환한다."""

from __future__ import annotations

import re

# 1. 사각형 안 숫자: U+F02B1 ~ U+F02C4 (1 ~ 20)
_SQUARED_NUMBERS = {
    chr(0xF02B1 + i - 1): f"[{i}]"
    for i in range(1, 21)
}

# 2. 불릿 및 특수 기호
_BULLETS_AND_SYMBOLS = {
    "\U000F02FA": "·",  # 가운뎃점 / 작은 불릿
    "\U000F02F8": "●",  # 검은 원
    "\U000F02F9": "○",  # 흰 원
    "\U000F02FB": "▪",  # 검은 작은 사각형
    "\U000F02FC": "▫",  # 흰 작은 사각형
    "\U000F02FD": "▸",  # 검은 작은 오른쪽 삼각형
    "\U000F02FE": "▹",  # 흰 작은 오른쪽 삼각형
}

# 3. 한컴 PUA 전체 매핑 사전
HANCOM_PUA_MAP: dict[str, str] = {
    **_SQUARED_NUMBERS,
    **_BULLETS_AND_SYMBOLS,
}

# PUA 판정 정규식 (Plane 15 Supplementary PUA-A, BMP PUA, Plane 16)
_PUA_CHAR_RE = re.compile(
    r"[\uE000-\uF8FF\U000F0000-\U000FFFFD\U00100000-\U0010FFFD]"
)


def has_pua_chars(text: str) -> bool:
    """문자열에 PUA(사용자 정의 영역) 글자가 포함되어 있는지 검사한다."""
    return bool(_PUA_CHAR_RE.search(text))


def replace_pua_chars(text: str) -> str:
    """텍스트 내 한컴 PUA 문자를 표시용 표준 유니코드/마커로 변환한다.
    
    매핑에 등록되지 않은 PUA 문자는 가운뎃점(·) 등으로 기본 대체하여
    미리보기나 검색 결과에서 ☒(글꼴 결측 문자)로 깨지는 것을 막는다.
    """
    if not text or not has_pua_chars(text):
        return text

    def _replace(match: re.Match) -> str:
        ch = match.group(0)
        return HANCOM_PUA_MAP.get(ch, "·")

    return _PUA_CHAR_RE.sub(_replace, text)
