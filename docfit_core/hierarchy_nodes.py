"""추출된 문단과 기존 위계 결과를 원본 접근 없이 검색 트리로 정규화한다."""
from __future__ import annotations

import copy
import re

ROLE_RANK = {"document_title": 0, "title": 0, "overview": 1,
             "chapter": 2, "section": 3, "topic": 4, "detail": 4,
             "square": 5, "circle": 6, "dash": 7, "asterisk": 8,
             "body": 9, "table_cell": 9, "paragraph": 9}
KIND = {"title": "document", "document_title": "document", "chapter": "chapter",
        "section": "section", "topic": "item", "detail": "item"}
_MARKER = re.compile(r"^\s*(□|ㅇ|○|-|[Ⅰ-Ⅿ]+|[IVXLCDM]+[.)]?|\d+(?:\.\d+)*[.)]?|[가-하][.)])(?:\s|$)")


def _proposal(p, previous, following):
    """기호는 후보일 뿐: 스타일 또는 인접 문단의 들여쓰기 근거를 요구한다."""
    match = _MARKER.match(p.get("text", p.get("body", "")))
    if not match:
        return "body", []
    if p.get("is_table_cell") or p.get("level") == "table_cell":
        return "body", []
    marker = match[1]
    style = p.get("paragraph_style") or {}
    name = str(p.get("style_name", style.get("name", ""))).casefold()
    styled = p.get("outline_level") is not None or any(x in name for x in ("heading", "제목", "개요"))
    indent = p.get("indent", style.get("left", style.get("left_margin")))
    neighbors = [x for x in (previous, following) if x]
    context = indent is not None and any(
        x.get("indent", (x.get("paragraph_style") or {}).get("left")) is not None
        and x.get("indent", (x.get("paragraph_style") or {}).get("left")) != indent
        for x in neighbors)
    evidence = [x for x, ok in (("paragraph_style", styled), ("neighbor_indent", context)) if ok]
    if not evidence:
        return "body", []
    role = {"□": "square", "ㅇ": "circle", "○": "circle", "-": "dash"}.get(marker)
    if role is None:
        role = "chapter" if re.fullmatch(r"[Ⅰ-ⅯIVXLCDM]+[.)]?", marker) else "section"
    return role, evidence


