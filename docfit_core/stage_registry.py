"""Pure-Python declarations and planning for document processing stages.

This is a shadow planner only.  It does not call the document engine or COM.
"""
from dataclasses import dataclass
from enum import Enum
from typing import Mapping

from docfit_core.stage_selection import _MODE_STAGES, _STAGE_BY_KEY, default_choice


class StageKind(str, Enum):
    XML_PROCESSOR = "XML 처리기"
    HWP_READ = "한/글 읽기"
    HWP_WRITE = "한/글 쓰기"
    LAYOUT = "조판 의존"


@dataclass(frozen=True)
class StageDeclaration:
    id: str
    name: str
    kind: StageKind
    supported_modes: frozenset[str]
    option_condition: str = ""
    prerequisites: tuple[str, ...] = ()
    page_range_supported: bool = True


_MODE_IDS = tuple(dict.fromkeys(key for keys in _MODE_STAGES.values() for key in keys))
_STAGE_KIND = {
    "abbreviation": StageKind.XML_PROCESSOR,
    "standard_format": StageKind.HWP_WRITE,
    "pre_format": StageKind.XML_PROCESSOR,
    "table_style": StageKind.XML_PROCESSOR,
    "table_width": StageKind.XML_PROCESSOR,
    "precise_table": StageKind.HWP_WRITE,
    "text_table_convert": StageKind.HWP_WRITE,
    "reset_spacing": StageKind.HWP_WRITE,
    "page_fit": StageKind.LAYOUT,
    "page_group": StageKind.LAYOUT,
}
_STAGE_KIND.update({key: StageKind.HWP_WRITE for key in _MODE_IDS if key not in _STAGE_KIND})

_SELECTABLE = [
    StageDeclaration(key, _STAGE_BY_KEY[key][1], _STAGE_KIND[key],
                     frozenset(mode for mode, ids in _MODE_STAGES.items() if key in ids),
                     option_condition=f"세부 작업 {key!r} 활성", page_range_supported=key not in {
                         "abbreviation", "pre_format", "text_table_convert", "table_style", "table_width", "precise_table"},
                     prerequisites=())
    for key in _MODE_IDS
]

# pipeline.py 선행목록에 추가되는 19개 HWPX 처리기. 이름과 등록 순서는 if 사슬과 같다.
_XML_ROWS = (
    ("xml_spacing_reset", "자간 초기화", "all + 표준서식 + XML 초기화", False, "reset_spacing"),
    ("xml_asterisk", "별표 위첨자", "format/all + 표준서식 + 쪽 범위 없음 + 선택", False, "asterisk_superscript"),
    ("xml_attachment_font", "붙임 글꼴", "format/all + 표준서식 + 쪽 범위 없음 + 선택", False, "attachment_font"),
    ("xml_report_header", "보고서 머리 서식 복사", "format/all + 머리 서식 기능 + 활성 프로필", False, "pre_format"),
    ("xml_title", "제목·개요", "format/all + 제목4종", False, "pre_format"),
    ("xml_overview_lead", "개요 표 첫 빈칸", "format/all + 제목4종", False, "pre_format"),
    ("xml_midtitle", "중제목", "format/all + 중제목 사용", False, "pre_format"),
    ("xml_attachment", "붙임", "format/all + 붙임2종 사용", False, "pre_format"),
    ("xml_box", "한 칸 상자", "format/all + box 프로필 활성", False, "pre_format"),
    ("xml_page_number", "쪽 번호 모양", "format/all + 쪽번호 설정 + standard_format 선택", False, "standard_format"),
    ("xml_table_style", "기본 표 서식", "format/all/unify + 선택 + 표 서식 프로필", False, "table_style"),
    ("xml_table_width", "표 칸 너비", "format/all + 선택", False, "table_width"),
    ("xml_table_style_align", "기본 표 서식 정렬 재확인", "표 칸 너비 활성 및 기본 표 서식 등록", False, "table_width"),
    ("xml_connector_tab", "연결 부호 탭 정렬", "format/all + 연결 부호 기능", False, "pre_format"),
    ("xml_colon_label", "콜론 라벨 정렬", "format/all + 콜론 라벨 기능", False, "pre_format"),
    ("xml_table_date", "표 날짜 칸 한 줄", "format/all/unify", False, "table_style"),
    ("xml_table_two_chars", "표 두 글자 칸", "format/all/unify", False, "table_style"),
    ("xml_blank_lines", "빈 줄 서식", "format/all + 빈줄 설정 + standard_format 선택", False, "standard_format"),
    ("xml_emphasis", "강조 낱말", "format/all + 강조 설정 + standard_format 선택", False, "standard_format"),
)
_XML_DECLARATIONS = [StageDeclaration(i, n, StageKind.XML_PROCESSOR,
    frozenset({"all", "format", "unify"} if i in {"xml_table_style", "xml_table_date", "xml_table_two_chars"} else
              {"all", "format"}), c, prerequisites=(), page_range_supported=scope)
    for i, n, c, scope, _ in _XML_ROWS]

