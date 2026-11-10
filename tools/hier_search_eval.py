"""같은 추출 문서 집합의 본문 전문검색과 구조검색을 오프라인 비교한다.

JSON 추출 결과만 읽는다. 원본 문서 파싱/OCR/COM·외부 API는 실행하지 않는다.
메모리는 tracemalloc의 Python 할당 최고값이며 Rust/프로세스 RSS가 아니다.
"""
from __future__ import annotations

import argparse
from collections import Counter
import copy
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import sys
import time
import tracemalloc

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from docfit_core.hierarchy_search import build_index, search

DEFAULT_CORPUS = ROOT / "tests" / "fixtures" / "hier_search" / "regressions.json"
TOKEN = re.compile(r"[0-9A-Za-z가-힣一-龥]+")
UNSUPPORTED = ["원본 문서 직접 입력·OCR·파일 변환 미지원; 추출 JSON만 평가",
               "Rust/native 메모리 및 프로세스 RSS는 tracemalloc 측정 범위 밖"]
UNVERIFIED = ["실문서 파싱 품질·대규모 코퍼스 성능·한/글 COM은 이 평가로 검증되지 않음"]


def tokens(value):
    return [token.casefold() for token in TOKEN.findall(value or "")]


def metrics(results, relevant, expected_context, k):
    """문맥 정확도 = 반환된 정답 중 필수 상위 문맥을 모두 포함한 비율.

    정답을 못 찾았으면 문맥 정확도는 0이다. 예상 문맥이 비어 있으면
    문맥 평가 대상이 아니므로 None으로 반환한다. 문맥은 breadcrumb나
    확장된 상위 문맥 텍스트에서 확인한다.
    """
    selected = results[:k]
    relevant = set(relevant)
    if not relevant:
        raise ValueError("각 질의에는 하나 이상의 정답 노드가 필요합니다")
    hits = [result for result in selected if result["id"] in relevant]
    reciprocal = next((1.0 / rank for rank, result in enumerate(selected, 1)
                       if result["id"] in relevant), 0.0)
    context_hits = 0
    for result in hits:
        context = result.get("breadcrumb", "")
        # 확장 결과의 구체적 자료형에 의존하지 않고 텍스트를 검사한다.
        if result.get("ancestors"):
            context += " " + json.dumps(result["ancestors"], ensure_ascii=False)
        if all(part in context for part in expected_context):
            context_hits += 1
    return {"recall_at_k": len({r["id"] for r in hits}) / len(relevant),
            "mrr_at_k": reciprocal,
            "context_accuracy": (context_hits / len(hits) if hits else 0.0)
                                if expected_context else None}


def _plain_index(structure):
    return {"records": [{"id": node["id"], "text": node.get("text", ""),
                          "tokens": tokens(node.get("text", "")),
                          "breadcrumb": node.get("text", "")}
                         for node in structure["nodes"]]}


def plain_search(index, query, *, limit=20):
    """기존 BM25-lite의 빈도·길이식을 본문 토큰에만 적용하는 대조군."""
    return _rank(index, query, limit=limit)


def _rank(index, query, *, limit, legacy=False):
    records = index["records"]
    query_tokens = tokens(query)
    n = len(records) or 1
    df = Counter(token for r in records for token in set(r["tokens"]))
    average = sum(len(r["tokens"]) for r in records) / n or 1
    ranked = []
    for record in records:
        counts = Counter(record["tokens"])
        length = max(1 + .75 * (len(record["tokens"]) / average - 1), .25)
        score = sum(math.log(1 + (n - df[token] + .5) / (df[token] + .5))
                    * counts[token] / length for token in query_tokens if counts[token])
        if score:
            if legacy and record.get("role") in {"title", "chapter", "section", "detail"}:
                score *= 1.15
            ranked.append({**record, "score": round(score, 6)})
    if legacy:
        ranked.sort(key=lambda r: (-r["score"], r.get("level", 99), r["id"]))
    else:
        ranked.sort(key=lambda r: (-r["score"], str(r["id"])))
    return ranked[:max(limit, 0)]


