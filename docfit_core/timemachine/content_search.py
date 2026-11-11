"""별도 SQLite FTS5 문서 본문 색인 및 로컬 텍스트 추출."""
from __future__ import annotations

import os
import sqlite3
import threading
import time
from contextlib import contextmanager
from .rules import excluded_name
import re
from dataclasses import dataclass, replace
from datetime import date
from pathlib import Path
from defusedxml import ElementTree as ET
from ..hwpx import open_hwpx_safely, open_zip_safely
from .extract_text import extract_text as _extract_text_by_format
from .extractors import ExtractionError, extract_document
from . import hierarchy_path

SUPPORTED = {"hwpx", "hwp", "txt", "md", "docx", "pptx", "xlsx", "xls", "pdf", "eml"}

LEVEL_ORDER: tuple[str, ...] = (
    "document_title", "chapter", "section", "topic", "square", "circle",
    "dash", "asterisk", "body",
)
LEVEL_RANKS: dict[str, int] = {level: rank for rank, level in enumerate(LEVEL_ORDER)}
# 표 칸은 표시 이름을 유지하면서 본문과 같은 최하위 위계를 사용한다.
LEVEL_RANKS["table_cell"] = LEVEL_RANKS["body"]
LEVEL_LABELS: dict[str, str] = {
    "document_title": "문서 제목", "chapter": "장", "section": "절",
    "topic": "주제·소제목", "square": "ㅁ", "circle": "ㅇ",
    "dash": "-", "asterisk": "*", "body": "본문", "table_cell": "표 칸",
}
ROLE_TO_LEVEL: dict[str, str] = {
    "장": "chapter",
    "중제목": "section",
    "소제목": "topic",
    "본문": "body",
    "내용": "body",
    "부연설명": "asterisk",
}
MARKER_CLASS_TO_LEVEL: dict[str, str] = {
    "□": "square",
    "ㅁ": "square",
    "■": "square",
    "ㅇ": "circle",
    "○": "circle",
    "◦": "circle",
    "-": "dash",
    "*": "asterisk",
    "**": "asterisk",
    "※": "asterisk",
    "•": "asterisk",
    "·": "asterisk",
    "‧": "asterisk",
    "∙": "asterisk",
    "⋅": "asterisk",
    "ㆍ": "asterisk",
    "●": "asterisk",
    "・": "asterisk",
    "–": "dash",
    "—": "dash",
    "−": "dash",
    "－": "dash",
}
_MARKDOWN_HEADING = re.compile(r"^(#{1,4})(?:\s+|$)(.*)$")
_NUMBERED_FAMILY_LEVELS = {
    "Ⅰ.": 1, "I.": 1, "1.": 2, "가.": 3, "(1)": 3, "①": 4,
    "㉠": 4, "㉮": 4,
}
CURRENT_HIERARCHY_SCHEMA_VERSION = 1
# 본문검색 쪽 넘김용 순위 목록을 보관하는 시간(초). 색인이 바뀌면 그 전에도 버린다.
KEYWORD_RANK_CACHE_SECONDS = 120
_EXTRACTED_LOCATIONS: dict[str, dict[int, tuple[str, str | None]]] = {}
_EXTRACTED_LOCATIONS_LOCK = threading.RLock()


def _min_position_span(position_lists) -> int | None:
    """낱말마다 위치를 하나씩 골랐을 때 (가장 뒤 - 가장 앞)의 최솟값. 빈 목록이 있으면 None.

    모든 조합을 만드는 대신(낱말 수만큼 곱으로 늘어남) 위치를 한 줄로 정렬해 창을 밀며 구한다.
    결과는 itertools.product로 구한 최솟값과 같다.
    """
    lists = [list(found) for found in position_lists]
    if not lists or any(not found for found in lists):
        return None
    merged = sorted((position, index) for index, found in enumerate(lists) for position in found)
    counts = [0] * len(lists)
    covered = 0
    best = None
    left = 0
    for position, index in merged:
        counts[index] += 1
        if counts[index] == 1:
            covered += 1
        while covered == len(lists):
            span = position - merged[left][0]
            if best is None or span < best:
                best = span
            left_index = merged[left][1]
            counts[left_index] -= 1
            if counts[left_index] == 0:
                covered -= 1
            left += 1
    return best


def _terms_are_near(text: str, terms: list[str], distance: int) -> bool:
    words = [match.group().casefold() for match in re.finditer(r"\S+", text)]
    positions = [[index for index, word in enumerate(words) if term.casefold() in word] for term in terms]
    span = _min_position_span(positions)
    return span is not None and span <= distance + 1


@dataclass(frozen=True)
class ParagraphRecord:
    paragraph_index: int
    text: str
    level: str
    level_rank: int
    is_table_cell: bool
    style_name: str | None = None
    outline_level: int | None = None


@dataclass
class ParagraphMatch:
    paragraph_index: int
    level: str
    level_rank: int
    match_counts: dict[str, int]
    bm25: float
    snippet: str
    highlights: list[list[int]]


def _tag_name(elem) -> str:
    tag = elem.tag
    return tag.split("}")[-1] if "}" in tag else tag


def _best_level(matches: list[ParagraphMatch]) -> tuple[str, int]:
    if not matches:
        body_rank = LEVEL_ORDER.index("body")
        return LEVEL_LABELS["body"], body_rank
    best = min(matches, key=lambda m: m.level_rank)
    return LEVEL_LABELS.get(best.level, LEVEL_LABELS["body"]), best.level_rank


def _document_date(created_at: float | None, modified_at: float | None) -> float:
    c = created_at if (isinstance(created_at, (int, float)) and created_at > 0) else 0.0
    m = modified_at if (isinstance(modified_at, (int, float)) and modified_at > 0) else 0.0
    return max(c, m)


def _sort_hierarchy_results(items: list[dict], *, sort: str) -> None:
    if sort == "hierarchy_relevance":
        def key_fn(item):
            best_rank = item.get("_best_level_rank", LEVEL_ORDER.index("body"))
            total = item.get("total_matches", 0)
            doc_date = item.get("_document_date", 0.0)
            has_date = 0 if doc_date > 0 else 1
            bm25 = item.get("_bm25_score", 0.0)
            path_norm = os.path.normcase(os.path.abspath(item["path"])).casefold()
            return (best_rank, -total, has_date, -doc_date, bm25, path_norm)
        items.sort(key=key_fn)
    elif sort == "mentions":
        items.sort(key=lambda item: (-item.get("total_matches", 0), item.get("name", "").casefold()))
    elif sort == "relevance":
        items.sort(key=lambda item: (-item.get("score", 0), item.get("name", "").casefold()))
    else:
        items.sort(key=lambda item: (-item.get("score", 0), item.get("name", "").casefold()))


def _plain_numbered_levels(paragraphs):
    """TXT/MD 번호 계열의 연속성과 첫 등장 순서를 확인한다."""
    from ..marker_scheme import classify_marker, _ordinal

    candidates, groups = {}, {}
    for position, paragraph in enumerate(paragraphs):
        text = paragraph.text.strip()
        if paragraph.is_table_cell or len(text) > 60:
            continue
        chapter = re.match(r"^제([0-9]+)장\s+\S", text)
        marker = classify_marker(text)
        if chapter:
            family, ordinal = "제N장", (int(chapter.group(1)),)
        elif marker and marker[0] in {"Ⅰ.", "I.", "1.", "가."} and text[marker[1]:].strip():
            family = "Ⅰ." if marker[0] == "I." else marker[0]
            ordinal = _ordinal(marker[0], text[:marker[1]])
        else:
            continue
        candidates[position] = family
        groups.setdefault(family, []).append((position, ordinal))
    confirmed = []
    for family, rows in groups.items():
        first = rows[0][0]
        # 일반 본문 바로 뒤에서 시작한 번호 묶음은 본문 목록으로 유지한다.
        follows_body = (first > 0 and first - 1 not in candidates
                        and not _MARKDOWN_HEADING.match(paragraphs[first - 1].text.strip()))
        if not follows_body and any(b[1][0] == a[1][0] + 1 for a, b in zip(rows, rows[1:])):
            confirmed.append(family)
    return {family: ("chapter", "section", "topic")[min(depth, 2)]
            for depth, family in enumerate(confirmed)}


def classify_paragraph_levels(path: str | os.PathLike[str], paragraphs: list[ParagraphRecord]) -> list[ParagraphRecord]:
    if not paragraphs:
        return []
    from docfit_app.hierarchy_bridge import analyze_levels

    engine_levels = analyze_levels(
        [{"text": p.text, "indent": 0, "font_size": 0} for p in paragraphs]
    )
    if engine_levels is not None:
        return _fallback_document_title(path, convert_engine_levels_to_records(paragraphs, engine_levels))

    from ..style_hierarchy import leading_marker, canonical_marker, DOT_MARKERS
    from ..marker_scheme import detect_scheme, classify_marker
    from ..style_unify import unify_marker

    texts = [p.text for p in paragraphs]
    plain_levels = (_plain_numbered_levels(paragraphs)
                    if Path(path).suffix.lower() in {".txt", ".md"} else None)
    scheme = detect_scheme(texts)
    numbered_levels = {family: _NUMBERED_FAMILY_LEVELS[family]
                       for family in scheme.family_levels if family in _NUMBERED_FAMILY_LEVELS}
    if numbered_levels:
        present_number_families = {}
        for text in texts:
            found = classify_marker(text)
            if found and found[0] in _NUMBERED_FAMILY_LEVELS:
                present_number_families.setdefault(found[0], []).append(text)
        observed = [family for family in _NUMBERED_FAMILY_LEVELS if family in present_number_families]
        if len(observed) >= 2 and all(_NUMBERED_FAMILY_LEVELS[a] < _NUMBERED_FAMILY_LEVELS[b]
                                      for a, b in zip(observed, observed[1:])):
            indents = {}
            for family in observed:
                rows = present_number_families[family]
                indents[family] = min(len(row) - len(row.lstrip()) for row in rows)
            if all(indents[a] <= indents[b] for a, b in zip(observed, observed[1:])):
                numbered_levels.update({family: _NUMBERED_FAMILY_LEVELS[family]
                                        for family in observed
                                        if family in {"(1)", "①", "㉠", "㉮"}})

    # 단일 # 문서는 제목 하나로 취급한다. # 제목이 여럿이면 첫 #은 문서 제목,
    # 이후 #은 장으로 보며 ##/###는 문서 수와 무관하게 절/주제로 유지한다.
    markdown_headings = [_MARKDOWN_HEADING.match(text.strip()) for text in texts]
    top_heading_count = sum(bool(match and len(match.group(1)) == 1) for match in markdown_headings)
    first_top_heading = next((i for i, match in enumerate(markdown_headings)
                              if match and len(match.group(1)) == 1), None)

    classified: list[ParagraphRecord] = []
    for paragraph_position, p in enumerate(paragraphs):
        text = p.text.strip()
        style_name = (p.style_name or "").lower()
        outline_lvl = p.outline_level
        level = None

        # [지적 4 반영] 표 셀 문단은 표 셀 예외를 최우선으로 적용하여 최하위인 table_cell로 고정
        if p.is_table_cell:
            level = "table_cell"
        else:
            # 1. HWPX 명시 제목/heading 스타일과 outline level
            if any(w in style_name for w in ("문서제목", "문서 제목", "title")) or style_name == "제목" or outline_lvl == 0:
                level = "document_title"
            elif any(w in style_name for w in ("heading 1", "heading1", "제목 1", "제목1", "개요 1", "개요1")) or outline_lvl == 1:
                level = "chapter"
            elif any(w in style_name for w in ("heading 2", "heading2", "제목 2", "제목2", "개요 2", "개요2")) or outline_lvl == 2:
                level = "section"
            elif any(w in style_name for w in ("heading 3", "heading3", "제목 3", "제목3", "개요 3", "개요3")) or outline_lvl == 3:
                level = "topic"

            if level is None:
                heading = markdown_headings[paragraph_position]
                if heading:
                    depth = len(heading.group(1))
                    if depth == 1:
                        level = ("document_title" if top_heading_count == 1 or paragraph_position == first_top_heading
                                 else "chapter")
                    else:
                        # #가 하나뿐이면(문서 제목) 아래 세 단계를 장·절·주제에 차례로 대응시킨다.
                        # #가 여럿이면 #이 장을 차지하므로 ##·###가 절·주제이고 ####는 본문이다.
                        if top_heading_count <= 1:
                            level = {2: "chapter", 3: "section", 4: "topic"}[depth]
                        else:
                            level = {2: "section", 3: "topic"}.get(depth)

            marker_info = classify_marker(text)
            family = marker_info[0] if marker_info else None
            if level is None and plain_levels is not None and family in {"Ⅰ.", "I.", "1.", "가."}:
                plain_family = "Ⅰ." if family == "I." else family
                level = plain_levels.get(plain_family, "body") if len(text) <= 60 else "body"
            if level is None and scheme and family in numbered_levels:
                level = {1: "chapter", 2: "section", 3: "topic", 4: "body"}[numbered_levels[family]]
            if level is None and scheme:
                marker_level = scheme.family_levels.get(family)
                if marker_level == 1:
                    level = "chapter"
                elif marker_level == 2:
                    level = "section"
                elif marker_level == 3:
                    level = "topic"

            # 번호 기호는 문서 안에서 계열·순서·들여쓰기로 체계가 확인된 경우에만 위계로 인정한다.
            if level is None:
                # [지적 3 반영] 저장소의 기존 기호 위계 판정 코드(leading_marker, unify_marker) 재사용 및 직접 매핑
                marker, role = leading_marker(text, scheme)
                unify_class, unify_role, raw_marker = unify_marker(text)

                canon_marker = canonical_marker(marker) if marker else ""
                canon_raw = canonical_marker(raw_marker) if raw_marker else ""
                cm = classify_marker(text)
                classified_marker = cm[0] if cm else ""
                canonical_classified_marker = canonical_marker(classified_marker) if classified_marker else ""
                effective_class = (
                    unify_class or canon_marker or canon_raw or canonical_classified_marker
                )

                # 기호 분류(MARKER_CLASS_TO_LEVEL) 우선 직접 매핑
                if effective_class in MARKER_CLASS_TO_LEVEL:
                    level = MARKER_CLASS_TO_LEVEL[effective_class]
                elif canon_marker in MARKER_CLASS_TO_LEVEL:
                    level = MARKER_CLASS_TO_LEVEL[canon_marker]
                elif canon_raw in MARKER_CLASS_TO_LEVEL:
                    level = MARKER_CLASS_TO_LEVEL[canon_raw]
                elif (raw_marker and raw_marker in DOT_MARKERS) or (marker and marker in DOT_MARKERS):
                    level = "asterisk"
                else:
                    # 역할(ROLE_TO_LEVEL) 기반 매핑
                    effective_role = role or unify_role

                    cm_family = cm[0] if cm else None
                    is_numbered_marker = bool(
                        cm_family
                        and (
                            cm_family.startswith("1")
                            or "가" in cm_family
                            or cm_family in ("①", "㉠", "㉮")
                        )
                    )
                    is_scheme_recognized = bool(
                        scheme
                        and (
                            scheme.canonical(text) is not None
                            or (cm_family and cm_family in scheme.family_levels)
                        )
                    )

                    if effective_role in ROLE_TO_LEVEL:
                        mapped_level = ROLE_TO_LEVEL[effective_role]
                        # 문맥 없는 단독 번호 기호는 body로 안전하게 처리 (문맥 체계에서 인정된 경우에만 절/주제 인정)
                        if is_numbered_marker and not is_scheme_recognized and mapped_level in ("section", "topic"):
                            level = "body"
                        else:
                            level = mapped_level
                    else:
                        level = "body"

        if level not in LEVEL_ORDER:
            level = "table_cell" if p.is_table_cell else "body"

        classified.append(
            ParagraphRecord(
                paragraph_index=p.paragraph_index,
                text=p.text,
                level=level,
                level_rank=LEVEL_RANKS[level],
                is_table_cell=p.is_table_cell,
                style_name=p.style_name,
                outline_level=p.outline_level,
            )
        )
    return _fallback_document_title(path, classified)