def normalize_document(document: dict, *, document_name: str = "") -> dict:
    """nodes 또는 Rust/SQLite paragraphs 행을 받는다. level/role/parent_id는 우선한다.

    페이지는 논리 계층과 교차하므로 root의 자식이며 page_ids로 연결한다.
    표도 table_id로 참조한다. 문장 분리는 구두점 기반 근사이며 원문 offset을 보존한다.
    """
    structure = document.get("document_structure", document)
    legacy = "nodes" in structure
    source = copy.deepcopy(structure.get("nodes" if legacy else "paragraphs", []))
    used = {p["id"] for p in source if p.get("id") is not None}
    serial = 0

    def fresh(label):
        nonlocal serial
        while True:
            serial += 1
            value = f"@{label}:{serial}"
            if value not in used:
                used.add(value)
                return value

    root_id = fresh("document")
    root = {"id": root_id, "kind": "document", "text": document_name or structure.get("document_name", ""),
            "parent_id": None, "synthetic": True}
    nodes, diagnostics, stack, pages, tables = [root], [], [], {}, {}
    source_ids = []
    for i, p in enumerate(source):
        if p.get("id") is None:
            p["id"] = fresh("source")
        elif p["id"] in source_ids:
            raise ValueError("중복 위계 노드 id")
        source_ids.append(p["id"])
        p["text"] = p.get("text", p.get("body", ""))
        proposed, evidence = _proposal(p, source[i - 1] if i else None,
                                        source[i + 1] if i + 1 < len(source) else None)
        level = p.get("level")
        role = p.get("role") or (level if isinstance(level, str) and level in ROLE_RANK else None)
        numeric = isinstance(level, (int, float)) and not isinstance(level, bool)
        if role is None:
            role = ({0: "body", 1: "chapter", 2: "section", 3: "detail",
                     4: "square", 5: "circle", 6: "dash"}.get(level, "body")
                    if numeric and not legacy else proposed)
        rank = level if numeric and (legacy or level > 0) else ROLE_RANK.get(role, 9)
        authoritative = p.get("role") is not None or level is not None
        if authoritative and proposed != role and (evidence or _MARKER.match(p["text"])):
            diagnostics.append({"id": p["id"], "code": "existing_hierarchy_preferred",
                                "existing_role": role, "existing_level": level,
                                "proposed_role": proposed, "evidence": evidence})
        p.update(role=role, kind=KIND.get(role, "paragraph"), rank=rank,
                 hierarchy_source="existing" if authoritative else "context" if evidence else "unclassified",
                 evidence=evidence)
        if "level" not in p:
            p["level"] = rank
        while stack and stack[-1]["rank"] >= rank:
            stack.pop()
        if "parent_id" not in p:
            p["parent_id"] = stack[-1]["id"] if stack else root_id
        elif p["parent_id"] is None:
            p["parent_id"] = root_id
        if role not in {"body", "table_cell", "paragraph", "overview", "asterisk"}:
            stack.append(p)
        p.setdefault("paragraph", p.get("paragraph_index", p.get("sequence", i)))
        position = copy.deepcopy(p.get("position") or {})
        for key in ("page", "sheet", "row", "cell", "section", "paragraph", "table_order", "location_kind", "location_label"):
            position.setdefault(key, p.get(key))
        # 위치 라벨에 명시된 페이지 번호만 채택한다.
        page_match = re.search(r"(\d+)쪽", str(position.get("location_label") or ""))
        if position.get("page") is None and page_match:
            position["page"] = int(page_match[1])
        p["position"] = position
        page = position.get("page")
        p["page_ids"] = []
        if page is not None:
            if page not in pages:
                pages[page] = {"id": fresh("page"), "kind": "page", "text": f"{page}쪽",
                               "parent_id": root_id, "page": page, "synthetic": True}
                nodes.append(pages[page])
            p["page_ids"] = [pages[page]["id"]]
        table = p.get("table_id", p.get("table_order"))
        p["table_id"] = None
        if table is not None:
            key = (p.get("section"), table)
            if key not in tables:
                tables[key] = {"id": fresh("table"), "kind": "table", "text": f"표 {table}",
                               "parent_id": p["parent_id"], "position": position, "synthetic": True}
                nodes.append(tables[key])
            p["table_id"] = tables[key]["id"]
        elif p.get("is_table_cell") or role == "table_cell":
            diagnostics.append({"id": p["id"], "code": "table_identity_unavailable"})
        nodes.append(p)
        if p["kind"] == "paragraph":
            for match in re.finditer(r"\S[\s\S]*?(?:[.!?。！？](?=\s|$)|$)", p["text"]):
                if match.group().strip():
                    nodes.append({"id": fresh("sentence"), "kind": "sentence", "text": match.group(),
                                  "parent_id": p["id"], "start": match.start(), "end": match.end(),
                                  "position": position, "synthetic": True})
    by_id = {n["id"]: n for n in nodes}
    for node in nodes:
        seen = {node["id"]}
        parent_id = node.get("parent_id")
        while parent_id is not None:
            if parent_id not in by_id:
                raise ValueError("존재하지 않는 상위 위계 노드")
            if parent_id in seen:
                raise ValueError("위계 노드 순환 참조")
            seen.add(parent_id)
            parent_id = by_id[parent_id].get("parent_id")
        node["child_ids"] = []
    for node in nodes:
        if node.get("parent_id") is not None:
            by_id[node["parent_id"]]["child_ids"].append(node["id"])
    return {"version": 2, "document_id": root_id, "nodes": nodes,
            "source_ids": source_ids, "diagnostics": diagnostics,
            "warnings": list(structure.get("warnings", [])),
            "limitations": ["문장 경계는 구두점 기반 근사", "없는 페이지·표 번호는 추정하지 않음",
                            "원본 포맷 추출은 기존 색인 담당; 이 모듈은 추출된 구조만 처리"]}
