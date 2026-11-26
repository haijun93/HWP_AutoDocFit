"""글꼴별 글자 폭 표로 문장 폭을 계산한다(INDENTW1). 한/글·글꼴 파일 없이 동작한다.

표는 ``resources/font_widths.json``(생성: ``scripts/make_font_width_table.py``)이다.
결과는 한/글의 ``ParagraphShapeIndentAtCaret``(왼쪽 정렬 첫 줄) 실측에 맞춘 규칙이다(INDENTW2).
한/글은 글자마다 4 HWPUNIT 격자로 폭을 정수화한 뒤 더한다.

    s    = floor(크기×상대크기 / 4)
    폭   = round(s × 글자폭em)                      ← 장평 100%
         = floor(s × 글자폭em × 장평)               ← 장평이 100%가 아니면(중간 반올림 없음)
         = round(floor(s × 0.5) × 장평)             ← 고정 빈칸
    자간 = round(폭 × 자간%)                         ← 글자 크기가 아니라 장평 적용 폭의 %
    글자 = (폭 + 자간) × 4

글꼴·장평·자간·상대크기는 글자가 속한 언어 칸의 값을 쓴다.

- 빈칸(``useFontSpace`` 꺼짐, 0.5em)·고정폭 빈칸(0.25em)은 크기·장평은 **한글 칸**, 자간은 **영문 칸**을 쓴다.
  묶음 빈칸(0.5em)은 한글 칸의 크기·장평만 쓰고 자간이 없다.
  ``useFontSpace`` 가 켜진 보통 빈칸은 영문 칸의 글자 모양과 글꼴 빈칸 폭을 쓴다.
- 고정폭 빈칸(``hp:fwSpace``)은 ``FW_SPACE``, 묶음 빈칸(``hp:nbSpace``)은 ``NB_SPACE`` 문자로 넘긴다.
- 굵게는 TTF 글자 폭을 바꾸지 않는다(예외: 일부 글꼴의 글꼴 빈칸). HFT 는 넓어진다. 달라지는 폭은 표의
  ``bold`` 에 실측해 두었다.
- 표 없는 글꼴을 쓰는 글자가 하나라도 있으면 ``None``('모름').
- 글꼴에 없는 글자: 한/글 대체 폭을 실측해 둔 것은 그 값을 쓴다. 실측이 없으면 전각 문자(East Asian Width
  W·F·A)는 1em, 그 밖은 0.5em 으로 어림하고 ``WidthResult.substituted`` 에 담는다.
- 탭·줄바꿈 등 제어 문자는 위치에 따라 폭이 달라 ``None``.
- **양쪽 정렬**로 첫 줄이 늘어나는 문단은 ``justified_indent`` 를 쓴다(문단 전체 글과 첫 줄 폭이 필요).
"""
from __future__ import annotations

import json
import math
import unicodedata
from bisect import bisect_right
from dataclasses import dataclass, replace
from functools import lru_cache
from pathlib import Path
from typing import Iterable, Mapping

DATA_PATH = Path(__file__).resolve().parents[1] / "resources" / "font_widths.json"
LANGUAGES = ("hangul", "latin", "hanja", "japanese", "other", "symbol", "user")
FIXED_SPACE_EM = 0.5
FW_SPACE = ""    # 고정폭 빈칸(hp:fwSpace, 한/글 문자 코드 31)
NB_SPACE = ""    # 묶음 빈칸(hp:nbSpace, 한/글 문자 코드 30)
FW_SPACE_EM = 0.25
_UNIT = 4            # 한/글 조판 격자(HWPUNIT)

