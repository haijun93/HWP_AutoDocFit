"""문서 계층을 보존하는 오프라인 검색 인덱스.

벡터 DB나 외부 서비스 없이 ``document_structure.analyze`` 결과를 검색용
레코드로 바꾼다. 키워드 점수와 상위 제목 경로 점수를 함께 사용하므로,
본문에만 검색어가 있어도 결과에는 문서의 장·절·항 경로가 포함된다.
"""
from __future__ import annotations

import math
import re
import copy
from collections import Counter
from .hierarchy_nodes import normalize_document

_TOKEN = re.compile(r"[0-9A-Za-z가-힣一-龥]+")


def _tokens(value: str) -> list[str]:
    return [x.casefold() for x in _TOKEN.findall(value or "")]


def build_index(document: dict, *, document_name: str = "") -> dict:
    """계층 분석 결과를 검색 인덱스로 변환한다.

    입력은 ``document_structure`` 자체 또는 ``{"document_structure": ...}``
    형태를 모두 허용한다. 원본 XML을 다시 읽지 않으며 입력 딕셔너리를
    변경하지 않는다.
    """
    normalized = normalize_document(document, document_name=document_name)
    by_id = {node["id"]: node for node in normalized["nodes"]}
    nodes = [by_id[node_id] for node_id in normalized["source_ids"]]
    records = []
    for node in nodes:
        chain = []
        parent = by_id.get(node.get("parent_id"))
        while parent is not None:
            if not parent.get("synthetic"):
                chain.append(parent)
            parent = by_id.get(parent.get("parent_id"))
        chain.reverse()
        path = [*(item.get("text", "") for item in chain), node.get("text", "")]
        text = " ".join(path)
        records.append({
            "id": node.get("id"), "text": node.get("text", ""),
            "role": node.get("role"), "level": node.get("level"),
            "parent_id": node.get("parent_id"), "path": path,
            "breadcrumb": " > ".join(x for x in path if x),
            "tokens": _tokens(text), "section": node.get("section"),
            "paragraph": node.get("paragraph"),
            "kind": node["kind"], "rank": node["rank"],
            "position": node["position"], "page_ids": node["page_ids"],
            "table_id": node["table_id"], "child_ids": node["child_ids"],
            "normalized_parent_id": node["parent_id"],
        })
    # 기존 루트 parent_id=None 계약을 유지한다.
    for record in records:
        if record["parent_id"] == normalized["document_id"]:
            record["parent_id"] = None
    return {"version": 2, "document_name": document_name,
            "records": records, "token_count": sum(len(r["tokens"]) for r in records),
            "nodes": normalized["nodes"], "document_id": normalized["document_id"],
            "diagnostics": normalized["diagnostics"], "limitations": normalized["limitations"]}


def generate_candidates(index: dict, query_tokens: list[str], *, candidate_ids=None) -> list[dict]:
    """1단계: 기존 BM25-lite. 선택적 로컬 색인의 후보 ID로 범위를 제한한다."""
    records = index.get("records", [])
    n = len(records) or 1
    document_frequency = Counter(token for record in records for token in set(record["tokens"]))
    average_length = sum(len(r["tokens"]) for r in records) / n
    ranked = []
    allowed = None if candidate_ids is None else set(candidate_ids)
    for record in records:
        if allowed is not None and record["id"] not in allowed:
            continue
        counts = Counter(record["tokens"])
        score = 0.0
        for token in query_tokens:
            if not counts[token]:
                continue
            idf = math.log(1 + (n - document_frequency[token] + 0.5) /
                           (document_frequency[token] + 0.5))
            length_factor = 1 + 0.75 * (len(record["tokens"]) / (average_length or 1) - 1)
            score += idf * counts[token] / max(length_factor, 0.25)
        if score <= 0:
            continue
        record = dict(record)
        record["candidate_score"] = score
        ranked.append(record)
    return ranked