def _fallback_document_title(path, paragraphs: list[ParagraphRecord]) -> list[ParagraphRecord]:
    """명시적 제목이 없을 때만 텍스트 제목 준말을 보완한다."""
    if any(p.level == "document_title" for p in paragraphs):
        return paragraphs
    ext = Path(path).suffix.lower()
    candidate = None
    if ext in {".txt", ".md"}:
        candidate = next((i for i, p in enumerate(paragraphs)
                          if p.text.strip().startswith(("제목1:", "제목:"))), None)
    if candidate is None:
        return paragraphs
    updated = list(paragraphs)
    updated[candidate] = replace(paragraphs[candidate], level="document_title",
                                 level_rank=LEVEL_RANKS["document_title"])
    return updated


def convert_engine_levels_to_records(paragraphs: list[ParagraphRecord], result: dict) -> list[ParagraphRecord]:
    """엔진의 정수 레벨을 본문 검색 색인 레코드로 변환한다."""
    levels = result.get("levels", [])
    converted = []
    for paragraph, engine_level in zip(paragraphs, levels):
        level = {0: "document_title", 1: "chapter", 2: "section", 3: "topic",
                 4: "square", 5: "circle", 6: "dash", 7: "asterisk", 8: "body",
                 9: "table_cell", 10: "body"}.get(engine_level)
        if paragraph.is_table_cell:
            level = "table_cell"
        if level not in LEVEL_ORDER and level != "table_cell":
            level = "body"
        converted.append(ParagraphRecord(
            paragraph_index=paragraph.paragraph_index, text=paragraph.text, level=level,
            level_rank=LEVEL_RANKS.get(level, LEVEL_RANKS["body"]),
            is_table_cell=paragraph.is_table_cell, style_name=paragraph.style_name,
            outline_level=paragraph.outline_level,
        ))
    return converted


def extract_paragraphs(path: str | os.PathLike[str]) -> list[ParagraphRecord]:
    ext = Path(path).suffix.lower().lstrip(".")
    if ext not in SUPPORTED:
        raise ValueError("지원하지 않는 형식")
    if ext in {"hwpx", "docx"}:
        hierarchy_records = extract_paragraphs_legacy(path, ext)
    else:
        hierarchy_records = None
    try:
        extracted = extract_document(path, hierarchy_records=hierarchy_records)
    except ExtractionError as exc:
        raise ValueError(str(exc)) from exc
    with _EXTRACTED_LOCATIONS_LOCK:
        _EXTRACTED_LOCATIONS[os.path.abspath(os.fspath(path))] = {
            record.sequence - 1: (record.location_label, record.section) for record in extracted}
    paragraphs = [ParagraphRecord(
        paragraph_index=record.sequence - 1,
        text=record.text,
        level=record.hierarchy_level or "body",
        level_rank=LEVEL_RANKS.get(record.hierarchy_level, LEVEL_RANKS["body"]),
        is_table_cell=record.is_table_cell,
        style_name=record.style_name,
        outline_level=record.outline_level,
    ) for record in extracted]
    return classify_paragraph_levels(path, paragraphs)

def extract_paragraphs_legacy(path, ext):
    """위치 추출 전에 기존 HWPX/DOCX 위계 판정 결과를 얻는다."""
    records = []
    if ext == "hwpx":
        with open_hwpx_safely(path) as archive:
            names = archive.namelist()
            style_map = {}
            if "Contents/header.xml" in names:
                try:
                    head_root = ET.fromstring(archive.read("Contents/header.xml"))
                    for elem in head_root.iter():
                        if _tag_name(elem) == "style":
                            sid, sname = elem.attrib.get("id"), elem.attrib.get("name")
                            if sid and sname:
                                style_map[sid] = sname
                except Exception:
                    pass
            section_names = sorted((n for n in names if n.startswith("Contents/section") and n.endswith(".xml")),
                                   key=lambda n: int(re.search(r"section(\d+)", n).group(1)) if re.search(r"section(\d+)", n) else 0)
            index = 0
            def walk(node, in_table=False):
                nonlocal index
                tag = _tag_name(node)
                if tag in ("tc", "cell"):
                    for child in node:
                        walk(child, True)
                    return
                if tag == "p":
                    chunks = []
                    def collect(elem, root=False):
                        if not root and _tag_name(elem) in ("tbl", "table"):
                            return
                        if elem.text:
                            chunks.append(elem.text)
                        for child in elem:
                            collect(child)
                            if child.tail:
                                chunks.append(child.tail)
                    collect(node, True)
                    text = "".join(chunks).strip()
                    if text:
                        style_id = node.attrib.get("styleIDRef") or node.attrib.get("styleID") or node.attrib.get("style")
                        outline = node.attrib.get("outlineLevel") or node.attrib.get("outline_level")
                        for child in node:
                            if _tag_name(child) in ("pPr", "p_pr"):
                                style_id = style_id or child.attrib.get("styleIDRef") or child.attrib.get("styleID")
                                outline = outline or child.attrib.get("outlineLevel") or child.attrib.get("outline_level")
                        records.append(ParagraphRecord(index, text, "body", LEVEL_ORDER.index("body"), in_table,
                            style_map.get(style_id) if style_id else node.attrib.get("styleName"),
                            int(outline) if outline and outline.isdigit() else None))
                        index += 1
                    for child in node:
                        walk(child, in_table)
                    return
                for child in node:
                    walk(child, in_table)
            for name in section_names:
                walk(ET.fromstring(archive.read(name)))
    elif ext == "docx":
        with open_zip_safely(path) as archive:
            if "word/document.xml" not in archive.namelist():
                return []
            root = ET.fromstring(archive.read("word/document.xml"))
            style_names = {}
            default_style_names = {
                "Heading1": "Heading 1", "Heading2": "Heading 2", "Heading3": "Heading 3",
                "Title": "Title", "제목1": "제목 1", "제목2": "제목 2", "제목3": "제목 3",
            }
            if "word/styles.xml" in archive.namelist():
                styles_root = ET.fromstring(archive.read("word/styles.xml"))
                for style in styles_root.iter():
                    if _tag_name(style) != "style":
                        continue
                    style_id = next((value for key, value in style.attrib.items()
                                     if key.rsplit("}", 1)[-1] == "styleId"), None)
                    style_name = next((child.attrib.get("val") for child in style
                                       if _tag_name(child) == "name"), None)
                    if style_id and style_name:
                        style_names[style_id] = style_name
            index = 0
            def walk(node, in_table=False):
                nonlocal index
                tag = _tag_name(node)
                if tag == "tc":
                    for child in node:
                        walk(child, True)
                    return
                if tag == "p":
                    chunks = []
                    def collect(elem, root=False):
                        if not root and _tag_name(elem) in ("tbl", "table"):
                            return
                        if elem.text: chunks.append(elem.text)
                        for child in elem:
                            collect(child)
                            if child.tail: chunks.append(child.tail)
                    collect(node, True)
                    text = "".join(chunks).strip()
                    if text:
                        style = None; outline = None
                        for ppr in node:
                            if _tag_name(ppr) == "pPr":
                                for child in ppr:
                                    if _tag_name(child) == "pStyle":
                                        style = next((value for key, value in child.attrib.items()
                                                      if key.rsplit("}", 1)[-1] == "val"), None)
                                    if _tag_name(child) == "outlineLvl":
                                        value = next(iter(child.attrib.values()), None)
                                        outline = int(value) if value and value.isdigit() else None
                        records.append(ParagraphRecord(index, text, "body", LEVEL_ORDER.index("body"), in_table,
                                                       style_names.get(style, default_style_names.get(style, style)), outline))
                        index += 1
                    for child in node:
                        walk(child, in_table)
                    return
                for child in node:
                    walk(child, in_table)
            walk(root)
    return classify_paragraph_levels(path, records)

_DATE_PATTERNS = (
    re.compile(r"(?:작성\s*)?기준일\s*[:：]?\s*(20\d{2})\s*[.년/-]\s*(\d{1,2})\s*[.월/-]\s*(\d{1,2})\s*일?"),
    re.compile(r"(20\d{2})\s*년도?\s*(\d{1,2})\s*분기"),
    re.compile(r"(?:작성\s*)?기준일\s*[:：]?\s*(20\d{2})\s*년?\s*(\d{1,2})\s*월?"),
    re.compile(r"(20\d{2})\s*년도?\s*(\d{1,2})\s*분기"),
)


def extract_reference_date(text):
    """본문에서 작성 기준일 또는 연·분기/연도 기준일을 ISO 날짜로 돌려준다."""
    for pattern in _DATE_PATTERNS:
        for match in pattern.finditer(text):
            parts = [int(part) for part in match.groups()]
            try:
                if len(parts) == 3:
                    return date(*parts).isoformat()
                if len(parts) == 2 and "분기" in match.group(0):
                    year, quarter = parts
                    if 1 <= quarter <= 4:
                        return date(year, quarter * 3, 1).isoformat()
                if len(parts) == 3:
                    return date(*parts).isoformat()
                if len(parts) == 2:
                    return date(parts[0], parts[1], 1).isoformat()
            except ValueError:
                continue
    return None


