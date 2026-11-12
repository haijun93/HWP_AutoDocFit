"""붙임 쪽 경계: 붙임(별첨·첨부)이 시작하는 본문 문단을 HWPX XML에서 찾는다.

쪽 수 맞춤은 붙임 쪽을 앞 쪽과 섞지 않는다. 붙임 표시는 세 가지 모양으로 온다.

- 문단 글: ``[붙임 1]``·``붙임 2``처럼 표시만 있는 문단, 또는 쪽 나눔 문단의 ``붙임 1  ○○ 계획``.
- 표: 쪽 나눔 문단의 ``붙임 1 | 제목`` 머리 표(행 2개 이하)의 첫 칸 글.
- 도형: 글자처럼 취급하는(``treatAsChar="1"``) 사각형 등 글상자의 글.

공문 끝의 붙임 목록(``붙임  계획서 1부.  끝.``)은 마침표로 끝나므로 붙임 쪽이 아니다.
한/글이 없어도 시험할 수 있게 순수 함수로 둔다. XML은 defusedxml로만 읽는다.
"""
from __future__ import annotations

import re
from pathlib import Path

from defusedxml.ElementTree import fromstring as _fromstring

from .attachment_block import is_numbered_item
from .hwpx import open_hwpx_safely

_NAMES = r"(?:붙임|별첨|첨부)"
# 표시만 있는 글: '붙임', '붙임 1', '[붙임 1]', '〈별첨 2〉'
_LABEL_ONLY = re.compile(rf"^[\[<〈【(]?\s*{_NAMES}\s*\d*\s*[\]>〉】)]?$")
# 괄호로 감싼 표시로 시작하는 글: '[붙임 1] 세부 일정'
_BRACKETED = re.compile(rf"^[\[<〈【]\s*{_NAMES}\s*\d*\s*[\]>〉】]")
# 쪽 나눔 문단에서만 받는 느슨한 표시: '붙임 1  ○○ 계획', '붙임1 | 제목'
_LOOSE = re.compile(rf"^{_NAMES}\s*\d*(?=\s|$|[:：|│])")
_MAX_LEN = 60
_MAX_HEAD_TABLE_ROWS = 2
_SHAPES = {"rect", "ellipse", "polygon", "curve", "arc", "container"}

KIND_TEXT = "문단"
KIND_TABLE = "표"
KIND_SHAPE = "도형"


def is_attachment_label(text: str | None, *, page_break: bool = False) -> bool:
    """글이 붙임 쪽 표시인지. page_break(문단 앞 쪽 나눔)면 '붙임 1  제목'처럼 뒤에 글이 있어도 받는다."""
    value = " ".join(str(text or "").split())
    if not value or len(value) > _MAX_LEN:
        return False
    if _LABEL_ONLY.match(value) or _BRACKETED.match(value):
        return True
    return bool(page_break and _LOOSE.match(value) and not value.endswith("."))


def _local(element) -> str:
    return str(element.tag).rsplit("}", 1)[-1]


def _text(element) -> str:
    """요소 아래 글(hp:t, 꼬리 글 포함). 표·글상자 같은 개체 안은 들어가지 않는다."""
    parts: list[str] = []

    def visit(node, root=False):
        name = _local(node)
        if not root and (name in ("tbl", "pic") or name in _SHAPES):
            return
        if name == "t":
            parts.append(node.text or "")
            for child in node:
                if _local(child) == "tab":
                    parts.append(" ")
                parts.append(child.tail or "")
            return
        for child in node:
            visit(child)

    visit(element, True)
    return "".join(parts)


def _objects(paragraph):
    """본문 문단의 run 바로 아래 개체들(표·도형 등), 문서 순서."""
    for run in paragraph:
        if _local(run) != "run":
            continue
        for child in run:
            yield child


def _table_first_cell_text(table) -> str | None:
    """머리 표(행 2개 이하)의 첫 칸 글. 머리 표가 아니면 None."""
    rows = [row for row in table if _local(row) == "tr"]
    try:
        count = int(table.get("rowCnt") or len(rows))
    except ValueError:
        count = len(rows)
    if not rows or count > _MAX_HEAD_TABLE_ROWS:
        return None
    for cell in rows[0]:
        if _local(cell) == "tc":
            return " ".join(_text(p) for p in cell.iter() if _local(p) == "p")
    return None