def expand_context(index: dict, candidates: list[dict], query_tokens: list[str]) -> list[dict]:
    """2단계: 저장된 그래프만 읽어 상위 문맥·하위 참조·일치어를 복원한다."""
    by_id = {node["id"]: node for node in index.get("nodes", [])}
    # version 1 저장 인덱스도 원본을 다시 열지 않고 읽는다.
    if not by_id:
        by_id = {record["id"]: record for record in index.get("records", [])}

    def summary(node):
        return {key: copy.deepcopy(node.get(key)) for key in
                ("id", "kind", "role", "text", "parent_id", "position")}

    expanded = []
    for candidate in candidates:
        result = copy.deepcopy(candidate)
        node = by_id.get(result["id"], result)
        ancestors, seen = [], {result["id"]}
        parent_id = node.get("parent_id")
        while parent_id is not None:
            if parent_id in seen or parent_id not in by_id:
                raise ValueError("검색 인덱스의 상위 노드가 잘못되었습니다")
            seen.add(parent_id)
            parent = by_id[parent_id]
            ancestors.append(summary(parent))
            parent_id = parent.get("parent_id")
        ancestors.reverse()
        result["parent_nodes"] = ancestors
        result["child_nodes"] = [summary(by_id[x]) for x in node.get("child_ids", []) if x in by_id]
        result["ancestor_ids"] = [x["id"] for x in ancestors]
        result["matched_terms"] = list(dict.fromkeys(t for t in query_tokens if t in result["tokens"]))
        body = set(_tokens(result["text"]))
        result["matched_body_terms"] = [t for t in result["matched_terms"] if t in body]
        result["matched_context_terms"] = [t for t in result["matched_terms"] if t not in body]
        path = result.get("path", [])
        name = index.get("document_name", "")
        result["document_name"] = name
        result["full_path"] = ([name] if name and (not path or path[0] != name) else []) + path
        result["full_breadcrumb"] = " > ".join(x for x in result["full_path"] if x)
        result["page"] = result.get("position", {}).get("page")
        result["pipeline"] = ["candidate_generation", "parent_context_expansion", "reranking"]
        expanded.append(result)
    return expanded


def rerank(candidates: list[dict]) -> list[dict]:
    """3단계: 기존 제목 가중치를 유지한다. 문맥으로 새 후보를 만들지 않는다."""
    for record in candidates:
        score = record["candidate_score"]
        if record["role"] in {"title", "chapter", "section", "detail"}:
            score *= 1.15
        record["score"] = round(score, 6)
        record["ranking_reason"] = "legacy_heading_weight" if score != record["candidate_score"] else "legacy_bm25_lite"
    def key(item):
        node_id = item.get("id", 0)
        # 정수 ID의 기존 순서는 유지하고 문자열·혼합 ID도 결정적으로 정렬한다.
        stable_id = (0, node_id) if isinstance(node_id, (int, float)) else (1, str(node_id))
        level = item.get("rank", item.get("level", 99))
        if not isinstance(level, (int, float)):
            level = 99
        return (-item["score"], level, stable_id)
    candidates.sort(key=key)
    return candidates


def search(index: dict, query: str, *, limit: int = 20, candidate_ids=None) -> list[dict]:
    """후보 생성→상위 문맥 확장→재정렬. 파일·네트워크 접근은 하지 않는다."""
    query_tokens = _tokens(query)
    if not query_tokens or limit <= 0:
        return []
    ranked = rerank(expand_context(index, generate_candidates(index, query_tokens,
                                                             candidate_ids=candidate_ids), query_tokens))
    return ranked[:max(0, limit)]


def build_index_from_content_db(database, document_path: str) -> dict:
    """Python/Rust 공용 content_paragraph_fts를 읽기 전용으로 정규화한다.

    등록·변경 시 호출하는 어댑터다. 원본 파일·DB 스키마를 수정하지 않는다.
    후보 검색은 생성된 인덱스의 search에서 처리한다.
    """
    import sqlite3
    from contextlib import closing
    from pathlib import Path
    uri = Path(database).resolve().as_uri() + "?mode=ro"
    with closing(sqlite3.connect(uri, uri=True)) as connection:
        connection.row_factory = sqlite3.Row
        connection.execute("BEGIN")  # 문단과 메타데이터를 같은 스냅숏으로 읽는다.
        rows = connection.execute(
            "SELECT paragraph_index, level, body, location_label, section "
            "FROM content_paragraph_fts WHERE path=? ORDER BY CAST(paragraph_index AS INTEGER)",
            (document_path,)).fetchall()
        meta = connection.execute("SELECT name FROM content_meta WHERE path=?", (document_path,)).fetchone()
    result = build_index({"paragraphs": [dict(row) for row in rows]},
                         document_name=meta["name"] if meta else "")
    result["source"] = {"kind": "content_paragraph_fts", "document_path": document_path}
    return result