def extract_text(path):
    ext = Path(path).suffix.lower().lstrip(".")
    if ext not in SUPPORTED:
        raise ValueError("지원하지 않는 형식")
    if ext == "txt":
        raw = Path(path).read_bytes()
        for encoding in ("utf-8-sig", "cp949", "utf-16"):
            try:
                return raw.decode(encoding)
            except UnicodeError:
                pass
        raise ValueError("텍스트 인코딩을 판별할 수 없음")
    if ext in ("hwpx", "docx"):
        chunks = []
        opener = open_hwpx_safely if ext == "hwpx" else __import__("zipfile").ZipFile
        with opener(path) as archive:
            names = archive.namelist()
            if ext == "hwpx":
                targets = [n for n in names if n.startswith("Contents/section") and n.endswith(".xml")]
            else:
                targets = [n for n in names if n == "word/document.xml" or
                           (n.startswith("word/header") or n.startswith("word/footer")) and n.endswith(".xml")]
            for name in targets:
                root = ET.fromstring(archive.read(name))
                chunks.extend(t.strip() for t in root.itertext() if t and t.strip())
        text = "\n".join(chunks)
        if not text.strip():
            raise ValueError("본문 텍스트가 없음")
        return text
    from pypdf import PdfReader
    reader = PdfReader(path)
    if reader.is_encrypted:
        raise ValueError("암호화된 PDF")
    text = "\n".join(page.extract_text() or "" for page in reader.pages)
    if not text.strip():
        raise ValueError("추출 가능한 PDF 텍스트가 없음")
    return text