def _legacy_index(structure):
    """2b7ec241의 원래 경로 토큰/역할 가중치/동점 정렬을 고정한 회귀 대조군.

    같은 입력 노드를 사용하되 신규 정규화나 검색 필드를 거치지 않는다.
    원본 함수와 출력이 같은지는 별도 golden 시험으로 고정한다.
    """
    by_id = {node["id"]: node for node in structure["nodes"]}
    records = []
    for node in structure["nodes"]:
        chain, visited = [], {node["id"]}
        parent = by_id.get(node.get("parent_id"))
        while parent is not None:
            if parent["id"] in visited:
                raise ValueError("부모 그래프 순환")
            visited.add(parent["id"])
            chain.append(parent)
            parent = by_id.get(parent.get("parent_id"))
        path = [*(item.get("text", "") for item in reversed(chain)), node.get("text", "")]
        records.append({**node, "tokens": tokens(" ".join(path)),
                        "breadcrumb": " > ".join(item for item in path if item)})
    return {"records": records}


def _measure_build(builder, structure):
    # 호출자 측 추적 세션을 중단하거나 재설정하지 않는다.
    already_tracing = tracemalloc.is_tracing()
    if not already_tracing:
        tracemalloc.start()
    before = tracemalloc.get_traced_memory()[0]
    start = time.perf_counter()
    try:
        result = builder(structure)
        elapsed = (time.perf_counter() - start) * 1000
        current, peak = tracemalloc.get_traced_memory()
        memory = {"retained_python_bytes": max(current - before, 0),
                  "peak_python_bytes": max(peak - before, 0),
                  "scope": "tracemalloc Python allocations; excludes native/Rust/RSS",
                  "exclusive_measurement": not already_tracing}
    finally:
        if not already_tracing:
            tracemalloc.stop()
    return result, elapsed, memory


def _combined_structure(corpus):
    nodes = []
    for document in corpus["documents"]:
        prefix = document["id"] + "/"
        root_id = prefix + "document"
        nodes.append({"id": root_id, "text": document["name"],
                      "role": "title", "level": 0, "parent_id": None})
        for source in document["nodes"]:
            node = copy.deepcopy(source)
            node["id"] = prefix + str(source["id"])
            parent = source.get("parent_id")
            node["parent_id"] = prefix + str(parent) if parent is not None else root_id
            nodes.append(node)
    ids = [n["id"] for n in nodes]
    if len(ids) != len(set(ids)):
        raise ValueError("중복 문서 또는 노드 ID")
    known = set(ids)
    for query in corpus["queries"]:
        if not query.get("relevant") or not set(query["relevant"]) <= known:
            raise ValueError("질의 정답 ID가 문서 집합에 없습니다")
    return {"nodes": nodes}


