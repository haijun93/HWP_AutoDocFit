"""폭 표 내어쓰기를 HWPX 에 직접 쓰고, Shift+Tab 단계에서 건너뛸 문단을 가린다(INDENTW3).

폭 계산은 ``font_widths``. 여기서는 INDENTW2 가 0.5pt 이내를 확인한 조건인지 **가려내기만** 한다.
하나라도 확인하지 않은 조건이면 그 문단은 표를 쓰지 않고 기존 한/글 Shift+Tab 방식에 맡긴다.

- 원본: HWP 문서는 변환된 작업용 HWPX 를 확인한 경우만 허용한다(INDENTW4).
- 환경: 한/글 판과 글꼴 파일이 표를 만들 때(``resources/font_widths_env.json``)와 같아야 한다.
- 문단: 본문 최상위의 글자만 있는 문단(표 칸·글상자·개체·탭·고정폭/묶음 빈칸 제외).
- 문단 모양: 왼쪽·양쪽 정렬, 최소 빈칸 0, 번호·글머리표 없음, 영문 낱말 유지, 자동 간격 없음.
- 글자 모양: 확인한 글꼴·크기·장평·자간·상대크기, 커닝·위치·첨자·외곽선 등 효과 없음.
- 구역: 세로 쪽, 제본 여백 없음, 1단, 가로쓰기, 줄·글자 격자 없음.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Callable

from defusedxml import ElementTree as ET

from . import font_widths as fw
from . import hangul_fonts
from .attachment_block import find_trailing_numbered_blocks
from .batch_format import ParagraphStyle, StylePool, _serialize
from .fidelity.package import PackageSnapshot, build_package

SETTING_KEY = "indent_table"
# INDENTW5: 14문단은 저장 비용을 회수하지 못했고, 59·299문단은 두 회차 모두 이득이었다.
# 전체 문단 수가 아니라 XML 일괄 서식으로 확정한 고유 문단 수를 쓴다.
MIN_FORMATTED_PARAGRAPHS = 59
ENV_PATH = fw.DATA_PATH.with_name("font_widths_env.json")
COM_UNIT = 2                       # ParaShape 여백 값 = HWPUNIT × 2
_OLE_MAGIC = bytes.fromhex("d0cf11e0a1b11ae1")   # HWP 5.0 바이너리 문서의 머리
HEIGHT_RANGE = (1000, 2200)        # 실측 범위: 10~22pt
RATIO_RANGE = (70, 120)
SPACING_RANGE = (-15, 10)
REL_SIZE_RANGE = (90, 110)
_BANNED_CHAR_ELEMENTS = ("italic", "emboss", "engrave", "supscript", "subscript")
_SECTION_NAME = re.compile(r"Contents/section(\d+)\.xml")

OffsetFunction = Callable[[str], "int | None"]


@dataclass(frozen=True)
class PerformanceDecision:
    paragraphs: int | None
    state: str                 # unmeasured → small | eligible, 문서마다 초기화

    @property
    def use_table(self) -> bool:
        return self.state == "eligible"


def performance_decision(paragraphs: int | None) -> PerformanceDecision:
    """실측상 저장 비용을 회수한 최소 규모. 이 판정은 정확도·환경 검사를 대신하지 않는다."""
    if not isinstance(paragraphs, int) or paragraphs <= 0:
        return PerformanceDecision(paragraphs, "unmeasured")
    return PerformanceDecision(paragraphs, "eligible" if paragraphs >= MIN_FORMATTED_PARAGRAPHS else "small")


# ── 환경(한/글 판·글꼴 파일) ────────────────────────────────

def environment_fingerprint(path: Path = ENV_PATH) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def version_matches(version) -> bool:
    """``hwp.Version``(예: "9, 6, 1, 10778")이 표를 실측한 판과 같은가. 모르면 False."""
    expected = environment_fingerprint().get("hangul")
    if not expected or not isinstance(version, str):
        return False
    return re.findall(r"\d+", version) == re.findall(r"\d+", expected)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def font_file(name: str) -> Path | None:
    """한/글이 쓰는 글꼴 파일. 표 생성 때처럼 Windows 설치본이 한/글 폴더보다 앞선다."""
    for folder in (*hangul_fonts.system_font_dirs(), *hangul_fonts.hangul_font_dirs()):
        candidate = folder / name
        if candidate.is_file():
            return candidate
    return None


def hft_fingerprint() -> str | None:
    """한/글 HFT 글꼴 묶음(hftInfo.dat 내용 + *.HFT 이름·크기)의 지문."""
    for ttf in hangul_fonts.hangul_font_dirs():
        shared = next((parent for parent in (ttf, *ttf.parents) if parent.name.lower() == "shared"), None)
        if shared is None:
            continue
        info = shared / "Fonts" / "hftInfo.dat"
        if not info.is_file():
            continue
        digest = hashlib.sha256(info.read_bytes())
        for item in sorted((shared / "Fonts").iterdir(), key=lambda p: p.name.lower()):
            if item.suffix.lower() == ".hft":
                digest.update(f"{item.name.lower()}:{item.stat().st_size};".encode("utf-8"))
        return digest.hexdigest()
    return None


@lru_cache(maxsize=None)
def font_verified(face: str, font_type: str) -> bool:
    """표에 있고, 설치 글꼴이 표를 만든 판과 같은가."""
    if not fw.has_table(face, font_type):
        return False
    environment = environment_fingerprint()
    try:
        if font_type.upper() == "HFT":
            return bool(environment.get("hft")) and hft_fingerprint() == environment["hft"]
        expected = environment.get("fonts", {}).get(face)
        if not expected:
            return False
        path = font_file(expected["file"])
        return path is not None and _sha256(path) == expected["sha256"]
    except OSError:
        return False


def _hwp_source(path) -> bool:
    """원본이 HWP(바이너리) 문서인가. 확장자만 .hwpx 인 바이너리도 포함한다."""
    if Path(path).suffix.lower() == ".hwp":
        return True
    try:
        with open(path, "rb") as stream:
            return stream.read(8) == _OLE_MAGIC
    except OSError:
        return False


def setting_on(load_settings) -> bool:
    """사용자가 켰는가. 읽지 못하면 False(기존 방식)이며 예외를 올리지 않는다."""
    try:
        return bool(load_settings().get(SETTING_KEY, False))
    except Exception:
        return False


def enabled(hwp, load_settings, source=None, *, converted_hwpx=None) -> bool:
    """설정이 켜져 있고(기본 꺼짐) 열려 있는 한/글이 표를 실측한 판인가. 확인하지 못하면 False(기존 방식).

    ``source`` 는 처리 중인 원본 파일. HWP 는 실제 작업용 ``converted_hwpx`` 패키지가 있을 때만 허용한다.
    바이너리 원본 자체에는 XML 계산을 적용하지 않는다. 글꼴·문단 조건은 ``plan_document`` 가 가린다.
    """
    try:
        if not setting_on(load_settings) or not version_matches(hwp.Version):
            return False
        if source is not None and _hwp_source(source):
            if converted_hwpx is None:
                return False
            # 경로 문자열이나 확장자만으로 HWP 변환이 끝났다고 간주하지 않는다.
            if isinstance(converted_hwpx, PackageSnapshot):
                snapshot = converted_hwpx
            else:
                if _hwp_source(converted_hwpx):
                    return False
                snapshot = PackageSnapshot.load(converted_hwpx)
            if snapshot.format != "hwpx" or "Contents/header.xml" not in snapshot.entries or not any(
                    _SECTION_NAME.fullmatch(name) for name in snapshot.entries):
                return False
        return True
    except Exception:
        return False


# ── 문단 판정 ───────────────────────────────────────────────

def _tag(element) -> str:
    return element.tag.rsplit("}", 1)[-1]


def _child(element, name):
    return next((x for x in element if _tag(x) == name), None)


def _find_all(element, name):
    return [x for x in element.iter() if _tag(x) == name]


@dataclass(frozen=True)
class _Section:
    width: int              # 본문 폭(HWPUNIT)
    wraps_text: bool        # 본문 줄을 좁힐 수 있는 떠 있는 개체가 있다


def _section_info(root) -> _Section | None:
    pages, grids = _find_all(root, "pagePr"), _find_all(root, "grid")
    setups, columns = _find_all(root, "secPr"), _find_all(root, "colPr")
    if len(pages) != 1 or len(setups) != 1 or not grids:
        return None
    page, margin = pages[0], _child(pages[0], "margin")
    if (margin is None or page.get("landscape") != "WIDELY" or int(margin.get("gutter", "0")) != 0
            or setups[0].get("textDirection") != "HORIZONTAL"
            or any(x.get("colCount", "1") != "1" for x in columns)
            or any(x.get("lineGrid", "0") != "0" or x.get("charGrid", "0") != "0" for x in grids)):
        return None
    width = int(page.get("width")) - int(margin.get("left")) - int(margin.get("right"))
    wraps = False
    for element in root.iter():
        position = _child(element, "pos")
        if (position is not None and position.get("treatAsChar") == "0"
                and element.get("textWrap") not in ("TOP_AND_BOTTOM", "BEHIND_TEXT", "IN_FRONT_OF_TEXT")):
            wraps = True
    return _Section(width, wraps)


def paragraph_text(paragraph) -> str | None:
    """글자만 있는 문단의 글. 개체·탭·특수 빈칸 등이 있으면 None."""
    parts = []
    for element in paragraph:
        if _tag(element) == "linesegarray":
            continue
        if _tag(element) != "run":
            return None
        for item in element:
            if _tag(item) != "t" or len(item):
                return None
            parts.append(item.text or "")
    text = "".join(parts)
    return None if any(ord(ch) < 32 for ch in text) else text


@dataclass(frozen=True)
class _ParaShape:
    justify: bool
    korean_by_word: bool
    left: int
    right: int
    intent: int


def _para_shape(shape) -> _ParaShape | None:
    """확인한 조건의 문단 모양이면 값, 아니면 None. 여백은 HWPX case(HWPUNIT) 값."""
    if shape is None:
        return None
    align = _find_all(shape, "align")
    heading = _find_all(shape, "heading")
    breaks = _find_all(shape, "breakSetting")
    spacing = _find_all(shape, "autoSpacing")
    cases = _find_all(shape, "case")
    if (shape.get("condense", "0") != "0" or shape.get("fontLineHeight", "0") != "0"
            or len(align) != 1 or align[0].get("horizontal") not in ("LEFT", "JUSTIFY")
            or any(x.get("type", "NONE") != "NONE" for x in heading)
            or len(breaks) != 1 or breaks[0].get("breakLatinWord") != "KEEP_WORD"
            or breaks[0].get("lineWrap") != "BREAK"
            or breaks[0].get("breakNonLatinWord") not in ("KEEP_WORD", "BREAK_WORD")
            or any(x.get("eAsianEng", "0") != "0" or x.get("eAsianNum", "0") != "0" for x in spacing)):
        return None
    margin = next((m for case in cases for m in _find_all(case, "margin")), None)
    if margin is None:
        return None
    values = {_tag(x): int(x.get("value", "0")) for x in margin if x.get("unit", "HWPUNIT") == "HWPUNIT"}
    if not {"left", "right", "intent"} <= values.keys():
        return None
    return _ParaShape(align[0].get("horizontal") == "JUSTIFY",
                      breaks[0].get("breakNonLatinWord") == "BREAK_WORD",
                      values["left"], values["right"], values["intent"])


def _within(element, low: int, high: int, default: int) -> bool:
    if element is None:
        return True
    return all(low <= int(element.get(lang, default)) <= high for lang in fw.LANGUAGES)


def _char_style(char_pr, fontfaces) -> fw.CharStyle | None:
    """확인한 조건의 글자 모양이면 CharStyle, 아니면 None."""
    if char_pr is None:
        return None
    children = {_tag(x): x for x in char_pr}
    offset = children.get("offset")
    underline, strike = children.get("underline"), children.get("strikeout")
    if (not HEIGHT_RANGE[0] <= int(char_pr.get("height", "1000")) <= HEIGHT_RANGE[1]
            or char_pr.get("useKerning", "0") != "0" or char_pr.get("symMark", "NONE") != "NONE"
            or any(name in children for name in _BANNED_CHAR_ELEMENTS)
            or any(name in children and children[name].get("type", "NONE") != "NONE"
                   for name in ("outline", "shadow"))
            or (underline is not None and underline.get("type", "NONE") != "NONE")
            or (strike is not None and strike.get("shape", "NONE") != "NONE")
            or (offset is not None and any(int(offset.get(lang, "0")) for lang in fw.LANGUAGES))
            or not _within(children.get("ratio"), *RATIO_RANGE, 100)
            or not _within(children.get("spacing"), *SPACING_RANGE, 0)
            or not _within(children.get("relSz"), *REL_SIZE_RANGE, 100)):
        return None
    return fw.style_from_char_pr(char_pr, fontfaces)


def _fonts_verified(runs) -> bool:
    for text, style in runs:
        languages = {fw.language_of(ch) for ch in text}
        if " " in text:
            languages.add("latin" if style.use_font_space else "hangul")
        for language in languages:
            slot = style.fonts.get(language)
            if slot is None or not font_verified(slot.face, slot.font_type):
                return False
    return True


def indent_width(runs, offset: int, line_width: int, *, justify: bool, korean_by_word: bool) -> int | None:
    """``offset`` 글자 앞까지의 폭(HWPUNIT). 0.5pt 이내를 확인하지 못한 경우는 None."""
    text = "".join(part for part, _ in runs)
    if offset <= 0 or offset >= len(text) or not _fonts_verified(runs):
        return None
    if justify:
        result = fw.justified_indent(runs, offset, line_width, korean_by_word=korean_by_word)
    else:
        result = fw.measure_runs(runs, offset)
        # 왼쪽 정렬: 기준 위치 뒤 첫 낱말까지 첫 줄에 들어가는 것이 확실할 때만(넘치면 줄 위치를 모른다).
        end = text.find(" ", offset + 1)
        head = fw.measure_runs(runs, len(text) if end < 0 else end)
        if head is None or head.width > line_width:
            return None
    if result is None or not result.exact or result.hwpunit <= 0:
        return None
    return result.hwpunit


# ── 문서 ────────────────────────────────────────────────────

@dataclass
class Plan:
    section: str
    paragraph: object       # XML 요소
    text: str
    left: int               # 써야 할 왼쪽 여백(HWPUNIT)
    intent: int             # 써야 할 첫 줄 값(HWPUNIT, 음수)
    current: tuple          # 지금 (left, intent)


@dataclass
class Document:
    snapshot: PackageSnapshot
    header: object
    sections: dict
    plans: dict             # 본문 문단 번호(구역을 이어 센 값, 한/글 SetPos(0, n)) → Plan


def plan_document(path: str | Path | PackageSnapshot, offset_of: OffsetFunction) -> Document:
    """HWPX 에서 표 계산을 쓸 수 있는 문단과 써야 할 값을 구한다. 파일은 바꾸지 않는다."""
    # 한 저장본의 변환 확인과 계획에서 같은 불변 바이트를 쓴다. 단계 사이 캐시는 아니다.
    snapshot = path if isinstance(path, PackageSnapshot) else PackageSnapshot.load(path)
    names = sorted((n for n in snapshot.entries if _SECTION_NAME.fullmatch(n)),
                   key=lambda n: int(_SECTION_NAME.fullmatch(n).group(1)))
    header = ET.fromstring(snapshot.read("Contents/header.xml"))
    fontfaces = fw.fontfaces_from_header(header)
    chars = {x.get("id"): x for x in _find_all(header, "charPr")}
    paras = {x.get("id"): x for x in _find_all(header, "paraPr")}
    sections = {n: ET.fromstring(snapshot.read(n)) for n in names}
    body = [(n, p) for n in names for p in sections[n] if _tag(p) == "p"]
    texts = ["".join("".join(t.itertext()) for run in p if _tag(run) == "run" for t in run if _tag(t) == "t")
             for _, p in body]
    # 붙임 번호 묶음은 앞 번호 위치에 맞추는 별도 규칙(왼쪽 여백까지 바꿈)이라 건드리지 않는다.
    attachments = {i for start, end in find_trailing_numbered_blocks(texts) for i in range(start, end + 1)}
    infos = {n: _section_info(sections[n]) for n in names}
    styles: dict = {}
    shapes: dict = {}
    plans = {}
    for index, (name, paragraph) in enumerate(body):
        info = infos[name]
        text = paragraph_text(paragraph)
        if info is None or index in attachments or not text or not text.strip():
            continue
        offset = offset_of(text)
        ref = paragraph.get("paraPrIDRef")
        if ref not in shapes:
            shapes[ref] = _para_shape(paras.get(ref))
        shape = shapes[ref]
        if offset is None or offset <= 0 or shape is None or (shape.justify and info.wraps_text):
            continue
        runs = []
        for run in paragraph:
            if _tag(run) != "run":
                continue
            piece = "".join(t.text or "" for t in run)
            if not piece:
                continue
            ref = run.get("charPrIDRef")
            if ref not in styles:
                styles[ref] = _char_style(chars.get(ref), fontfaces)
            runs.append((piece, styles[ref]))
        if any(style is None for _, style in runs):
            continue
        # 앱의 Shift+Tab 경로와 같은 왼쪽 여백 규칙: 그대로 두되 (여백 = -내어쓰기) 쌍은 0.
        left = max(0, shape.left)
        if shape.intent < 0 and shape.left == -shape.intent:
            left = 0
        width = indent_width(runs, offset, info.width - left - shape.right,
                             justify=shape.justify, korean_by_word=shape.korean_by_word)
        if width is None:
            continue
        plans[index] = Plan(name, paragraph, text, left, -width, (shape.left, shape.intent))
    return Document(snapshot, header, sections, plans)


def apply_in_place(path: str | Path, offset_of: OffsetFunction, texts=None, *, snapshot=None) -> int:
    """표 계산이 가능한 문단(``texts`` 를 주면 그 글의 문단만)의 left/intent 를 직접 쓴다. 쓴 문단 수."""
    path = Path(path)
    if snapshot is not None and (Path(snapshot.path).resolve() != path.resolve()
                                 or snapshot.data != path.read_bytes()):
        raise ValueError("다른 파일이나 변경 전 스냅샷으로 내어쓰기를 기록할 수 없습니다.")
    document = plan_document(snapshot if snapshot is not None else path, offset_of)
    chosen = [plan for plan in document.plans.values() if texts is None or plan.text in texts]
    chosen = [plan for plan in chosen if plan.current != (plan.left, plan.intent)]
    if not chosen:
        return 0
    pool = StylePool(document.header)
    for plan in chosen:
        spec = ParagraphStyle(text=plan.text, para_extra=(("left", plan.left), ("intent", plan.intent)))
        plan.paragraph.set("paraPrIDRef", pool.para(plan.paragraph.get("paraPrIDRef"), spec))
        for element in list(plan.paragraph):
            if _tag(element) == "linesegarray":
                plan.paragraph.remove(element)     # 한/글이 다시 조판하게 한다
    snapshot = document.snapshot
    replacements = {name: _serialize(document.sections[name], snapshot.read(name))
                    for name in {plan.section for plan in chosen}}
    replacements["Contents/header.xml"] = _serialize(document.header, snapshot.read("Contents/header.xml"))
    temporary = path.with_name(path.name + ".indent.tmp")
    try:
        temporary.write_bytes(build_package(snapshot, replacements))
        PackageSnapshot.load(temporary)
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()
    return len(chosen)


def cleanup_snapshot(folder) -> bool:
    """한/글이 잡은 SaveAs 파일은 기다리지 않는다. 기존 오래된 임시폴더 정리가 재시도한다.

    hwp_format_first_ 접두어·보존 시간·소유 문서의 정상 종료 정책은 그대로 둔다.
    """
    try:
        shutil.rmtree(folder)
        return True
    except FileNotFoundError:
        return True
    except OSError:
        return False


class StageCheck:
    """Shift+Tab 단계 하나에서 '이미 표 값인 문단'을 가리고 처리 문단 수를 센다."""

    def __init__(self, plans: dict | None = None):
        self.plans = plans or {}
        self.table = 0
        self.com = 0

    def matches(self, hwp, text: str) -> bool:
        """캐럿 문단의 한/글 값이 지금 글자·문단 모양으로 계산한 표 값과 같으면 True(Shift+Tab 불필요)."""
        area, index = hwp.GetPos()[:2]
        plan = self.plans.get(index) if area == 0 else None
        if plan is None or (text or "").rstrip("\r\n") != plan.text:
            return False
        left = int(hwp.ParaShape.Item("LeftMargin"))
        indent = int(hwp.ParaShape.Item("Indentation"))
        if abs(left - plan.left * COM_UNIT) > 1 or abs(indent - plan.intent * COM_UNIT) > 2 * COM_UNIT:
            return False
        self.table += 1
        return True

    def summary(self) -> str:
        return f"[폭 표 내어쓰기] 표 처리 {self.table}문단 / 한/글 처리 {self.com}문단"