def _shape_text(shape) -> str | None:
    """글자처럼 취급하는 도형의 글상자 글. 떠 있는 도형이거나 글상자가 없으면 None."""
    position = next((child for child in shape if _local(child) == "pos"), None)
    if position is None or position.get("treatAsChar") != "1":
        return None
    boxes = [node for node in shape.iter() if _local(node) == "drawText"]
    if not boxes:
        return None
    return " ".join(_text(p) for box in boxes for p in box.iter() if _local(p) == "p")


def attachment_kind(paragraph) -> str | None:
    """본문 문단이 붙임 쪽을 시작하면 어떤 표시로 알았는지(문단·표·도형), 아니면 None."""
    page_break = paragraph.get("pageBreak") == "1"
    for obj in _objects(paragraph):
        name = _local(obj)
        if name == "tbl":
            text = _table_first_cell_text(obj)
            if text is not None and is_attachment_label(text, page_break=True):
                return KIND_TABLE
        elif name in _SHAPES:
            text = _shape_text(obj)
            if text is not None and is_attachment_label(text, page_break=True):
                return KIND_SHAPE
    if is_attachment_label(_text(paragraph), page_break=page_break):
        return KIND_TEXT
    return None


def find_attachment_starts(paragraphs) -> list[tuple[int, str]]:
    """본문 직속 문단 목록에서 붙임 쪽을 시작하는 (문단 순번, 표시 종류)."""
    paragraphs = list(paragraphs)
    found = []
    for index, paragraph in enumerate(paragraphs):
        kind = attachment_kind(paragraph)
        if kind == KIND_TEXT and _numbered_list_follows(paragraphs, index):
            continue
        if kind:
            found.append((index, kind))
    return found


def _numbered_list_follows(paragraphs, index) -> bool:
    """'붙임' 한 줄 뒤에 '1. 계획서 1부.' 같은 공문 붙임 목록이 오는지(그러면 붙임 쪽이 아니다)."""
    for paragraph in paragraphs[index + 1:index + 3]:
        value = _text(paragraph).strip()
        if value:
            return is_numbered_item(value)
    return False


def body_paragraphs(path: str | Path) -> list:
    """HWPX 본문(모든 구역)의 직속 문단을 문서 순서로. 한/글 본문 문단 번호와 같은 순번이다."""
    from .style_inventory import _sections

    paragraphs = []
    with open_hwpx_safely(path) as archive:
        for name in _sections(archive):
            paragraphs.extend(p for p in _fromstring(archive.read(name)) if _local(p) == "p")
    return paragraphs


def table_paragraph_indices(paragraphs) -> set[int]:
    """본문 표가 놓인 문단 순번(한/글 본문 표 위치와 맞춰 번호가 같은지 확인하는 데 쓴다)."""
    return {i for i, p in enumerate(paragraphs) if any(_local(x) == "tbl" for x in _objects(p))}


def pull_up_blocked(last_page: int | None, boundary_page: int | None, page_break: bool,
                    lines_before: int | None, max_lines: int) -> bool:
    """마지막 쪽 줄을 앞 쪽으로 당기는 쪽 수 맞춤이 처음부터 불가능한지.

    마지막 쪽이 쪽 나눔(page_break)으로 시작하는 붙임 쪽이면 그 줄은 간격을 줄여도 앞 쪽으로 올라오지
    않는다. 쪽 수가 줄려면 붙임 앞 내용이 한 쪽을 통째로 비워야 하는데, 붙임 앞 쪽에 걸린 줄
    (lines_before)이 당겨올 기준(max_lines)보다 많으면 불가능으로 본다. 줄 수를 모르면(None) 판정하지 않는다.
    쪽 나눔이 없는 붙임은 간격을 줄이면 함께 올라오므로 막지 않는다.
    """
    return bool(last_page and boundary_page == last_page and page_break
                and lines_before is not None and lines_before > max_lines)


def report_fit_impossible(target_pages: int, page_break_starts: int) -> bool:
    """보고서를 target_pages쪽 안에 담는 것이 처음부터 불가능한지.

    보고서 첫머리(제목) 뒤에서 쪽 나눔으로 새 쪽을 시작하는 붙임이 page_break_starts개면 보고서는
    붙임 앞 내용 1쪽 + 붙임마다 1쪽 이상을 차지한다(간격·줄간격을 줄여도 쪽 나눔은 남는다).
    """
    return page_break_starts > 0 and 1 + page_break_starts > max(1, int(target_pages or 1))