# 코드 구역 → 글자 모양의 언어 칸. 어느 구역에도 없으면 other.
_LANGUAGE_BLOCKS = (
    (0x0000, 0x007F, "latin"),
    (0x0080, 0x00FF, "latin"),    # · ± × § 등
    (0x0370, 0x04FF, "symbol"),   # 그리스·키릴
    (0x1100, 0x11FF, "hangul"),
    (0x2018, 0x2019, "latin"),    # ‘’
    (0x201C, 0x201D, "latin"),    # “”
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
    width: float                        # HWPUNIT
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
def _tables() -> dict[tuple[str, str], tuple[_Widths | None, _Widths | None, _Widths | None]]:
    """(글꼴 이름 또는 별칭, 형식) → (기본 폭, TTF 실측 폭, 굵게 실측 폭). 실측 폭이 기본 폭에 앞선다.

    측정 필요 항목과 표 파일이 없는 경우는 담지 않는다.
    """
    try:
        data = json.loads(DATA_PATH.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    result = {}
    for font in data["fonts"]:
        if font["status"] not in ("ok", "measured"):
            continue
        widths = tuple(_Widths(part) if part and "ranges" in part else None
                       for part in (font, font.get("measured"), font.get("bold")))
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
    if ch not in (FW_SPACE, NB_SPACE) and unicodedata.category(ch) == "Cc":
        return None
    fixed_space = ch in (FW_SPACE, NB_SPACE) or (ch == " " and not style.use_font_space)
    slot = style.fonts.get("hangul" if fixed_space else language_of(ch))
    if slot is None:
        return None
    tables = _tables().get((slot.face, slot.font_type.upper()))
    if tables is None:
        return None
    substituted = False
    if fixed_space:
        em = FW_SPACE_EM if ch == FW_SPACE else FIXED_SPACE_EM
        latin = style.fonts.get("latin")
        if latin is not None:
            slot = replace(slot, spacing=latin.spacing)
    else:
        order = (tables[2] if style.bold else None, tables[1], tables[0])
        em = next((found for table in order if table is not None
                   for found in (table.em(ch),) if found is not None), None)
        if em is None:
            em = 1.0 if unicodedata.east_asian_width(ch) in ("W", "F", "A") else 0.5
            substituted = True
    return hangul_char_width(ch, em, style, slot), substituted


def _floor(value: float) -> int:
    return math.floor(value + 1e-7)


def _round(value: float) -> int:
    """절댓값 기준 반올림(0.5 는 0 에서 먼 쪽)."""
    return int(math.copysign(math.floor(abs(value) + 0.5 + 1e-7), value))


def hangul_char_width(ch: str, em: float, style: CharStyle, slot: LangFont) -> int:
    """한/글 방식의 글자 하나 폭(HWPUNIT, 4의 배수). 모듈 설명의 산식."""
    size = _floor(style.height * slot.rel_size / 100.0 / _UNIT)
    ratio = slot.ratio / 100.0
    if ch == NB_SPACE:                       # 자간을 적용하지 않는다
        return _floor(_floor(size * ratio) * em) * _UNIT
    if ch == FW_SPACE:
        width = _floor(_floor(size * ratio) * em)
    else:
        if ch == " " and not style.use_font_space:
            width = _round(_floor(size * em) * ratio)
        elif slot.ratio == 100:
            width = _round(size * em)
        else:
            width = _floor(size * em * ratio)
    return (width + _round(width * slot.spacing / 100.0)) * _UNIT


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


# 줄 머리·줄 끝에 올 수 없는 문자(금칙). 대조에 쓴 범위만 담는다.
_NO_LINE_START = frozenset(".,:;!?)]}%·」』〕》〉】’”。、，．）］｝：；！？…")
_NO_LINE_END = frozenset("([{「『〔《〈【‘“（［｛")


def _without_spacing(style: CharStyle) -> CharStyle:
    return replace(style, fonts={language: replace(font, spacing=0) for language, font in style.fonts.items()})


def _breakable(text: str, index: int, korean_by_word: bool) -> bool:
    """``text[index]`` 앞에서 줄을 나눌 수 있는가."""
    before, after = text[index - 1], text[index]
    if after in (" ", NB_SPACE) or before == NB_SPACE:
        return False
    if before in (" ", FW_SPACE):
        return True
    if after in _NO_LINE_START or before in _NO_LINE_END or korean_by_word:
        return False
    # 영문·숫자 낱말은 붙여 둔다. 한글 등 전각 문자 앞뒤에서는 나눌 수 있다.
    return any(unicodedata.east_asian_width(ch) in ("W", "F") for ch in (before, after))


def justified_indent(runs: Iterable[tuple[str, CharStyle]], offset: int, line_width: int,
                     *, korean_by_word: bool = False) -> WidthResult | None:
    """양쪽 정렬 문단에서 ``offset`` 글자 앞까지의 폭(첫 줄이 늘어난 몫 포함).

    ``runs`` 는 문단 **전체**, ``line_width`` 는 첫 줄 폭(HWPUNIT, 본문 폭 − 왼쪽·오른쪽 여백)이다.
    ``korean_by_word`` 는 한글 줄 나눔 기준이 어절이면 True(HWPX ``breakNonLatinWord="BREAK_WORD"``),
    글자면 False(``"KEEP_WORD"``).

    첫 줄에 다 들어가면 늘어나지 않는다. 늘어나면 남는 폭을 문두 빈칸을 뺀 첫 줄 안의 빈칸에 똑같이 나눈다.
    탭·모르는 글꼴, 첫 줄에 늘릴 빈칸이 없는 경우(글자 사이가 벌어짐, 규칙 미확인), ``offset`` 이 첫 줄 밖이면
    ``None``. 문단 모양의 최소 빈칸(condense)으로 줄을 줄이는 경우는 다루지 않는다.
    """
    chars = [(ch, style) for text, style in runs for ch in text]
    text = "".join(ch for ch, _ in chars)
    widths: list[int] = []
    substituted: list[str] = []
    spacings: list[int] = []                          # 글자마다 자간이 차지한 몫(줄 끝 글자는 빼고 맞춘다)
    total, end = 0, None
    for index, (ch, style) in enumerate(chars):
        measured = _char_width(ch, style)
        if measured is None:
            return None
        spacing = measured[0] - _char_width(ch, _without_spacing(style))[0]
        if ch != " " and total + measured[0] - spacing > line_width:
            end = index
            break
        total += measured[0]
        widths.append(measured[0])
        spacings.append(spacing)
        if measured[1] and ch not in substituted:
            substituted.append(ch)
    if end is None:                                   # 한 줄 문단
        return WidthResult(sum(widths[:offset]), tuple(substituted))
    breaks = [i for i in range(1, end + 1) if _breakable(text, i, korean_by_word)]
    line = text[: breaks[-1] if breaks else end].rstrip(" ")
    if offset >= len(line):
        return None
    leading = len(line) - len(line.lstrip(" "))
    spaces = [i for i in range(leading, len(line)) if line[i] == " "]
    if not spaces:
        return None
    slack = line_width - sum(widths[: len(line)]) + spacings[len(line) - 1]
    stretched = slack * sum(i < offset for i in spaces) / len(spaces)
    return WidthResult(sum(widths[:offset]) + stretched, tuple(substituted))


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