class ContentSearchIndex:
    """메타데이터 DB와 물리적으로 분리된 FTS5 색인."""
    def __init__(self, path):
        self.path = os.fspath(path)
        if self.path != ":memory:":
            os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
        self.db = sqlite3.connect(self.path, check_same_thread=False)
        try:
            self.db.execute("PRAGMA journal_mode=WAL;")
            self.db.execute("PRAGMA busy_timeout=5000;")
        except sqlite3.OperationalError:
            pass
        self._lock = threading.RLock()
        self._batch_state = None
        self._closed = False
        self._keyword_rank_cache = None
        self.db.row_factory = sqlite3.Row
        self.db.executescript("""
            CREATE TABLE IF NOT EXISTS content_meta(
                path TEXT PRIMARY KEY,
                size INTEGER,
                mtime REAL,
                reference_date TEXT,
                name TEXT,
                extension TEXT,
                ctime REAL,
                hierarchy_schema_version INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS content_errors(path TEXT PRIMARY KEY, reason TEXT, size INTEGER, mtime REAL);
        """)
        self._paragraph_has_location = False
        try:
            # 1. 문단 FTS5 테이블 확인 및 생성
            para_exists = self.db.execute("SELECT 1 FROM sqlite_master WHERE name='content_paragraph_fts'").fetchone()
            if para_exists:
                para_sql = (self.db.execute("SELECT sql FROM sqlite_master WHERE name='content_paragraph_fts'").fetchone() or [""])[0] or ""
                if "location_label" not in para_sql:
                    with self.db:
                        self.db.execute("ALTER TABLE content_paragraph_fts RENAME TO content_paragraph_fts_legacy")
                        self.db.execute("CREATE VIRTUAL TABLE content_paragraph_fts USING fts5(path UNINDEXED,name UNINDEXED,paragraph_index UNINDEXED,level UNINDEXED,level_rank UNINDEXED,body,location_label UNINDEXED,section UNINDEXED,tokenize='trigram')")
                        self.db.execute("INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section) SELECT path,name,paragraph_index,level,level_rank,body,'문단 '||CAST(CAST(paragraph_index AS INTEGER)+1 AS TEXT),NULL FROM content_paragraph_fts_legacy")
                        self.db.execute("DROP TABLE content_paragraph_fts_legacy")
            else:
                self.db.execute("""
                CREATE VIRTUAL TABLE IF NOT EXISTS content_paragraph_fts USING fts5(
                    path UNINDEXED,
                    name UNINDEXED,
                    paragraph_index UNINDEXED,
                    level UNINDEXED,
                    level_rank UNINDEXED,
                    body,
                    location_label UNINDEXED,
                    section UNINDEXED,
                    tokenize='trigram'
                )
            """)
            self._paragraph_has_location = True
            # 2. 문서 FTS5 테이블 확인 및 생성/이전
            fts_exists = self.db.execute("SELECT 1 FROM sqlite_master WHERE name='content_fts'").fetchone()
            if fts_exists:
                sql = self.db.execute("SELECT sql FROM sqlite_master WHERE name='content_fts'").fetchone()[0]
                if "trigram" not in (sql or "").lower():
                    with self.db:
                        self.db.execute("ALTER TABLE content_fts RENAME TO content_fts_legacy")
                        self.db.execute("CREATE VIRTUAL TABLE content_fts USING fts5(path UNINDEXED,name UNINDEXED,body, tokenize='trigram')")
                        self.db.execute("INSERT INTO content_fts(path,name,body) SELECT path,name,body FROM content_fts_legacy")
                        self.db.execute("DROP TABLE content_fts_legacy")
            else:
                self.db.execute("CREATE VIRTUAL TABLE content_fts USING fts5(path UNINDEXED,name UNINDEXED,body, tokenize='trigram')")

            # SQLite trigram tokenizer cannot answer LIKE queries shorter than 3 chars.
            # Store distinct 2-character substrings as an ordinary indexed candidate table.
            self.db.executescript("""
                CREATE TABLE IF NOT EXISTS content_short_grams(
                    gram TEXT NOT NULL, path TEXT NOT NULL,
                    PRIMARY KEY(gram,path)
                );
                CREATE INDEX IF NOT EXISTS content_short_grams_path ON content_short_grams(path);
            """)

            cols = {r[1] for r in self.db.execute("PRAGMA table_info(content_meta)")}
            if "reference_date" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN reference_date TEXT")
            if "name" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN name TEXT")
            if "extension" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN extension TEXT")
            if "mtime" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN mtime REAL")
            if "size" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN size INTEGER")
            if "ctime" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN ctime REAL")
            if "hierarchy_schema_version" not in cols:
                self.db.execute("ALTER TABLE content_meta ADD COLUMN hierarchy_schema_version INTEGER NOT NULL DEFAULT 0")

            self.db.execute("""
                UPDATE content_meta
                SET name=COALESCE(name,(SELECT name FROM content_fts WHERE content_fts.path=content_meta.path)),
                    extension=COALESCE(extension,lower(substr(path,instr(path,'.')+1))),
                    mtime=COALESCE(mtime,0),
                    size=COALESCE(size,0),
                    ctime=COALESCE(ctime,0),
                    hierarchy_schema_version=COALESCE(hierarchy_schema_version,0)
            """)
            self.migration_error = None
        except sqlite3.OperationalError as exc:
            try:
                with self.db:
                    self.db.execute("ALTER TABLE content_fts RENAME TO content_fts_legacy")
                    self.db.execute("CREATE VIRTUAL TABLE content_fts USING fts5(path UNINDEXED,name UNINDEXED,body, tokenize='trigram')")
                    self.db.execute("INSERT INTO content_fts(path,name,body) SELECT path,name,body FROM content_fts_legacy")
                    self.db.execute("DROP TABLE content_fts_legacy")
                self.migration_error = None
            except sqlite3.Error as migration_exc:
                self.migration_error = str(migration_exc)
                raise RuntimeError("본문 색인 스키마 이전에 실패했습니다. 본문 재색인이 필요합니다.") from migration_exc
        # (경로, 문단 번호) → 문단 FTS rowid 대응 표. path가 UNINDEXED라 path 조건은 표 전체를 훑기 때문에
        # 경로로 문단을 찾을 때는 이 표에서 rowid를 얻어 rowid로 읽는다. 문서 재색인 없이 FTS에서 채운다.
        self._map_stamp = None
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS content_paragraph_map("
            "path TEXT NOT NULL, paragraph_index INTEGER NOT NULL, para_rowid INTEGER NOT NULL,"
            " PRIMARY KEY(path, paragraph_index, para_rowid)) WITHOUT ROWID")
        # 대응 표의 행 수·가장 큰 rowid. 행 수를 세는 것(24만 문단에서 수백 ms)을 피하려고 따로 적어 둔다.
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS content_paragraph_map_state("
            "id INTEGER PRIMARY KEY CHECK(id=1), row_count INTEGER NOT NULL, max_rowid INTEGER NOT NULL)")
        self.db.execute("INSERT OR IGNORE INTO content_paragraph_map_state(id, row_count, max_rowid) VALUES(1, -1, -1)")
        self.purge_excluded()
        with self._lock:
            if getattr(self.db, "in_transaction", False):
                self.db.commit()

    def _map_state(self):
        """이 연결·다른 연결의 쓰기를 알아채는 값. 같으면 그 사이에 표가 바뀌지 않았다."""
        return (self.db.execute("PRAGMA data_version").fetchone()[0], self.db.total_changes)

    @contextmanager
    def _map_kept(self):
        """대응 표와 문단 FTS를 함께 고치는(또는 둘 다 건드리지 않는) 쓰기를 감싼다.

        쓰기 전에 일치가 확인된 상태였다면 쓰기 뒤에도 확인된 상태로 남겨, 다음 확인을 건너뛰게 한다.
        self._lock을 잡은 채로 쓴다.
        """
        kept = self._map_stamp is not None and self._map_stamp == self._map_state()
        try:
            yield
        except BaseException:
            # 쓰다 만 상태일 수 있으니 다음에 다시 견준다.
            self._map_stamp = None
            raise
        else:
            if kept:
                self._map_stamp = self._map_state()

    def _paragraph_fts_max_rowid(self):
        try:
            return self.db.execute("SELECT MAX(id) FROM content_paragraph_fts_docsize").fetchone()[0] or 0
        except sqlite3.OperationalError:
            return self.db.execute("SELECT MAX(rowid) FROM content_paragraph_fts").fetchone()[0] or 0

    def _paragraph_fts_row_count(self):
        """문단 FTS의 행 수. 열린 트랜잭션이 없으면 FTS5가 적어 둔 전체 행 수(읽기 1번)를 쓴다."""
        if not self.db.in_transaction:
            # FTS5 평균 기록(_data의 id=1)은 전체 행 수(varint)로 시작하고 커밋 때 갱신된다.
            try:
                row = self.db.execute("SELECT block FROM content_paragraph_fts_data WHERE id=1").fetchone()
            except sqlite3.OperationalError:
                row = None
            block = bytes(row[0]) if row and row[0] else b""
            value = 0
            for position, byte in enumerate(block[:9]):
                if position == 8:
                    return (value << 8) | byte
                value = (value << 7) | (byte & 0x7F)
                if byte < 0x80:
                    return value
        try:
            return self.db.execute("SELECT COUNT(*) FROM content_paragraph_fts_docsize").fetchone()[0]
        except sqlite3.OperationalError:
            return self.db.execute("SELECT COUNT(*) FROM content_paragraph_fts").fetchone()[0]

    def _ensure_paragraph_map(self):
        """대응 표가 문단 FTS와 어긋났으면 비우고 다시 채운다(전체 훑기 1회). 다시 채웠으면 True.

        색인 함수는 대응 표를 함께 고치지만, 옛 색인이나 문단 FTS에 직접 넣은 행은 대응 표에 없다.
        행 수·가장 큰 rowid·그 rowid의 경로를 견줘 어긋남을 찾는다. 바뀐 것이 없으면 견주지도 않는다.
        """
        with self._lock:
            if self._map_stamp is not None and self._map_stamp == self._map_state():
                return False
            state = self.db.execute(
                "SELECT row_count, max_rowid FROM content_paragraph_map_state WHERE id=1").fetchone()
            fts_max = self._paragraph_fts_max_rowid()
            same = state is not None and tuple(state) == (self._paragraph_fts_row_count(), fts_max)
            if same and fts_max:
                last = self.db.execute(
                    "SELECT path, CAST(paragraph_index AS INTEGER) FROM content_paragraph_fts WHERE rowid=?",
                    (fts_max,)).fetchone()
                same = last is not None and self.db.execute(
                    "SELECT 1 FROM content_paragraph_map WHERE path=? AND paragraph_index=? AND para_rowid=?",
                    (last[0] if last[0] is not None else "", last[1] or 0, fts_max)).fetchone() is not None
            if not same:
                with self.db:
                    self.db.execute("DELETE FROM content_paragraph_map")
                    filled = self.db.execute(
                        "INSERT INTO content_paragraph_map(path, paragraph_index, para_rowid)"
                        " SELECT COALESCE(path, ''), COALESCE(CAST(paragraph_index AS INTEGER), 0), rowid"
                        " FROM content_paragraph_fts").rowcount
                    self.db.execute(
                        "INSERT OR REPLACE INTO content_paragraph_map_state(id, row_count, max_rowid) VALUES(1, ?, ?)",
                        (filled, self._paragraph_fts_max_rowid()))
            self._map_stamp = self._map_state()
            return not same

    def _map_rows_changed(self, delta):
        """대응 표 행 수 기록을 고친다. 대응 표와 문단 FTS를 함께 고친 같은 트랜잭션 안에서 부른다."""
        self.db.execute(
            "UPDATE content_paragraph_map_state SET row_count=row_count+?, max_rowid=? WHERE id=1",
            (delta, self._paragraph_fts_max_rowid()))

    def _delete_paragraphs(self, path):
        """한 문서의 문단을 대응 표의 rowid로 지운다(path 조건의 전체 훑기 없음). 대응 표 행도 지운다."""
        self.db.execute(
            "DELETE FROM content_paragraph_fts WHERE rowid IN"
            " (SELECT para_rowid FROM content_paragraph_map WHERE path=?)", (path,))
        removed = self.db.execute("DELETE FROM content_paragraph_map WHERE path=?", (path,)).rowcount
        if removed:
            self._map_rows_changed(-removed)
        return removed

    def purge_excluded(self):
        """이전 버전이 색인한 제외 대상(#IEeEE…) 문서를 모든 본문 표에서 지운다. 지운 경로 목록."""
        paths = set()
        with self._lock:
            for sql in ("SELECT path, name FROM content_meta", "SELECT path, NULL AS name FROM content_errors",
                        "SELECT DISTINCT path, name FROM content_paragraph_fts", "SELECT path, name FROM content_fts",
                        "SELECT DISTINCT path, NULL AS name FROM content_short_grams"):
                try:
                    rows = self.db.execute(sql).fetchall()
                except sqlite3.OperationalError:
                    continue
                paths.update(row[0] for row in rows
                             if excluded_name(os.path.basename(row[0] or "")) or excluded_name(row[1] or ""))
            if paths:
                self._ensure_paragraph_map()
                with self._map_kept(), self.db:
                    for path in paths:
                        self._delete_paragraphs(path)
                        for table in ("content_fts", "content_short_grams",
                                      "content_meta", "content_errors"):
                            try:
                                self.db.execute(f"DELETE FROM {table} WHERE path=?", (path,))
                            except sqlite3.OperationalError:
                                pass
        return sorted(paths)

    @staticmethod
    def parse_query(query):
        """Parse AND terms, quoted phrases and Docufinder-style filters."""
        if not isinstance(query, str) or not query.strip() or len(query) > 200:
            raise ValueError("검색어는 1~200자로 입력해 주세요.")
        tokens = re.findall(r'-?"(?:[^"\\]|\\.)*"|\S+', query)
        parsed = {"terms": [], "phrases": [], "exclude": [], "ext": [], "path": [], "after": None, "before": None, "near": None}
        for token in tokens:
            neg = token.startswith("-")
            raw = token[1:] if neg else token
            if raw.startswith('"') and raw.endswith('"'):
                parsed["exclude" if neg else "phrases"].append(raw[1:-1].replace('\\"','"'))
                continue
            if not neg and ":" in raw:
                key, value = raw.split(":", 1)
                if key == "ext": parsed["ext"].extend(x.lower().lstrip(".") for x in value.split(",") if x)
                elif key == "path" and value: parsed["path"].append(value)
                elif key in ("after", "before"):
                    parsed[key] = value if re.fullmatch(r"\d{4}(?:-\d{2}(?:-\d{2})?)?", value) else (_ for _ in ()).throw(ValueError(f"{key} 날짜 형식이 올바르지 않습니다."))
                else: parsed["terms"].append(raw)
                continue
            if not neg and re.fullmatch(r"~\d+", raw): parsed["near"] = int(raw[1:]); continue
            (parsed["exclude"] if neg else parsed["terms"]).append(raw)
        return parsed

    def close(self):
        with self._lock:
            if not self._closed:
                self.db.close()
                self._closed = True

    def needs_reindex_count(self, roots=None) -> int:
        """위계 정보가 없거나 오래된 스키마 버전의 색인 문서 수."""
        with self._lock:
            self._ensure_paragraph_map()
            rows = self.db.execute("""
                SELECT m.path, m.hierarchy_schema_version,
                       (SELECT COUNT(*) FROM content_paragraph_map p WHERE p.path = m.path) AS para_count
                FROM content_meta m
            """).fetchall()
        allowed = [os.path.normcase(os.path.abspath(root)) for root in roots] if roots else None
        count = 0
        for r in rows:
            path = os.path.abspath(r["path"])
            if allowed is not None and not any(
                os.path.commonpath((os.path.normcase(path), root)) == root
                for root in allowed
                if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(path)[0].casefold()
            ):
                continue
            if (r["hierarchy_schema_version"] or 0) < CURRENT_HIERARCHY_SCHEMA_VERSION or (r["para_count"] or 0) == 0:
                count += 1
        return count

    @contextmanager
    def batch(self, *, max_files=50, max_seconds=2.0):
        """여러 파일의 색인 커밋을 묶는다(max_files개 또는 max_seconds초마다 한 번).

        잠금(self._lock)은 묶음 내내 쥐지 않는다. 파일 하나를 고치는 동안만 잡고 커밋만 미루므로
        다른 스레드의 검색·상태 조회가 파일 사이에 끼어들 수 있다. 잠금이 풀린 시점의 미커밋 변경은
        항상 '파일 단위로 끝난' 변경뿐이라, 끼어든 쪽이 먼저 커밋해도 표들은 어긋나지 않는다.
        예외가 나면 아직 커밋하지 않은 조각만 되돌린다(그 전에 커밋된 조각은 남는다).
        이미 묶음이 열려 있으면(다른 스레드 포함) 그 묶음에 얹혀 간다.
        """
        if max_files < 1 or max_seconds <= 0:
            raise ValueError("batch limits must be positive")
        with self._lock:
            owner = self._batch_state is None
            if owner:
                if self.db.in_transaction:
                    self.db.commit()
                # [미커밋 파일 수, 조각을 연 시각, 파일 수 한도, 시간 한도]
                self._batch_state = [0, time.monotonic(), max_files, max_seconds]
        if not owner:
            yield self
            return
        try:
            yield self
        except BaseException:
            with self._lock:
                self._batch_state = None
                if not self._closed:
                    if self.db.in_transaction:
                        self.db.rollback()
                    self._map_stamp = None
            raise
        else:
            with self._lock:
                self._batch_state = None
                if not self._closed and self.db.in_transaction:
                    self.db.commit()

    def _batch_flush_if_due(self, *, count=0):
        """묶음의 한도(파일 수·시간)를 넘었으면 커밋한다. self._lock을 잡은 채로 부른다."""
        state = self._batch_state
        if state is None:
            return
        if not self.db.in_transaction:
            # 다른 스레드가 먼저 커밋했거나 아직 쓴 것이 없다. 조각을 새로 센다.
            state[0:2] = [0, time.monotonic()]
            return
        state[0] += count
        if state[0] >= state[2] or time.monotonic() - state[1] >= state[3]:
            self.db.commit()
            state[0:2] = [0, time.monotonic()]

    @contextmanager
    def _index_transaction(self, path):
        """파일 하나(path)의 쓰기를 감싼다. self._lock을 잡은 채로 쓴다."""
        if self._batch_state is None:
            with self.db:
                yield
            return
        if not self.db.in_transaction:
            self.db.execute("BEGIN")
            self._batch_state[0:2] = [0, time.monotonic()]
        try:
            yield
        except BaseException:
            # 파일마다 SAVEPOINT를 두면 묶음의 이득이 절반으로 준다(300개 측정). 대신 실패한 파일의
            # 쓰다 만 행만 잠금을 쥔 채로 지워, 묶음의 앞선 파일들은 살리고 표들은 1:1로 남긴다.
            try:
                self._map_stamp = None
                self._ensure_paragraph_map()
                self._delete_paragraphs(path)
                for table in ("content_fts", "content_short_grams", "content_meta"):
                    self.db.execute(f"DELETE FROM {table} WHERE path=?", (path,))
            except BaseException:
                # 정리마저 실패하면 미커밋 조각을 통째로 되돌린다.
                if self.db.in_transaction:
                    self.db.rollback()
            self._map_stamp = None
            raise
        else:
            self._batch_flush_if_due(count=1)

    def index_file(self, path):
        path = os.path.abspath(os.fspath(path))
        from .rules import excluded_name
        if excluded_name(os.path.basename(path)):
            self.remove_file(path)
            return {"indexed": False, "error": None}
        stat = os.stat(path)
        ctime = getattr(stat, "st_ctime", 0.0)
        mtime = stat.st_mtime
        size = stat.st_size
        try:
            with self._lock:
                # 본문 추출이 오래 걸릴 수 있으니, 그 전에 시간 한도를 넘긴 묶음 조각을 커밋해 둔다.
                self._batch_flush_if_due()
                self._ensure_paragraph_map()
                old = self.db.execute(
                    "SELECT size, mtime, hierarchy_schema_version FROM content_meta WHERE path=?",
                    (path,)
                ).fetchone()
                if (
                    old
                    and old["size"] == size
                    and old["mtime"] == mtime
                    and (old["hierarchy_schema_version"] or 0) >= CURRENT_HIERARCHY_SCHEMA_VERSION
                ):
                    has_para = self.db.execute("SELECT 1 FROM content_paragraph_map WHERE path=? LIMIT 1", (path,)).fetchone()
                    if has_para:
                        return {"indexed": False, "error": None}

            paragraphs = extract_paragraphs(path)
            if not paragraphs:
                raise ValueError("본문 텍스트가 없음")
            body = "\n".join(p.text for p in paragraphs)
            ref_date = extract_reference_date(body)
            name = os.path.basename(path)
            ext = Path(path).suffix.lower().lstrip(".")

            with self._lock:
                self._ensure_paragraph_map()
            with self._lock, self._map_kept(), self._index_transaction(path):
                self._delete_paragraphs(path)
                self.db.execute("DELETE FROM content_fts WHERE path=?", (path,))
                self.db.execute("DELETE FROM content_short_grams WHERE path=?", (path,))
                map_rows = []
                for p in paragraphs:
                    location = _EXTRACTED_LOCATIONS.get(path, {}).get(p.paragraph_index,
                        (f"문단 {p.paragraph_index + 1}", None))
                    para_rowid = self.db.execute(
                        "INSERT INTO content_paragraph_fts(path, name, paragraph_index, level, level_rank, body, location_label, section) VALUES(?,?,?,?,?,?,?,?)",
                        (path, name, int(p.paragraph_index), p.level, int(p.level_rank), p.text, *location)
                    ).lastrowid
                    map_rows.append((path, int(p.paragraph_index), para_rowid))
                self.db.executemany(
                    "INSERT INTO content_paragraph_map(path, paragraph_index, para_rowid) VALUES(?,?,?)", map_rows)
                self._map_rows_changed(len(map_rows))
                self.db.execute("INSERT INTO content_fts(path, name, body) VALUES(?,?,?)", (path, name, body))
                grams = {body[i:i + 2].casefold() for i in range(max(0, len(body) - 1))}
                self.db.executemany("INSERT INTO content_short_grams(gram,path) VALUES(?,?)",
                                    ((gram, path) for gram in grams))
                self.db.execute(
                    "INSERT OR REPLACE INTO content_meta(path, size, mtime, ctime, reference_date, name, extension, hierarchy_schema_version) VALUES(?,?,?,?,?,?,?,?)",
                    (path, size, mtime, ctime, ref_date, name, ext, CURRENT_HIERARCHY_SCHEMA_VERSION)
                )
                self.db.execute("DELETE FROM content_errors WHERE path=?", (path,))
            return {"indexed": True, "error": None}
        except Exception as exc:
            reason = f"{type(exc).__name__}: {exc}"
            with self._lock:
                self._ensure_paragraph_map()
            with self._lock, self._map_kept(), self._index_transaction(path):
                self._delete_paragraphs(path)
                self.db.execute("DELETE FROM content_fts WHERE path=?", (path,))
                self.db.execute("DELETE FROM content_meta WHERE path=?", (path,))
                self.db.execute("DELETE FROM content_short_grams WHERE path=?", (path,))
                self.db.execute("INSERT OR REPLACE INTO content_errors VALUES(?,?,?,?)", (path, reason, size, mtime))
            return {"indexed": False, "error": reason}

    def remove_file(self, path):
        path = os.path.abspath(os.fspath(path))
        with self._lock:
            self._ensure_paragraph_map()
        with self._lock, self._map_kept(), self._index_transaction(path):
            self._delete_paragraphs(path)
            self.db.execute("DELETE FROM content_fts WHERE path=?", (path,))
            self.db.execute("DELETE FROM content_short_grams WHERE path=?", (path,))
            self.db.execute("DELETE FROM content_meta WHERE path=?", (path,))
            self.db.execute("DELETE FROM content_errors WHERE path=?", (path,))

    def indexed_paths(self, *, roots=None):
        """색인된 경로 목록. 전달된 루트 안의 문서만 돌려준다."""
        with self._lock:
            self.purge_excluded()
            if getattr(self.db, "in_transaction", False):
                self.db.commit()
            rows = self.db.execute("SELECT path FROM content_meta").fetchall()
            if getattr(self.db, "in_transaction", False):
                self.db.commit()
        allowed = [os.path.normcase(os.path.abspath(root)) for root in roots] if roots else None
        return [os.path.abspath(row["path"]) for row in rows if not excluded_name(os.path.basename(row["path"])) and (allowed is None or any(
            os.path.commonpath((os.path.normcase(os.path.abspath(row["path"])), root)) == root
            for root in allowed if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(os.path.abspath(row["path"]))[0].casefold()))]

    def search(self, query, *, roots=None, limit=100, sort="hierarchy_relevance"):
        self._ensure_paragraph_map()
        parsed = self.parse_query(query)
        terms = parsed["terms"] + parsed["phrases"]
        if not terms:
            raise ValueError("본문 검색어를 입력해 주세요.")
        lim = max(1, min(int(limit), 500))

        # 1. Trigram 전문 검색으로 양성 검색어별 후보 경로 추출 (AND 교집합)
        candidates = None
        fts_terms = [t for t in terms if len(t) >= 3]
        for term in fts_terms:
            expression = '"' + term.replace('"', '""') + '"'
            try:
                with self._lock:
                    found = {r[0] for r in self.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?", (expression,))}
            except sqlite3.OperationalError:
                with self._lock:
                    found = {r[0] for r in self.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?", ("%" + term + "%",))}
            candidates = found if candidates is None else candidates & found
            if not candidates:
                return []

        if candidates is None:
            # 2글자 이하 검색어: 2글자는 조각 표(content_short_grams)로, 나머지는 LIKE로 후보 경로 조회.
            # 조각 표가 없는 옛 색인 문서가 하나라도 있으면 결과가 빠지지 않게 LIKE를 쓴다.
            with self._lock:
                grams_complete = self.db.execute(
                    "SELECT 1 FROM content_meta m WHERE NOT EXISTS"
                    " (SELECT 1 FROM content_short_grams g WHERE g.path=m.path) LIMIT 1").fetchone() is None
            for term in terms:
                with self._lock:
                    if grams_complete and len(term) == 2:
                        found = {r[0] for r in self.db.execute(
                            "SELECT path FROM content_short_grams WHERE gram=?", (term.casefold(),))}
                    else:
                        found = {r[0] for r in self.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?", ("%" + term + "%",))}
                candidates = found if candidates is None else candidates & found
                if not candidates:
                    return []

        # 2. 메타데이터 조회 및 필터링 (구형 스키마 버전 문서는 제외)
        with self._lock:
            if len(candidates) <= 500:
                marks = ",".join("?" for _ in candidates)
                meta_rows = self.db.execute(f"""
                    SELECT path, name, extension, mtime, ctime, reference_date, hierarchy_schema_version
                    FROM content_meta
                    WHERE hierarchy_schema_version >= ? AND path IN ({marks})
                """, (CURRENT_HIERARCHY_SCHEMA_VERSION, *candidates)).fetchall()
            else:
                c_set = set(candidates)
                all_meta = self.db.execute("""
                    SELECT path, name, extension, mtime, ctime, reference_date, hierarchy_schema_version
                    FROM content_meta
                    WHERE hierarchy_schema_version >= ?
                """, (CURRENT_HIERARCHY_SCHEMA_VERSION,)).fetchall()
                meta_rows = [r for r in all_meta if r["path"] in c_set or os.path.abspath(r["path"]) in c_set]

        allowed = [os.path.normcase(os.path.abspath(r)) for r in roots] if roots else None
        valid_meta = {}
        for r in meta_rows:
            raw_p = r["path"]
            p = os.path.abspath(raw_p)
            if excluded_name(r["name"] or os.path.basename(p)):
                continue
            if any(part.casefold() == "$recycle.bin" for part in Path(p).parts):
                continue
            if allowed is not None and not any(
                os.path.commonpath((os.path.normcase(p), root)) == root
                for root in allowed
                if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(p)[0].casefold()
            ):
                continue
            ext = r["extension"] or Path(p).suffix.lower().lstrip(".")
            if parsed["ext"] and ext not in parsed["ext"]:
                continue
            if parsed["path"] and not all(v.casefold() in p.casefold() for v in parsed["path"]):
                continue
            ref_date = r["reference_date"]
            if parsed["after"] and (not ref_date or ref_date < parsed["after"]):
                continue
            if parsed["before"] and (not ref_date or ref_date >= parsed["before"]):
                continue
            valid_meta[raw_p] = r
            valid_meta[p] = r

        if not valid_meta:
            return []

        # 3. FTS 문단을 문서별로 SQL 집계한다. 본문 전체는 순위 계산 뒤
        # 상위 결과 snippet을 만들 때만 다시 읽는다.
        count_terms = [(t, t) for t in parsed["terms"]] + [(p, p) for p in parsed["phrases"]]
        aggregate_sql = []
        for idx, (label, term) in enumerate(count_terms):
            aggregate_sql.append(
                f"SUM(CASE WHEN instr(body_folded, lower(?)) > 0 "
                f"THEN (length(body_folded) - length(replace(body_folded, lower(?), ''))) "
                f"/ max(length(lower(?)), 1) "
                f"ELSE 0 END) AS match_count_{idx}"
            )
        count_columns = ", ".join(aggregate_sql)
        # 기존 문서 순위/집계는 유지하고, 직접 AND 일치가 없는 문서만 문맥을 조사한다.
        direct_condition = " AND ".join(
            "instr(body_folded, lower('" + term.replace("'", "''") + "')) > 0"
            for term in terms)
        count_columns += f", MAX(CASE WHEN {direct_condition} THEN 1 ELSE 0 END) AS direct_match"
        match_any = " OR ".join("instr(body_folded, lower(?)) > 0" for _ in terms)
        # FTS BM25 is constant per query result document in this path's legacy
        # ranking; use the minimum matching paragraph rank and first top-level
        # paragraph for the established hierarchy tie break.
        with self._lock:
            # 집계 대상을 1·2단계에서 남은 후보 문서로 좁힌다. 후보가 아닌 문서의 문단은 어차피
            # 아래에서 버려지므로 결과는 같고, 본문 복사·소문자 변환·그룹 집계만 줄어든다.
            opened_here = not self.db.in_transaction
            self.db.execute("CREATE TEMP TABLE IF NOT EXISTS hierarchy_candidate_paths(path TEXT PRIMARY KEY)")
            self.db.execute("DELETE FROM hierarchy_candidate_paths")
            self.db.executemany("INSERT OR IGNORE INTO hierarchy_candidate_paths(path) VALUES(?)",
                                ((row["path"],) for row in {id(m): m for m in valid_meta.values()}.values()))
            if opened_here and self.db.in_transaction:
                self.db.commit()
            if fts_terms:
                match_expr = " OR ".join('"' + t.replace('"', '""') + '"' for t in fts_terms)
                try:
                    aggregate_rows = self.db.execute(f"""
                        WITH scored AS MATERIALIZED (
                            SELECT rowid AS paragraph_rowid, path, paragraph_index, level, level_rank, body,
                                   lower(body) AS body_folded, bm25(content_paragraph_fts) AS bm25_rank
                            FROM content_paragraph_fts
                            WHERE content_paragraph_fts MATCH ?
                              AND path IN (SELECT path FROM temp.hierarchy_candidate_paths)
                        ), matched AS (
                            SELECT * FROM scored WHERE ({match_any})
                        ), best AS (
                            SELECT path, MIN(COALESCE(level_rank, 8)) AS best_level_rank,
                                   MIN(bm25_rank) AS best_bm25,
                                   MIN(CAST(paragraph_index AS INTEGER)) AS first_match_index
                            FROM matched GROUP BY path
                        ), chosen AS (
                            -- MIN 하나만 있는 집계에서는 SQLite가 그 최솟값 행의 다른 열을 돌려준다.
                            SELECT m.path, MIN(CAST(m.paragraph_index AS INTEGER)) AS chosen_paragraph_index,
                                   m.paragraph_rowid AS chosen_rowid
                            FROM matched m JOIN best b ON b.path=m.path AND b.best_level_rank=COALESCE(m.level_rank, 8)
                            GROUP BY m.path
                        ), counts AS (
                            SELECT path, COUNT(*) AS matched_paragraphs, {count_columns}
                            FROM matched GROUP BY path
                        )
                        SELECT b.path, c.matched_paragraphs, b.best_level_rank, b.best_bm25,
                               ch.chosen_paragraph_index, ch.chosen_rowid, c.direct_match, {", ".join(f"c.match_count_{i}" for i in range(len(count_terms)))}
                        FROM best b JOIN counts c ON c.path=b.path JOIN chosen ch ON ch.path=b.path
                        ORDER BY c.matched_paragraphs DESC LIMIT 100000
                    """, (match_expr, *terms,
                          *[v for _, t in count_terms for v in (t, t, t)])).fetchall()
                except sqlite3.OperationalError:
                    aggregate_rows = self.db.execute(f"""
                        SELECT path, COUNT(*) AS matched_paragraphs,
                               MIN(CASE WHEN ({match_any}) THEN COALESCE(level_rank, 8) END) AS best_level_rank,
                               0.0 AS best_bm25,
                               MIN(CASE WHEN ({match_any}) THEN CAST(paragraph_index AS INTEGER) END) AS chosen_paragraph_index,
                               {count_columns}
                        FROM content_paragraph_fts
                        WHERE content_paragraph_fts MATCH ?
                          AND path IN (SELECT path FROM temp.hierarchy_candidate_paths)
                          AND ({match_any})
                        GROUP BY path
                        ORDER BY SUM(CASE WHEN ({match_any}) THEN 1 ELSE 0 END) DESC
                        LIMIT 100000
                    """, (*terms, *terms, *[v for _, t in count_terms for v in (t, t, t)], match_expr, *terms, *terms)).fetchall()
            else:
                aggregate_rows = self.db.execute(f"""
                    SELECT path, COUNT(*) AS matched_paragraphs,
                           MIN(CASE WHEN ({match_any}) THEN COALESCE(level_rank, 8) END) AS best_level_rank,
                           0.0 AS best_bm25,
                           MIN(CASE WHEN ({match_any}) THEN CAST(paragraph_index AS INTEGER) END) AS chosen_paragraph_index,
                           {count_columns}
                    FROM (SELECT content_paragraph_fts.*, lower(body) AS body_folded
                          FROM content_paragraph_fts)
                    WHERE ({match_any})
                      AND path IN (SELECT path FROM temp.hierarchy_candidate_paths)
                    GROUP BY path
                    ORDER BY SUM(CASE WHEN ({match_any}) THEN 1 ELSE 0 END) DESC
                    LIMIT 100000
                """, (*terms, *terms, *[v for _, t in count_terms for v in (t, t, t)], *terms, *terms)).fetchall()

        # 4. SQL 집계 결과로 문서 AND·근접 조건을 판정한다.

        # 제외어 포함 문서 사전 추출
        excluded_paths = set()
        if parsed["exclude"]:
            with self._lock:
                for ex in parsed["exclude"]:
                    if len(ex) >= 3:
                        ex_expr = '"' + ex.replace('"', '""') + '"'
                        try:
                            for er in self.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?", (ex_expr,)):
                                excluded_paths.add(er[0])
                                excluded_paths.add(os.path.abspath(er[0]))
                        except sqlite3.OperationalError:
                            for er in self.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?", ("%" + ex + "%",)):
                                excluded_paths.add(er[0])
                                excluded_paths.add(os.path.abspath(er[0]))
                    else:
                        for er in self.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?", ("%" + ex + "%",)):
                            excluded_paths.add(er[0])
                            excluded_paths.add(os.path.abspath(er[0]))

        prepared = []
        for aggregate in aggregate_rows:
            path = aggregate["path"]
            path = path if path in valid_meta else os.path.abspath(path)
            if path in excluded_paths or os.path.abspath(path) in excluded_paths:
                continue
            meta = valid_meta.get(path)
            if not meta:
                continue

            match_counts = {label: int(aggregate[f"match_count_{idx}"] or 0)
                            for idx, (label, _) in enumerate(count_terms)}
            # SQL occurrence counts allow document-level AND without materializing bodies.
            if any(match_counts.get(t, 0) == 0 for t in terms):
                continue

            # N 근접도 검사
            if parsed["near"] is not None and len(terms) > 1:
                with self._lock:
                    near_rows = self.db.execute("SELECT body FROM content_paragraph_fts WHERE rowid IN (SELECT para_rowid FROM content_paragraph_map WHERE path=?) ORDER BY CAST(paragraph_index AS INTEGER)", (path,)).fetchall()
                combined_matched_text = "\n".join(r["body"] or "" for r in near_rows)
                word_spans = [(m.group().casefold(), m.start(), m.end()) for m in re.finditer(r"\S+", combined_matched_text)]
                term_positions = [[i for i, (word, _, _) in enumerate(word_spans) if term.casefold() in word]
                                  for term in terms]
                span = _min_position_span(term_positions)
                if span is None or span > parsed["near"] + 1:
                    continue

            # 언급 횟수 집계
            total_matches = sum(match_counts.values())
            best_level_rank = int(aggregate["best_level_rank"] if aggregate["best_level_rank"] is not None else LEVEL_ORDER.index("body"))
            best_level = LEVEL_LABELS.get(LEVEL_ORDER[best_level_rank] if best_level_rank < len(LEVEL_ORDER) else "body", LEVEL_LABELS["body"])
            best_bm25 = float(aggregate["best_bm25"] or 0.0)

            doc_date = _document_date(meta["ctime"], meta["mtime"])
            file_name = meta["name"] or os.path.basename(path)

            prepared.append({
                "path": path,
                "name": file_name,
                "_chosen_paragraph_index": aggregate["chosen_paragraph_index"],
                "_chosen_rowid": aggregate["chosen_rowid"] if "chosen_rowid" in aggregate.keys() else None,
                "_raw_path": aggregate["path"],
                "_direct_match": bool(aggregate["direct_match"]),
                "score": total_matches,
                "match_counts": match_counts,
                "total_matches": total_matches,
                "position": {"page": None, "sheet": None, "row": None},
                "best_level": best_level,
                "_best_level_rank": best_level_rank,
                "_document_date": doc_date,
                "_bm25_score": best_bm25,
            })

        # 직접 일치가 없는 후보만 읽는다. 조상은 표시용으로 줄이지 않은 원문을 사용한다.
        context_paths = {item["_raw_path"] for item in prepared if not item["_direct_match"]}
        context_rows = {}
        if context_paths:
            for row in self._select_paragraphs_for_paths(
                    "rowid AS paragraph_rowid, path, paragraph_index, level_rank, body", context_paths):
                context_rows.setdefault(row["path"], []).append(row)
        folded_terms = {term.casefold() for term in terms}
        for item in prepared:
            item["_context_match"] = False
            if item.pop("_direct_match"):
                continue
            rows = sorted(context_rows.get(item["_raw_path"], ()), key=lambda r: int(r["paragraph_index"]))
            parent_positions = hierarchy_path.parents([row["level_rank"] for row in rows])
            heading_terms = []
            best = None
            for position, row in enumerate(rows):
                body_terms = {term for term in folded_terms if term in (row["body"] or "").casefold()}
                parent = parent_positions[position]
                ancestor_terms = heading_terms[parent] if parent is not None else set()
                heading_terms.append(ancestor_terms | body_terms)
                # 조상에만 있는 질의로 무관한 본문까지 일치시키지 않는다.
                if not body_terms or not folded_terms <= body_terms | ancestor_terms:
                    continue
                missing = len(folded_terms - body_terms)
                key = (missing, int(row["level_rank"] or 0), int(row["paragraph_index"]))
                if best is None or key < best[0]:
                    best = (key, row, len(ancestor_terms))
            if best is not None:
                item["_context_match"] = True
                item["_chosen_paragraph_index"] = best[1]["paragraph_index"]
                item["_chosen_rowid"] = best[1]["paragraph_rowid"]
                item["_context_hits"] = best[2]
                item["best_level"] = LEVEL_LABELS[LEVEL_ORDER[int(best[1]["level_rank"])]]

        # 5. 정렬 및 순위 부여. 직접 일치가 있는 기존 결과의 상대 순위는 그대로다.
        _sort_hierarchy_results(prepared, sort=sort)
        prepared.sort(key=lambda item: item.pop("_context_match"))
        final_items = prepared[:lim]
        for item in prepared[lim:]:
            item.pop("_chosen_rowid", None); item.pop("_raw_path", None)
        with self._lock:
            # 발췌문은 결과마다 path로 표 전체를 훑지 않고 한 번에 읽는다.
            # rowid를 아는 결과는 rowid로, 모르는 결과(2글자·예비 경로)는 경로 묶음 조회 한 번으로 읽는다.
            by_rowid, by_position = {}, {}
            rowids = sorted({int(item["_chosen_rowid"]) for item in final_items if item["_chosen_rowid"] is not None})
            for start in range(0, len(rowids), 500):
                chunk = rowids[start:start + 500]
                for row in self.db.execute(
                        "SELECT rowid AS paragraph_rowid, body, level FROM content_paragraph_fts WHERE rowid IN ("
                        + ",".join("?" for _ in chunk) + ")", chunk):
                    by_rowid[row["paragraph_rowid"]] = row
            scan_paths = {item["_raw_path"] for item in final_items if item["_chosen_rowid"] is None}
            if scan_paths:
                for row in self._select_paragraphs_for_paths("path, paragraph_index, body, level", scan_paths):
                    by_position.setdefault((row["path"], int(row["paragraph_index"])), row)
            for item in final_items:
                chosen = item.pop("_chosen_paragraph_index")
                chosen_rowid = item.pop("_chosen_rowid")
                raw_path = item.pop("_raw_path")
                if chosen_rowid is not None:
                    row = by_rowid.get(int(chosen_rowid))
                else:
                    row = by_position.get((raw_path, int(chosen))) if chosen is not None else None
                item["_target_body"] = (row["body"] if row else "") or ""
                if row and row["level"] == "table_cell":
                    item["best_level"] = LEVEL_LABELS["table_cell"]
                item["_crumb"] = (raw_path, chosen)
            # 상위 경로(장 > 절 > 항): 결과 문서의 문단 위계를 한 번에 읽어 고른 문단의 조상 제목을 붙인다.
            crumb_paths = {item["_crumb"][0] for item in final_items if item["_crumb"][1] is not None}
            crumb_rows = {}
            if crumb_paths:
                for row in self._select_paragraphs_for_paths(
                        "path, paragraph_index, level, level_rank, body", crumb_paths):
                    crumb_rows.setdefault(row["path"], []).append(row)
            for item in final_items:
                raw_path, chosen = item.pop("_crumb")
                trail = hierarchy_path.breadcrumb(crumb_rows.get(raw_path, ()), chosen) if chosen is not None else []
                item["breadcrumb"] = [step["text"] for step in trail]
                item["breadcrumb_text"] = hierarchy_path.breadcrumb_text(trail)
                item["context_hits"] = item.pop("_context_hits", hierarchy_path.context_term_hits(trail, terms))
        for idx, item in enumerate(final_items):
            # 스니펫·하이라이트는 화면에 보일 상위 결과에만 만든다(N214B 속도 개선, 결과 동일)
            target_para_body = item.pop("_target_body")
            t_folded = target_para_body.casefold()
            offsets = [t_folded.find(t.casefold()) for t in terms if t_folded.find(t.casefold()) >= 0]
            first = min(offsets) if offsets else 0
            start, end = max(0, first - 70), min(len(target_para_body), first + 180)
            snippet = target_para_body[start:end]
            spans = []
            for term in terms:
                match = re.search(re.escape(term), snippet, re.I)
                if match:
                    spans.append([match.start(), match.end()])
            ordered = {}
            for key, value in item.items():
                ordered[key] = value
                if key == "name":
                    ordered["snippet"] = snippet
                    ordered["highlights"] = spans
            item.clear()
            item.update(ordered)
            item["relevance_rank"] = idx + 1
            item.pop("_best_level_rank", None)
            item.pop("_document_date", None)
            item.pop("_bm25_score", None)
        return final_items

    @staticmethod
    def _keyword_match_expression(parsed):
        """Build FTS5 AND query while retaining phrases and excluding operators."""
        positive = [*parsed["terms"], *parsed["phrases"]]
        if not positive:
            raise ValueError("본문 검색어를 입력해 주세요.")
        def quote(term):
            return '"' + term.replace('"', '""') + '"'
        expression = " AND ".join(quote(term) for term in positive if len(term) >= 3)
        if not expression:
            return None
        if parsed["near"] is not None and len(positive) > 1:
            expression = " AND ".join(quote(term) for term in positive if len(term) >= 3)
        return expression

    def _keyword_rank_stamp(self):
        """본문검색 순위 보관이 아직 유효한지 판단하는 값(다른 연결의 쓰기, 문단 수, 가장 큰 rowid)."""
        with self._lock:
            return (self.db.execute("PRAGMA data_version").fetchone()[0],
                    self._paragraph_fts_row_count(), self._paragraph_fts_max_rowid())

    def _select_for_paths(self, sql, paths, params=()):
        """경로 목록으로 거르는 조회를 임시 표로 실행한다.

        `path IN (?, ?, …)`는 경로가 SQL 변수 한도(기본 32,766)를 넘으면 실패한다.
        sql의 `{paths}` 자리에 임시 표 조회를 넣어 경로 수와 무관하게 동작하게 한다.
        """
        with self._lock:
            opened_here = not self.db.in_transaction
            try:
                self.db.execute("CREATE TEMP TABLE IF NOT EXISTS keyword_path_filter(path TEXT PRIMARY KEY)")
                self.db.execute("DELETE FROM keyword_path_filter")
                self.db.executemany("INSERT OR IGNORE INTO keyword_path_filter(path) VALUES(?)",
                                    ((path,) for path in paths))
                return self.db.execute(
                    sql.format(paths="SELECT path FROM temp.keyword_path_filter"), tuple(params)).fetchall()
            finally:
                # 임시 표 쓰기로 열린 트랜잭션을 남겨 두면 이후 조회가 옛 스냅숏을 본다.
                if opened_here and self.db.in_transaction:
                    self.db.commit()

    # 후보 문단이 전체의 이 비율(1/4)을 넘으면 rowid 조회보다 전체 훑기 한 번이 빠르다.
    # 24만 문단 측정: rowid 조회는 문단당 약 4.5µs(8만 문단 0.39초), 전체 훑기는 0.2~0.7초로 8만 문단 근처에서 뒤집힌다.
    _PARAGRAPH_SCAN_RATIO = 4

    def _select_paragraphs_for_paths(self, columns, paths, where="", params=()):
        """경로 목록에 속한 문단을 rowid 순으로 읽는다.

        후보가 적으면 대응 표(content_paragraph_map)에서 rowid를 얻어 rowid로 읽고,
        후보가 많으면(전체의 1/4 초과) 문단 FTS를 한 번 훑는다. 결과와 순서는 두 방식이 같다.
        where는 `{body} LIKE ?`처럼 본문 열을 `{body}`로 적은 추가 조건이다. rowid 조회에서는
        LIKE가 FTS5로 넘어가 rowid마다 3글자 색인을 다시 찾지 않도록 `+body`로 바꿔 넣는다.
        MATCH 조건은 rowid 조회와 함께 쓰면 rowid마다 MATCH를 다시 실행하므로 여기에 넣지 않는다.
        """
        with self._lock:
            opened_here = not self.db.in_transaction
            try:
                self.db.execute("CREATE TEMP TABLE IF NOT EXISTS keyword_path_filter(path TEXT PRIMARY KEY)")
                self.db.execute("DELETE FROM keyword_path_filter")
                self.db.executemany("INSERT OR IGNORE INTO keyword_path_filter(path) VALUES(?)",
                                    ((path,) for path in paths))
                source = "SELECT path FROM temp.keyword_path_filter"
                state = self.db.execute("SELECT row_count FROM content_paragraph_map_state WHERE id=1").fetchone()
                total = state[0] if state else -1
                # 후보 문단 수는 분기점을 넘는지만 알면 되므로 거기까지만 센다(24만 문단에서 최대 약 20ms).
                ceiling = total // self._PARAGRAPH_SCAN_RATIO if total >= 0 and self._PARAGRAPH_SCAN_RATIO else max(total, 0)
                wanted = self.db.execute(
                    f"SELECT COUNT(*) FROM (SELECT 1 FROM content_paragraph_map WHERE path IN ({source}) LIMIT ?)",
                    (ceiling + 1,)).fetchone()[0]
                if total >= 0 and wanted <= ceiling:
                    condition = f"rowid IN (SELECT para_rowid FROM content_paragraph_map WHERE path IN ({source}))"
                    extra = where.format(body="+body")
                else:
                    condition = f"path IN ({source})"
                    extra = where.format(body="body")
                return self.db.execute(
                    f"SELECT {columns} FROM content_paragraph_fts WHERE {condition}"
                    + (f" AND {extra}" if extra else "") + " ORDER BY rowid", tuple(params)).fetchall()
            finally:
                if opened_here and self.db.in_transaction:
                    self.db.commit()

    def keyword_search(self, query, *, roots=None, limit=100, offset=0):
        """Docufinder식 키워드 검색. 위계 필드 없이 BM25로 문서 순위를 정한다."""
        self._ensure_paragraph_map()
        parsed = self.parse_query(query)
        positive = [*parsed["terms"], *parsed["phrases"]]
        if not positive:
            raise ValueError("본문 검색어를 입력해 주세요.")
        if type(limit) is not int or not 1 <= limit <= 500 or type(offset) is not int or offset < 0:
            raise ValueError("결과 범위를 확인해 주세요.")
        # 레거시 Office 그룹 확장
        groups = ({"hwp", "hwpx"}, {"doc", "docx"}, {"xls", "xlsx"}, {"ppt", "pptx"})
        extensions = set(parsed["ext"])
        for group in groups:
            if extensions & group:
                extensions.update(group)
        parsed["ext"] = extensions
        fts_positive = [t for t in parsed["terms"] if len(t) >= 3]
        match = " AND ".join('"' + t.replace('"', '""') + '"' for t in fts_positive if t not in parsed["phrases"])
        short_positive = [t for t in positive if len(t) < 3]
        allowed = [os.path.normcase(os.path.abspath(root)) for root in roots] if roots else None
        clauses, params = [], []
        candidate_paths = None
        for term in parsed["terms"]:
            if len(term) >= 3:
                clauses.append("content_paragraph_fts MATCH ?")
                params.append('"' + term.replace('"', '""') + '"')
            else:
                clauses.append("body LIKE ?"); params.append("%" + term + "%")
        for phrase in parsed["phrases"]:
            clauses.append("body LIKE ?"); params.append("%" + phrase + "%")
        where_positive = " AND ".join(clauses)
        # 쪽 넘김: 같은 검색어·범위의 순위 목록을 잠깐 보관해 다음 쪽은 위치 조회만 한다.
        rank_key = (query, tuple(allowed) if allowed is not None else None)
        rank_stamp = self._keyword_rank_stamp()
        cached = self._keyword_rank_cache
        if (cached is not None and cached[0] == rank_key and cached[1] == rank_stamp
                and time.monotonic() - cached[2] < KEYWORD_RANK_CACHE_SECONDS):
            ranked, valid_indices_by_path = cached[3], cached[4]
        else:
            cached = None
        if cached is None:
            with self._lock:
                # Short terms use the gram table only to narrow candidates; LIKE remains the
                # final substring check, preserving the old exact matching behavior.
                if short_positive:
                    short_candidates = None
                    for term in short_positive:
                        if len(term) == 2:
                            found = {r[0] for r in self.db.execute(
                                "SELECT path FROM content_short_grams WHERE gram=?", (term.casefold(),))}
                        else:
                            found = {r[0] for r in self.db.execute(
                                "SELECT path FROM content_paragraph_fts WHERE body LIKE ?", ("%" + term + "%",))}
                        short_candidates = found if short_candidates is None else short_candidates & found
                    if short_candidates is not None:
                        candidate_paths = short_candidates
                        if candidate_paths is not None and not candidate_paths:
                            return []
                if short_positive and candidate_paths is not None:
                    long_predicates = []
                    long_params = []
                    for term in parsed["terms"]:
                        if len(term) >= 3:
                            long_predicates.append("{body} LIKE ?")
                            long_params.append("%" + term + "%")
                    for phrase in parsed["phrases"]:
                        long_predicates.append("{body} LIKE ?")
                        long_params.append("%" + phrase + "%")
                    # 후보 문서가 적으면 rowid로, 많으면 전체 훑기 한 번으로 읽는다(대응 표와 FTS를 조인하지 않는다:
                    # 조인은 문단 FTS 전체 × 대응 표 전체로 풀려 수십 초가 걸렸다).
                    scored_rows = self._select_paragraphs_for_paths(
                        "path,name,level_rank,body,0.0 AS rank", candidate_paths,
                        " AND ".join(long_predicates), long_params)
                else:
                    scored_rows = self.db.execute(f"""
                        SELECT path, name, level_rank, body,
                               bm25(content_paragraph_fts, 0.0, 3.0, 2.0, 1.0, 0.0, 0.0, 1.0) AS rank
                        FROM content_paragraph_fts WHERE {where_positive}
                        ORDER BY rank ASC
                    """, params).fetchall()
                if candidate_paths is not None:
                    scored_rows = [r for r in scored_rows if r["path"] in candidate_paths]
            # FTS5 bm25 weights: filename=3, body=1. Add a hierarchy boost to the
            # paragraph score for levels 0..3, then proximity/phrase boosts per document.
            doc_scores = {}
            for scored in scored_rows:
                path = scored["path"]
                rank = float(scored["rank"])
                name_folded = (scored["name"] or "").casefold()
                # Filename is UNINDEXED in this legacy FTS5 schema, so apply the exact
                # requested 3:1 field weight as an explicit weighted occurrence count.
                body_term_count = sum((scored["body"] or "").casefold().count(term.casefold()) for term in positive)
                filename_term_count = sum(term.casefold() in name_folded for term in positive)
                rank -= 0.001 * (body_term_count + 3.0 * filename_term_count)
                level_rank = int(scored["level_rank"] if scored["level_rank"] is not None else LEVEL_ORDER.index("body"))
                if level_rank <= 3:
                    rank -= 0.002 * (4 - level_rank)
                prior = doc_scores.get(path)
                doc_scores[path] = min(rank, prior) if prior is not None else rank
            if short_positive and candidate_paths is not None:
                for candidate_path in candidate_paths:
                    doc_scores.setdefault(candidate_path, 0.0)
            rows = [{"path": path, "best_bm25": score} for path, score in doc_scores.items()]
            if short_positive and candidate_paths is not None:
                rows = [row for row in rows if row["path"] in candidate_paths]
            rows = rows[:100000]
            excluded = set()
            excluded_short = []
            with self._lock:
                for term in parsed["exclude"]:
                    if len(term) < 3:
                        excluded_short.append(term)
                    if len(term) >= 3:
                        expr = '"' + term.replace('"', '""') + '"'
                        try:
                            excluded.update(row[0] for row in self.db.execute(
                                "SELECT DISTINCT path FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?", (expr,)))
                        except sqlite3.OperationalError:
                            # MATCH를 못 쓸 때만 LIKE로 전체를 훑는다. 성공했으면 같은 말을 다시 찾지 않는다.
                            excluded.update(row[0] for row in self.db.execute(
                                "SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?", ("%" + term + "%",)))
                    # 2글자 이하 제외어는 아래에서 후보 문서 본문(candidate_texts)으로 걸러낸다(전체 훑기 불필요).
                metas ={row["path"]: row for row in self._select_for_paths(
                    "SELECT path,name,extension,mtime,reference_date FROM content_meta WHERE path IN ({paths})",
                    (row["path"] for row in rows))} if rows else {}
            ranked = []
            valid_indices_by_path = {}
            candidate_path_set = {r["path"] for r in rows}
            # 후보 문서의 문단을 한 번만 읽어, 합친 본문(candidate_texts)과 문단별 검사에 함께 쓴다.
            paragraph_rows_by_path = {}
            with self._lock:
                if candidate_path_set:
                    for paragraph_row in self._select_paragraphs_for_paths(
                            "path,paragraph_index,body,level_rank", candidate_path_set):
                        paragraph_rows_by_path.setdefault(paragraph_row["path"], []).append(paragraph_row)
            # 문서의 모든 문단을 저장 순서(rowid 순)대로 줄바꿈으로 잇는다. 예전 group_concat과 같게 NULL 본문은 뺀다.
            candidate_texts = {path: "\n".join(p["body"] for p in paragraphs if p["body"] is not None)
                               for path, paragraphs in paragraph_rows_by_path.items()}
            rows = [row for row in rows if all(term.casefold() in candidate_texts.get(row["path"], "").casefold()
                                               for term in parsed["terms"]) and all(
                    phrase.casefold() in candidate_texts.get(row["path"], "").replace('"', '').casefold()
                    for phrase in parsed["phrases"]) and not any(
                    term.casefold() in candidate_texts.get(row["path"], "").casefold()
                    for term in [*parsed["exclude"], *excluded_short])]
            paragraph_rows_by_path = {row["path"]: paragraph_rows_by_path[row["path"]] for row in rows
                                      if row["path"] in paragraph_rows_by_path}
            # 근접 검색은 동일 문단 내 양성 용어 사이의 단어 간격으로 제한한다.
            if parsed["near"] is not None and len(positive) > 1:
                near = parsed["near"]
                filtered = []
                for row in rows:
                    paragraphs_for_near = paragraph_rows_by_path.get(row["path"], [])
                    proximity_terms = [*parsed["terms"]]
                    if len(proximity_terms) < 2:
                        proximity_terms = positive
                    if any(_terms_are_near(paragraph["body"] or "", proximity_terms, near) for paragraph in paragraphs_for_near):
                        filtered.append(row)
                rows = filtered
            for row in rows:
                path = row["path"]
                para_candidates = paragraph_rows_by_path.get(path, [])
                valid_para_indices = {int(p["paragraph_index"]) for p in para_candidates
                    if all(term.casefold() in (p["body"] or "").casefold() for term in parsed["terms"])
                    and all(phrase.casefold() in (p["body"] or "").casefold() for phrase in parsed["phrases"])}
                if not valid_para_indices:
                    continue
                meta = metas.get(path)
                if not meta or path in excluded or os.path.abspath(path) in excluded:
                    continue
                if excluded_name(meta["name"] or "") or excluded_name(os.path.basename(path)):
                    continue
                normalized = os.path.normcase(os.path.abspath(path))
                if any(part.casefold() == "$recycle.bin" for part in Path(path).parts):
                    continue
                if allowed is not None and not any(os.path.commonpath((normalized, root)) == root for root in allowed
                        if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(normalized)[0].casefold()):
                    continue
                ext = (meta["extension"] or Path(path).suffix.lstrip(".")).lower()
                if parsed["ext"] and ext not in parsed["ext"]:
                    continue
                if parsed["path"] and not all(value.casefold() in path.casefold() for value in parsed["path"]):
                    continue
                ref_date = meta["reference_date"] or ""
                if ref_date and len(ref_date) == 4:
                    ref_date += "-01-01"
                elif ref_date and len(ref_date) == 7:
                    ref_date += "-01"
                if not ref_date and meta["mtime"]:
                    ref_date = date.fromtimestamp(meta["mtime"]).isoformat()
                text_date = re.search(r"\b(\d{4}-\d{2}-\d{2})\b", candidate_texts.get(path, ""))
                if text_date:
                    ref_date = text_date.group(1)
                after = parsed["after"]
                before = parsed["before"]
                if after and len(after) == 4: after += "-01-01"
                elif after and len(after) == 7: after += "-01"
                if before and len(before) == 4: before += "-12-31"
                elif before and len(before) == 7: before += "-31"
                if after and (not ref_date or ref_date < after):
                    continue
                if before and (not ref_date or ref_date > before):
                    continue
                # Proximity bonus: reward the closest pair of positive terms in a single
                # paragraph. Explicit quoted phrases also get a phrase-match bonus.
                proximity_bonus = 0.0
                phrase_bonus = 0.0
                if parsed["near"] is None or len(positive) > 1 or parsed["phrases"]:
                    proximity_paragraphs = para_candidates
                else:
                    proximity_paragraphs = ()
                for paragraph in proximity_paragraphs:
                    text = paragraph["body"] or ""
                    if all(term.casefold() in text.casefold() for term in positive):
                        terms_for_distance = parsed["terms"] or positive
                        words = [m.group().casefold() for m in re.finditer(r"\S+", text)]
                        positions = [[i for i, word in enumerate(words) if term.casefold() in word]
                                     for term in terms_for_distance]
                        if positions and all(positions):
                            distance = _min_position_span(positions)
                            proximity_bonus = max(proximity_bonus, 0.20 / (1 + distance))
                        if parsed["phrases"] and all(p.casefold() in text.casefold() for p in parsed["phrases"]):
                            phrase_bonus = max(phrase_bonus, 0.35)
                filename_bonus = 0.0
                ranked.append((float(row["best_bm25"]) - proximity_bonus - phrase_bonus - filename_bonus, path, meta))
                valid_indices_by_path[path] = valid_para_indices
            ranked.sort(key=lambda value: (value[0], value[1].casefold()))
            self._keyword_rank_cache = (rank_key, rank_stamp, time.monotonic(), ranked, valid_indices_by_path)
        selected = ranked[offset:offset + limit]
        results = []
        # 위치 조회는 결과마다 표를 훑지 않고 선택된 문서 전체를 한 번에 읽어 문서별 상위 3개를 고른다.
        conditions, match_params = [], []
        if match and not parsed["phrases"]:
            conditions.append("content_paragraph_fts MATCH ?"); match_params.append(match)
        else:
            for term in parsed["terms"]:
                if len(term) >= 3:
                    conditions.append("content_paragraph_fts MATCH ?")
                    match_params.append('"' + term.replace('"', '""') + '"')
                else:
                    conditions.append("body LIKE ?"); match_params.append("%" + term + "%")
            for phrase in parsed["phrases"]:
                conditions.append("body LIKE ?"); match_params.append("%" + phrase.replace('"', '""') + "%")
        for term in short_positive:
            conditions.append("body LIKE ?"); match_params.append("%" + term + "%")
        match_where = " AND ".join(conditions)
        rank_column = ("0.0" if short_positive
                       else "bm25(content_paragraph_fts, 0.0, 3.0, 2.0, 1.0, 0.0, 0.0, 1.0)")
        located_by_path = {}
        breadcrumb_by_path = {}
        if selected:
            if any("MATCH" in condition for condition in conditions):
                # MATCH(와 bm25)는 rowid 조회와 함께 쓰면 rowid마다 다시 실행되므로 MATCH로 찾고 경로로 거른다.
                located_rows = self._select_for_paths(
                    f"SELECT path, paragraph_index, body, location_label, section, {rank_column} AS rank"
                    f" FROM content_paragraph_fts WHERE {match_where} AND path IN ({{paths}})",
                    (path for _, path, _ in selected), match_params)
            else:
                located_rows = self._select_paragraphs_for_paths(
                    f"path, paragraph_index, body, location_label, section, {rank_column} AS rank",
                    (path for _, path, _ in selected),
                    " AND ".join(condition.replace("body LIKE", "{body} LIKE") for condition in conditions),
                    match_params)
            for located in located_rows:
                located_by_path.setdefault(located["path"], []).append(located)
            # Fetch each displayed document's paragraph hierarchy in one batched
            # lookup. The rank cache remains limited to ranking and valid indices.
            with self._lock:
                for row in self._select_paragraphs_for_paths(
                        "path,paragraph_index,body,level,level_rank",
                        (path for _, path, _ in selected)):
                    breadcrumb_by_path.setdefault(row["path"], []).append(row)
        for score, path, meta in selected:
            # 위치는 그 문서 자신의 일치 문단으로 거른다(마지막으로 처리한 문서의 집합을 쓰면 안 된다).
            valid_para_indices = valid_indices_by_path.get(path, ())
            paragraphs = sorted((p for p in located_by_path.get(path, ())
                                 if int(p["paragraph_index"]) in valid_para_indices),
                                key=lambda p: (float(p["rank"]), int(p["paragraph_index"])))[:3]
            paragraphs = [p for p in paragraphs if int(p["paragraph_index"]) in valid_para_indices]
            crumbs = breadcrumb_by_path.get(path, ())
            locations = []
            for paragraph in paragraphs:
                text = paragraph["body"] or ""
                folded = text.casefold()
                starts = [folded.find(term.casefold()) for term in positive if folded.find(term.casefold()) >= 0]
                if not starts:
                    continue
                first = min(starts); start = max(0, first - 70); end = min(len(text), first + 180)
                snippet = text[start:end]
                highlights = [[m.start(), m.end()] for term in positive
                              for m in list(re.finditer(re.escape(term), snippet, re.I))]
                label = paragraph["location_label"] or f"문단 {int(paragraph['paragraph_index']) + 1}"
                location = {"page": None, "sheet": None, "row": None, "label": label}
                page_match = re.search(r"(\d+)쪽", label)
                row_match = re.search(r"(.+?)\s+(\d+)행", label)
                line_match = re.fullmatch(r"(\d+)행", label)
                if page_match:
                    location["page"] = int(page_match.group(1))
                if row_match:
                    location["sheet"], location["row"] = row_match.group(1), int(row_match.group(2))
                elif line_match:
                    location["row"] = int(line_match.group(1))
                trail = hierarchy_path.breadcrumb(crumbs, int(paragraph["paragraph_index"]))
                locations.append({"paragraph_index": int(paragraph["paragraph_index"]), "snippet": snippet,
                                  "highlights": highlights, "position": location,
                                  "breadcrumb": [step["text"] for step in trail],
                                  "breadcrumb_text": hierarchy_path.breadcrumb_text(trail)})
            primary_breadcrumb = locations[0]["breadcrumb"] if locations else []
            primary_breadcrumb_text = locations[0]["breadcrumb_text"] if locations else ""
            results.append({"path": path, "name": meta["name"] or os.path.basename(path),
                            "extension": meta["extension"] or Path(path).suffix.lstrip("."),
                            "mtime": meta["mtime"], "folder_path": os.path.dirname(path),
                            "folder_name": os.path.basename(os.path.dirname(path)),
                            "locations": locations, "breadcrumb": primary_breadcrumb,
                            "breadcrumb_text": primary_breadcrumb_text,
                            "snippet": locations[0]["snippet"] if locations else "",
                            "highlights": locations[0]["highlights"] if locations else [],
                            "position": locations[0]["position"] if locations else {"page": None, "sheet": None, "row": None},
                            "score": score, "relevance_rank": offset + len(results) + 1})
        return results

    def search_files(self, query, *, roots=None, limit=500):
        """Immediate in-memory filename cache populated from indexed metadata."""
        words = [w.casefold() for w in query.split() if w]
        if not words: return []
        allowed = [os.path.normcase(os.path.abspath(r)) for r in roots] if roots else None
        with self._lock: rows = self.db.execute("SELECT path,name,size,mtime,extension FROM content_meta ORDER BY name COLLATE NOCASE").fetchall()
        out = []
        for r in rows:
            # 이름이 맞지 않는 행은 경로 정규화·제외 판정을 하지 않고 넘어간다(결과는 같다).
            name_folded = (r["name"] or "").casefold()
            if not all(w in name_folded for w in words):
                continue
            p = os.path.abspath(r["path"])
            if excluded_name(r["name"] or "") or excluded_name(os.path.basename(p)):
                continue
            if (allowed is None or any(os.path.commonpath((os.path.normcase(p), root)) == root for root in allowed if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(p)[0].casefold())):
                out.append({"path": p, "name": r["name"] or Path(p).name, "size": r["size"], "mtime": r["mtime"], "extension": r["extension"] or Path(p).suffix.lower().lstrip(".")})
                if len(out) >= limit: break
        return out

    def errors(self, roots=None):
        with self._lock:
            rows = self.db.execute("SELECT path,reason FROM content_errors ORDER BY path").fetchall()
        allowed = [os.path.normcase(os.path.abspath(r)) for r in roots] if roots else None
        return [dict(r) for r in rows if not any(p.casefold() == "$recycle.bin" for p in Path(r["path"]).parts)
                and not excluded_name(os.path.basename(r["path"]))
                and (allowed is None or any(os.path.commonpath((os.path.normcase(os.path.abspath(r["path"])), root)) == root
                                            for root in allowed if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(os.path.abspath(r["path"]))[0].casefold()))]

    def related_candidates(self, path, roots=None):
        path_str = os.fspath(path)
        path_abs = os.path.abspath(path_str)
        path_variants = list(dict.fromkeys([
            path_str,
            path_abs,
            os.path.normpath(path_str),
            os.path.normpath(path_abs),
            path_str.replace("\\", "/"),
            path_str.replace("/", "\\"),
            path_abs.replace("\\", "/"),
            path_abs.replace("/", "\\"),
        ]))
        placeholders = ",".join("?" for _ in path_variants)

        with self._lock:
            selected = None
            # 1. content_paragraph_fts에서 문단 결합 본문 조회
            try:
                para_row = self.db.execute(f"""
                    SELECT GROUP_CONCAT(body, CHAR(10)) AS body,
                           (SELECT reference_date FROM content_meta WHERE path IN ({placeholders}) LIMIT 1) AS reference_date,
                           path
                    FROM (SELECT path, body FROM content_paragraph_fts WHERE path IN ({placeholders}) ORDER BY paragraph_index ASC)
                    GROUP BY path
                    LIMIT 1
                """, (*path_variants, *path_variants)).fetchone()
                if para_row and para_row["body"]:
                    selected = para_row
            except sqlite3.OperationalError:
                pass

            # 2. 없으면 content_fts (구형 단일 본문 색인) 조회
            if not selected:
                try:
                    selected = self.db.execute(f"""
                        SELECT f.body,
                               COALESCE(m.reference_date, (SELECT reference_date FROM content_meta WHERE path IN ({placeholders}) LIMIT 1)) AS reference_date,
                               f.path
                        FROM content_fts f
                        LEFT JOIN content_meta m ON m.path=f.path
                        WHERE f.path IN ({placeholders})
                        LIMIT 1
                    """, (*path_variants, *path_variants)).fetchone()
                except sqlite3.OperationalError:
                    pass

            if not selected:
                return None, []

            matched_path = selected["path"] if ("path" in selected.keys() and selected["path"]) else path_str
            exclude_paths = set(path_variants)
            exclude_paths.add(matched_path)
            exclude_paths.add(os.path.abspath(matched_path))

            seen_paths = set(exclude_paths)
            rows = []

            # (1) content_paragraph_fts에서 후보 수집
            try:
                ex_list = list(seen_paths)
                ex_placeholders = ",".join("?" for _ in ex_list)
                p_rows = self.db.execute(f"""
                    SELECT p.path, p.name, GROUP_CONCAT(p.body, CHAR(10)) AS body, m.reference_date
                    FROM (SELECT path, name, body FROM content_paragraph_fts WHERE path NOT IN ({ex_placeholders}) ORDER BY paragraph_index ASC) p
                    LEFT JOIN content_meta m ON m.path=p.path
                    GROUP BY p.path
                """, ex_list).fetchall()
                for r in p_rows:
                    rows.append(r)
                    seen_paths.add(r["path"])
                    seen_paths.add(os.path.abspath(r["path"]))
            except sqlite3.OperationalError:
                pass

            # (2) content_fts에서 아직 수집되지 않은 구형 문서 후보 수집
            try:
                ex_list = list(seen_paths)
                ex_placeholders = ",".join("?" for _ in ex_list)
                legacy_rows = self.db.execute(f"""
                    SELECT f.path, f.name, f.body, m.reference_date
                    FROM content_fts f
                    LEFT JOIN content_meta m ON m.path=f.path
                    WHERE f.path NOT IN ({ex_placeholders})
                """, ex_list).fetchall()
                for r in legacy_rows:
                    rows.append(r)
                    seen_paths.add(r["path"])
                    seen_paths.add(os.path.abspath(r["path"]))
            except sqlite3.OperationalError:
                pass

        allowed = [os.path.normcase(os.path.abspath(r)) for r in roots] if roots else None
        candidates = []
        for row in rows:
            row_path = row["path"]
            if excluded_name(row["name"] or "") or excluded_name(os.path.basename(row_path or "")):
                continue
            if allowed is not None:
                item_path = os.path.normcase(os.path.abspath(row_path))
                if not any(
                    os.path.commonpath((item_path, root)) == root
                    for root in allowed
                    if os.path.splitdrive(root)[0].casefold() == os.path.splitdrive(os.path.abspath(row_path))[0].casefold()
                ):
                    continue
            candidates.append(dict(row))
        return dict(selected) if selected else None, candidates