def evaluate(corpus, *, k=5, repeats=5):
    if k < 1 or repeats < 1:
        raise ValueError("k와 repeats는 1 이상이어야 합니다")
    if not corpus.get("documents") or not corpus.get("queries"):
        raise ValueError("documents와 queries가 필요합니다")
    structure = _combined_structure(corpus)
    plain, plain_ms, plain_memory = _measure_build(_plain_index, structure)
    hierarchy, hier_ms, hier_memory = _measure_build(build_index, structure)
    legacy = _legacy_index(structure)
    rows, latencies = [], {"plain": [], "hierarchy": []}
    for query in corpus["queries"]:
        row = {"id": query["id"], "group": query["group"],
               "query": query["query"], "relevant": query["relevant"],
               "expected_context": query.get("expected_context", [])}
        for name, index, operation in (("plain", plain, plain_search),
                                       ("hierarchy", hierarchy, search)):
            for _ in range(repeats):
                start = time.perf_counter()
                results = operation(index, query["query"], limit=k)
                latencies[name].append((time.perf_counter() - start) * 1000)
            row[name] = {**metrics(results, query["relevant"],
                                   query.get("expected_context", []), k),
                         "ids": [r["id"] for r in results],
                         "scores": [r["score"] for r in results]}
        row["ranking_changed_from_plain"] = row["plain"]["ids"] != row["hierarchy"]["ids"]
        row["change_reason"] = ("본문 토큰만 점수화한 대조군과 달리 상위 문맥 및 역할 가중치가 반영됨. "
                                "원본 2b7ec241 대비 순위 변화는 별도 비교."
                                if row["ranking_changed_from_plain"] else "대조군과 동일 순위")
        legacy_results = _rank(legacy, query["query"], limit=k, legacy=True)
        row["legacy_top_k"] = [result["id"] for result in legacy_results]
        row["legacy_scores"] = [result["score"] for result in legacy_results]
        row["ranking_changed_from_2b7ec241"] = row["legacy_top_k"] != row["hierarchy"]["ids"]
        row["legacy_change_reason"] = ("신규 정규화/후보 생성/재정렬로 원래 경로 BM25-lite와 순위가 달라짐; "
                                        "해당 ids를 비교해 개별 원인을 검토해야 함"
                                        if row["ranking_changed_from_2b7ec241"] else "원래 입력 노드 순위 보존")
        rows.append(row)
    summary = {}
    for name, elapsed, memory in (("plain", plain_ms, plain_memory),
                                   ("hierarchy", hier_ms, hier_memory)):
        sample = sorted(latencies[name])
        summary[name] = {metric: statistics.mean(row[name][metric] for row in rows
                                                if row[name][metric] is not None)
                         if any(row[name][metric] is not None for row in rows) else None
                         for metric in ("recall_at_k", "mrr_at_k", "context_accuracy")}
        summary[name].update({"index_time_ms": elapsed, "memory": memory,
                              "search_latency_ms": {"mean": statistics.mean(sample),
                                  "p50": statistics.median(sample),
                                  "p95": sample[max(math.ceil(len(sample) * .95) - 1, 0)],
                                  "samples": len(sample)}})
    return {"schema_version": 1, "k": k, "repeats": repeats,
            "document_count": len(corpus["documents"]), "query_count": len(rows),
            "corpus_sha256": hashlib.sha256(json.dumps(corpus, ensure_ascii=False,
                sort_keys=True).encode("utf-8")).hexdigest(),
            "scope": "합성 추출 결과 전체 코퍼스 검색; 실제 원본 파싱 성능 아님",
            "definitions": {"recall_at_k": "상위 k개 정답 고유 ID 수 / 전체 정답 ID 수",
                            "mrr_at_k": "상위 k 내 첫 정답 순위의 역수, 미발견 0",
                            "context_accuracy": "반환 정답 중 필수 상위 문맥 전부 포함한 비율; 정답 미발견 0",
                            "summary": "질의별 macro 평균; 문맥 정답이 없는 질의는 문맥 평균 제외"},
            "unsupported": list(dict.fromkeys([*UNSUPPORTED, *corpus.get("unsupported", [])])),
            "unverified": list(dict.fromkeys([*UNVERIFIED, *corpus.get("unverified", [])])),
            "documents": [{key: doc.get(key) for key in ("id", "name", "format", "fixture_kind")}
                          for doc in corpus["documents"]],
            "summary": summary, "queries": rows}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--k", type=int, default=5)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args(argv)
    if (args.output.resolve() == args.corpus.resolve()
            or args.output.exists() and args.corpus.exists() and args.output.samefile(args.corpus)):
        parser.error("입력 코퍼스 파일을 결과 파일로 덮어쓸 수 없습니다")
    try:
        corpus = json.loads(args.corpus.read_text(encoding="utf-8"))
        report = evaluate(corpus, k=args.k, repeats=args.repeats)
    except (ValueError, KeyError, TypeError) as exc:
        parser.error(str(exc))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "documents": report["document_count"],
                      "queries": report["query_count"], "summary": report["summary"]}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
