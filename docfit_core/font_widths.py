"""글꼴별 글자 폭 표로 문장 폭을 계산한다(INDENTW1). 한/글·글꼴 파일 없이 동작한다.

표는 ``resources/font_widths.json``(생성: ``scripts/make_font_width_table.py``)이다.
폭(HWPUNIT) = Σ 글자 (크기×상대크기) × (글자폭/units_per_em × 장평 + 자간)
이며 글꼴·장평·자간·상대크기는 글자가 속한 언어 칸의 값을 쓴다.

- 빈칸: 글꼴에 어울리는 빈칸(``useFontSpace``)이 꺼져 있으면 글꼴과 관계없이 0.5em, 켜져 있으면 글꼴의 빈칸 폭.
- 표 없는 글꼴(또는 아직 측정하지 않은 HFT)을 쓰는 글자가 하나라도 있으면 ``None``('모름').
- 글꼴에 없는 글자: 전각 문자(East Asian Width W·F·A)는 1em, 그 밖은 0.5em 으로 어림하고
  ``WidthResult.substituted`` 에 담는다. 한/글의 실제 대체 글꼴 폭과 다를 수 있으므로
  정확한 값이 필요하면 ``exact`` 를 확인한다.
- 탭·줄바꿈 등 제어 문자는 위치에 따라 폭이 달라 ``None``.
"""
from __future__ import annotations

import json
import unicodedata
from bisect import bisect_right
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Iterable, Mapping

DATA_PATH = Path(__file__).resolve().parents[1] / "resources" / "font_widths.json"
LANGUAGES = ("hangul", "latin", "hanja", "japanese", "other", "symbol", "user")
FIXED_SPACE_EM = 0.5

# 코드 구역 → 글자 모양의 언어 칸. 어느 구역에도 없으면 other.
_LANGUAGE_BLOCKS = (
    (0x0000, 0x007F, "latin"),
    (0x00A0, 0x00BF, "symbol"),   # · 등
    (0x00D7, 0x00D7, "symbol"),
    (0x00F7, 0x00F7, "symbol"),
    (0x1100, 0x11FF, "hangul"),
    (0x2000, 0x2BFF, "symbol"),
    (0x3000, 0x303F, "symbol"),
    (0x3040, 0x30FF, "japanese"),
    (0x3130, 0x318F, "hangul"),   # ㅇ ㅁ
    (0x31F0, 0x31FF, "japanese"),
    (0x3200, 0x33FF, "symbol"),
    (0x3400, 0x4DBF, "hanja"),
    (0x4E00, 0x9FFF, "hanja"),
    (0xA960, 0xA97F, "hangul"),
    (0xAC00, 0xD7FF, "hangul"),
    (0xE000, 0xF8FF, "user"),
    (0xF900, 0xFAFF, "hanja"),
    (0xFF00, 0xFFEF, "symbol"),
)


@dataclass(frozen=True)
class LangFont:
    """글자 모양의 언어 칸 하나."""

    face: str
    font_type: str = "TTF"
    ratio: int = 100      # 장평(%)
    spacing: int = 0      # 자간(%)
    rel_size: int = 100   # 상대 크기(%)


@dataclass(frozen=True)
class CharStyle:
    height: int                       # 글자 크기(HWPUNIT, 1pt = 100)
    fonts: Mapping[str, LangFont]     # 언어 칸 → LangFont
    use_font_space: bool = False
    bold: bool = False

    @classmethod
    def uniform(cls, face: str, size_pt: float = 15.0, *, font_type: str = "TTF", ratio: int = 100,
                spacing: int = 0, rel_size: int = 100, use_font_space: bool = False, bold: bool = False):
        """모든 언어 칸이 같은 글자 모양."""
        slot = LangFont(face, font_type, ratio, spacing, rel_size)
        return cls(round(size_pt * 100), {lang: slot for lang in LANGUAGES}, use_font_space, bold)


@dataclass(frozen=True)
class WidthResult:
    width: float                        # HWPUNIT(반올림 전)
    substituted: tuple[str, ...] = ()   # 글꼴에 없어 어림한 글자

    @property
    def exact(self) -> bool:
        return not self.substituted

    @property
    def hwpunit(self) -> int:
        return max(0, round(self.width))