STAGE_REGISTRY = tuple(_SELECTABLE + _XML_DECLARATIONS)
_BY_ID = {stage.id: stage for stage in STAGE_REGISTRY}
_MODE_SET = frozenset(_MODE_STAGES)


def _xml_plan(mode: str, options: Mapping, page_range: bool, selected: Mapping) -> list[str]:
    """Recreate the ordered XML 선행목록 decision chain with named context flags."""
    standard = bool(options.get("standard_format", mode in ("format", "all"))) and mode in ("format", "all")
    no_range = not page_range
    fmt = standard and no_range
    active = []
    def chosen(key):
        return bool(selected.get(key, default_choice(key, mode)))
    if mode == "all" and standard and no_range and options.get("xml_spacing_reset", False) and chosen("reset_spacing"):
        active.append("xml_spacing_reset")
    if fmt:
        if chosen("asterisk_superscript"):
            active.append("xml_asterisk")
        if chosen("attachment_font"):
            active.append("xml_attachment_font")
        if chosen("pre_format"):
            if options.get("report_header_profile", False) and options.get("report_header_enabled", False):
                active.append("xml_report_header")
            if options.get("titles_enabled", True):
                active.extend(("xml_title", "xml_overview_lead"))
            if options.get("midtitle_enabled", True):
                active.append("xml_midtitle")
            if options.get("attachments_enabled", True):
                active.append("xml_attachment")
            if options.get("box_profile", False):
                active.append("xml_box")
    if fmt and options.get("page_number", False) and chosen("standard_format"):
        active.append("xml_page_number")
    table_allowed = (fmt or mode == "unify") and chosen("table_style") and options.get("table_profile", True)
    if table_allowed:
        active.append("xml_table_style")
    width_allowed = fmt and chosen("table_width")
    if width_allowed:
        active.append("xml_table_width")
        if table_allowed:
            active.append("xml_table_style_align")
    if fmt and options.get("connector_tab", True):
        active.append("xml_connector_tab")
    if fmt and options.get("colon_label", True):
        active.append("xml_colon_label")
    if (fmt or mode == "unify") and no_range:
        active.extend(("xml_table_date", "xml_table_two_chars"))
    if fmt and options.get("blank_lines", False) and chosen("standard_format"):
        active.append("xml_blank_lines")
    if fmt and options.get("emphasis", False) and chosen("standard_format"):
        active.append("xml_emphasis")
    return active


def plan_stages(mode: str, options: Mapping | None = None, page_range: bool = False,
                selected_stages: Mapping | None = None) -> tuple[StageDeclaration, ...]:
    """Return a deterministic shadow plan from mode/options/range/detail choices.

    ``options`` carries pipeline-level flags (profiles and feature toggles), while
    ``selected_stages`` carries stage_selection.py's per-stage booleans.
    """
    if mode not in _MODE_SET:
        raise ValueError("지원하지 않는 작업 모드입니다.")
    options = options or {}
    selected_stages = selected_stages or {}
    ids = [key for key in _MODE_STAGES[mode]
           if selected_stages.get(key, default_choice(key, mode)) and
           (not page_range or _BY_ID[key].page_range_supported)]
    ids.extend(_xml_plan(mode, options, page_range, selected_stages))
    return tuple(_BY_ID[key] for key in ids)


@dataclass(frozen=True)
class StagePlanDiff:
    expected: tuple[str, ...]
    actual: tuple[str, ...]

    @property
    def differences(self) -> tuple[str, ...]:
        from collections import Counter
        expected, actual = Counter(self.expected), Counter(self.actual)
        missing = [f"누락: {key} x{count}" for key, count in (expected - actual).items()]
        extra = [f"추가: {key} x{count}" for key, count in (actual - expected).items()]
        order = ([f"순서: expected={self.expected!r}, actual={self.actual!r}"]
                 if not missing and not extra and self.expected != self.actual else [])
        return tuple(missing + extra + order)


def compare_stage_names(expected, actual) -> StagePlanDiff:
    """Compare enabled pipeline display names with the shadow plan."""
    return StagePlanDiff(tuple(expected), tuple(actual))
