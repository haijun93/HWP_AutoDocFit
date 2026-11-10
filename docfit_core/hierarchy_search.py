"""문서 계층을 보존하는 오프라인 검색 인덱스.

벡터 DB나 외부 서비스 없이 ``document_structure.analyze`` 결과를 검색용
레코드로 바꾼다. 키워드 점수와 상위 제목 경로 점수를 함께 사용하므로,
본문에만 검색어가 있어도 결과에는 문서의 장·절·항 경로가 포함된다.
"""
from __future__ import annotations

import math
import re
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


def search(index: dict, query: str, *, limit: int = 20) -> list[dict]:
    """BM25-lite 점수에 계층 경로·제목 역할 가중치를 더해 검색한다."""
    query_tokens = _tokens(query)
    if not query_tokens:
        return []
    records = index.get("records", [])
    n = len(records) or 1
    document_frequency = Counter(token for record in records for token in set(record["tokens"]))
    average_length = sum(len(r["tokens"]) for r in records) / n
    ranked = []
    for record in records:
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
        if record["role"] in {"title", "chapter", "section", "detail"}:
            score *= 1.15
        record = dict(record)
        record["score"] = round(score, 6)
        ranked.append(record)
    ranked.sort(key=lambda item: (-item["score"], item.get("level", 99), item.get("id", 0)))
    return ranked[:max(0, limit)]