class _Widths:
    def __init__(self, data: dict):
        self.units = data["units_per_em"]
        self.ranges = data["ranges"]
        self.starts = [item[0] for item in self.ranges]

    def em(self, ch: str) -> float | None:
        code = ord(ch)
        index = bisect_right(self.starts, code) - 1
        if index < 0 or code > self.ranges[index][1]:
            return None
        return self.ranges[index][2] / self.units


@lru_cache(maxsize=1)
def _tables() -> dict[tuple[str, str], tuple[_Widths, _Widths | None]]:
    """(글꼴 이름 또는 별칭, 형식) → (보통, 굵게). 측정 필요 항목과 표 파일이 없는 경우는 담지 않는다."""
    try:
        data = json.loads(DATA_PATH.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    result = {}
    for font in data["fonts"]:
        if font["status"] not in ("ok", "measured"):
            continue
        widths = (_Widths(font), _Widths(font["bold"]) if "bold" in font else None)
        for name in (font["face"], *font.get("aliases", ())):
            result[(name, font["type"])] = widths
    return result


def has_table(face: str, font_type: str = "TTF") -> bool:
    return (face, font_type.upper()) in _tables()


def language_of(ch: str) -> str:
    code = ord(ch)
    for first, last, language in _LANGUAGE_BLOCKS:
        if first <= code <= last:
            return language
    return "other"


def _char_width(ch: str, style: CharStyle) -> tuple[float, bool] | None:
    """(폭 HWPUNIT, 어림 여부). 글꼴을 모르면 None."""
    if unicodedata.category(ch) == "Cc":
        return None
    slot = style.fonts.get(language_of(ch))
    if slot is None:
        return None
    tables = _tables().get((slot.face, slot.font_type.upper()))
    if tables is None:
        return None
    widths = tables[1] if style.bold and tables[1] is not None else tables[0]
    substituted = False
    if ch == " " and not style.use_font_space:
        em = FIXED_SPACE_EM
    else:
        em = widths.em(ch)
        if em is None:
            em = 1.0 if unicodedata.east_asian_width(ch) in ("W", "F", "A") else 0.5
            substituted = True
    size = style.height * slot.rel_size / 100.0
    return size * (em * slot.ratio / 100.0 + slot.spacing / 100.0), substituted


def measure_runs(runs: Iterable[tuple[str, CharStyle]], offset: int | None = None) -> WidthResult | None:
    """(문장 조각, 글자 모양) 목록의 첫 글자부터 ``offset`` 글자까지(없으면 전부)의 폭. 모르면 None."""
    total = 0.0
    substituted: list[str] = []
    remaining = offset
    for text, style in runs:
        part = text if remaining is None else text[:max(0, remaining)]
        for ch in part:
            measured = _char_width(ch, style)
            if measured is None:
                return None
            total += measured[0]
            if measured[1] and ch not in substituted:
                substituted.append(ch)
        if remaining is not None:
            remaining -= len(part)
    return WidthResult(total, tuple(substituted))


def measure_text(text: str, style: CharStyle, offset: int | None = None) -> WidthResult | None:
    return measure_runs(((text, style),), offset)


def _local(element) -> str:
    return element.tag.rsplit("}", 1)[-1]


def fontfaces_from_header(header) -> dict[str, dict[str, tuple[str, str]]]:
    """HWPX header.xml 루트 → {언어 칸: {글꼴 id: (이름, 형식)}}."""
    return {
        group.get("lang", "").lower(): {
            font.get("id"): (font.get("face", ""), font.get("type", "TTF"))
            for font in group if _local(font) == "font"
        }
        for group in header.iter() if _local(group) == "fontface"
    }


def style_from_char_pr(char_pr, fontfaces: Mapping[str, Mapping[str, tuple[str, str]]]) -> CharStyle:
    """HWPX ``hh:charPr`` → CharStyle. 글꼴 참조를 찾지 못한 언어 칸은 비워 '모름'이 되게 한다."""
    children = {_local(child): child for child in char_pr}

    def value(name: str, language: str, default: int) -> int:
        element = children.get(name)
        return default if element is None else int(element.get(language, default))

    reference = children.get("fontRef")
    fonts = {}
    for language in LANGUAGES:
        found = None if reference is None else fontfaces.get(language, {}).get(reference.get(language))
        if found is not None:
            fonts[language] = LangFont(found[0], found[1], value("ratio", language, 100),
                                       value("spacing", language, 0), value("relSz", language, 100))
    return CharStyle(int(char_pr.get("height", "1000")), fonts,
                     char_pr.get("useFontSpace", "0") == "1", "bold" in children)
