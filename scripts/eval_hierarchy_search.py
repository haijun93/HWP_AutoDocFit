#!/usr/bin/env python3
"""고정 합성 문서로 위계 검색과 일반 본문 검색을 비교한다."""
from __future__ import annotations

import argparse
import json
import os
import random
import statistics
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from docfit_core.timemachine.content_search import ContentSearchIndex

SEED = 20261112
DOCUMENT_COUNT = 180
QUERIES = ("예산", "안전", "인력", "조달", "보안", "품질", "교육", "기록")
REGRESSION_GROUPS = ("목차 없음", "번호 체계 오류", "표 안 제목", "아주 긴 제목")


def canonical_path(path: str | os.PathLike[str]) -> str:
    return os.path.normcase(os.path.abspath(path)).replace("\\", "/")


def _mean(values):
    return sum(values) / len(values) if values else 0.0


def score_ranked_results(ranked_results, relevant_by_query, expected_breadcrumbs=None):
    """순위별 검색 결과로 Recall@5/10, MRR, 문맥 정확도를 계산한다.

    expected_breadcrumbs는 query별 path별 정확한 breadcrumb 문자열 목록이다.
    문맥 정확도는 검색된 정답 문서 중 breadcrumb 전체가 일치하는 비율이다.
    """
    expected_breadcrumbs = expected_breadcrumbs or {}
    per_query = {}
    for query, rows in ranked_results.items():
        relevant = {canonical_path(p) for p in relevant_by_query.get(query, ())}
        ranked_paths = [canonical_path(row["path"]) for row in rows]
        first_rank = next((rank for rank, path in enumerate(ranked_paths, 1) if path in relevant), None)
        found_at_5 = set(ranked_paths[:5]) & relevant
        found_at_10 = set(ranked_paths[:10]) & relevant
        context_expected = expected_breadcrumbs.get(query, {})
        relevant_rows = [row for row in rows if canonical_path(row["path"]) in relevant]
        context_matches = sum(
            list(row.get("breadcrumb", [])) == list(context_expected.get(canonical_path(row["path"]), ()))
            for row in relevant_rows
            if canonical_path(row["path"]) in context_expected
        )
        context_count = sum(canonical_path(row["path"]) in context_expected for row in relevant_rows)
        per_query[query] = {
            "recall_at_5": len(found_at_5) / len(relevant) if relevant else 0.0,
            "recall_at_10": len(found_at_10) / len(relevant) if relevant else 0.0,
            "mrr": 1.0 / first_rank if first_rank else 0.0,
            "context_accuracy": context_matches / context_count if context_count else None,
            "relevant_count": len(relevant),
            "context_evaluated": context_count,
        }
    for metric in ("recall_at_5", "recall_at_10", "mrr"):
        per_query["mean_" + metric] = _mean([item[metric] for item in per_query.values() if isinstance(item, dict)])
    context_values = [item["context_accuracy"] for item in per_query.values()
                      if isinstance(item, dict) and item["context_accuracy"] is not None]
    per_query["mean_context_accuracy"] = _mean(context_values) if context_values else None
    return per_query


def generate_corpus(root: Path, seed: int = SEED, document_count: int = DOCUMENT_COUNT):
    """고정 시드로 구조 문서와 정답·회귀군 메타데이터를 만든다."""
    if not 100 <= document_count <= 300:
        raise ValueError("문서 수는 100~300이어야 합니다.")
    rng = random.Random(seed)
    rng.shuffle(query_terms := list(QUERIES))
    documents = {}
    groups = {group: [] for group in REGRESSION_GROUPS}
    group_for_number = {number: REGRESSION_GROUPS[(number // 4) % len(REGRESSION_GROUPS)]
                        for number in range(document_count) if number % 4 == 0}
    for number in range(document_count):
        group = group_for_number.get(number)
        term = query_terms[number % len(query_terms)]
        title_term = number % 3 != 0
        doc_id = f"문서_{number + 1:03d}"
        title = f"{term} 운영 지침 {number + 1:03d}" if title_term else f"공통 운영 지침 {number + 1:03d}"
        chapter = f"제1장 {term} 관리"
        section = f"1.1 {term} 계획"
        topic = f"가. {term} 세부 기준"
        body = f"본문에서는 {term} 관련 세부 절차와 담당 기준을 설명한다. " + "자료 검토와 실행 결과를 기록한다. " * 3
        lines = [title, chapter, section, topic, body]
        expected = [chapter, section, topic]
        if group == "목차 없음":
            lines = [title, chapter, section, topic, body]
            expected = [chapter, section, topic]
        elif group == "번호 체계 오류":
            lines[2] = f"제3장 {term} 관리"
            lines[3] = f"7.4.9 {term} 계획"
            lines[4] = f"나) {term} 세부 기준"
            expected = [lines[2], lines[3], lines[4]]
        elif group == "표 안 제목":
            lines = [title, chapter, f"| 구분 | {term} 제목 |", "| --- | --- |", "| 설명 | 표 안에서 다루는 내용 |", section, body]
        elif group == "아주 긴 제목":
            title = f"{term} " + "매우 긴 운영 기준과 세부 적용 절차 " * 12
            lines[0] = title
        extension = ".md" if number % 2 == 0 else ".txt"
        path = root / f"{doc_id}{extension}"
        text = "\n".join(lines)
        path.write_text(text, encoding="utf-8")
        documents[canonical_path(path)] = {
            "query": term, "title_match": title_term, "group": group,
            "breadcrumb": expected, "path": str(path),
        }
        if group:
            groups[group].append(canonical_path(path))
    relevant_by_query = {
        term: [path for path, item in documents.items() if item["query"] == term and item["title_match"]]
        for term in query_terms
    }
    expected_breadcrumbs = {
        term: {path: documents[path]["breadcrumb"] for path in relevant_by_query[term]}
        for term in query_terms
    }
    return documents, relevant_by_query, expected_breadcrumbs, groups


def _measure_method(index, method_name, queries, relevant_by_query, expected_breadcrumbs, groups, documents):
    method = getattr(index, method_name)
    timings = []
    all_results = {}
    for query in queries:
        started = time.perf_counter()
        all_results[query] = method(query, limit=500)
        timings.append((time.perf_counter() - started) * 1000)
    overall = score_ranked_results(all_results, relevant_by_query, expected_breadcrumbs)
    group_scores = {}
    for group, paths in groups.items():
        subset = {query: [row for row in rows if canonical_path(row["path"]) in paths]
                  for query, rows in all_results.items()}
        qrels = {query: [path for path in relevant_by_query[query] if path in paths] for query in queries}
        crumbs = {query: {path: expected_breadcrumbs[query][path] for path in qrels[query]}
                  for query in queries}
        group_result = score_ranked_results(subset, qrels, crumbs)
        # 해당 회귀군의 관련 문서 순위는 전체 결과에서 보존한 채 분모를 군 정답 수로 둔다.
        group_result["document_count"] = len(paths)
        group_scores[group] = group_result
    return {
        "metrics": overall, "regression_groups": group_scores,
        "latency_ms": {"median": statistics.median(timings) if timings else 0.0,
                       "max": max(timings, default=0.0), "per_query": dict(zip(queries, timings))},
    }


def run_evaluation(output_dir: Path, seed: int = SEED, document_count: int = DOCUMENT_COUNT):
    output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="hs2-hsb-corpus-") as corpus_temp, \
            tempfile.TemporaryDirectory(prefix="hs2-hsb-index-") as index_temp:
        corpus_root = Path(corpus_temp)
        documents, qrels, expected, groups = generate_corpus(corpus_root, seed, document_count)
        index_path = Path(index_temp) / "content.sqlite"
        index = ContentSearchIndex(index_path)
        start = time.perf_counter()
        index_results = [index.index_file(item["path"]) for item in documents.values()]
        index_seconds = time.perf_counter() - start
        indexed = sum(bool(item["indexed"]) for item in index_results)
        errors = [item["error"] for item in index_results if item["error"]]
        if indexed != document_count or errors:
            index.close()
            raise RuntimeError(f"색인 오류: {indexed}/{document_count}, 오류 {errors[:3]}")
        hierarchy = _measure_method(index, "search", QUERIES, qrels, expected, groups, documents)
        keyword = _measure_method(index, "keyword_search", QUERIES, qrels, expected, groups, documents)
        index.db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        index_size = index_path.stat().st_size
        index.close()
        result = {
            "seed": seed, "document_count": document_count, "indexed_count": indexed,
            "query_count": len(QUERIES), "queries": list(QUERIES),
            "corpus_format_counts": {"md": sum(Path(d["path"]).suffix == ".md" for d in documents.values()),
                                     "txt": sum(Path(d["path"]).suffix == ".txt" for d in documents.values())},
            "indexing_seconds": index_seconds, "index_file_bytes": index_size,
            "index_file_size_mib": index_size / (1024 * 1024),
            "hierarchy_search": hierarchy, "keyword_search": keyword,
            "measurement_note": "각 방법과 질의는 한 번씩 측정. 검색 캐시 영향이 있으므로 방법별 실행 순서를 고정함.",
        }
    return result


def markdown_report(result):
    lines = ["# HS2-B 위계검색 품질 기준선 (2026-11-12)", "",
             "## 측정 조건", "",
             f"- 고정 시드: `{result['seed']}`; 문서: {result['document_count']}개 (MD {result['corpus_format_counts']['md']}, TXT {result['corpus_format_counts']['txt']})",
             f"- 질의: {result['query_count']}개; 색인 완료: {result['indexed_count']}개; 색인 시간: {result['indexing_seconds']:.3f}초",
             f"- SQLite 색인 파일: {result['index_file_bytes']:,}바이트 ({result['index_file_size_mib']:.3f} MiB)",
             "- Recall은 정답 제목 문서 중 상위 k개에 든 비율의 질의 평균이며, MRR은 첫 정답 문서의 역순위 평균입니다.",
             "- 문맥 정확도는 검색된 정답 문서 결과의 `breadcrumb` 전체가 생성 시 기대 경로와 일치한 비율입니다.", "",
             "## 전체 점수", "",
             "| 검색 방식 | Recall@5 | Recall@10 | MRR | 문맥 정확도 | 지연 중앙값 (ms) | 지연 최댓값 (ms) |",
             "|---|---:|---:|---:|---:|---:|---:|"]
    for label, key in (("위계 검색", "hierarchy_search"), ("일반 본문 검색", "keyword_search")):
        item = result[key]
        metrics = item["metrics"]
        def pct(value): return "N/A" if value is None else f"{value:.3f}"
        lines.append(f"| {label} | {metrics['mean_recall_at_5']:.3f} | {metrics['mean_recall_at_10']:.3f} | {metrics['mean_mrr']:.3f} | {pct(metrics['mean_context_accuracy'])} | {item['latency_ms']['median']:.3f} | {item['latency_ms']['max']:.3f} |")
    lines.extend(["", "## 회귀군", "", "검색 결과를 회귀군 문서로 제한하고, 해당 군의 정답 문서 수를 분모로 계산합니다. 군별 문서는 각각 9~12개입니다. 이번 저장 기준선에서는 군별 순위 원자료가 없어 점수를 산출하지 않았습니다.", "",
                  "| 문서군 | 문서 수 | 검색 방식 | Recall@5 | Recall@10 | MRR | 문맥 정확도 |", "|---|---:|---|---:|---:|---:|---:|"])
    for group in REGRESSION_GROUPS:
        for label, key in (("위계", "hierarchy_search"), ("일반", "keyword_search")):
            metrics = result[key]["regression_groups"][group]
            def show(value): return "N/A" if value is None else f"{value:.3f}"
            lines.append(f"| {group} | {metrics['document_count']} | {label} | {show(metrics['mean_recall_at_5'])} | {show(metrics['mean_recall_at_10'])} | {show(metrics['mean_mrr'])} | {pct(metrics['mean_context_accuracy'])} |")
    lines.extend(["", "## 질의별 결과", "", "| 질의 | 방식 | Recall@5 | Recall@10 | MRR | 문맥 정확도 | 지연 (ms) |", "|---|---|---:|---:|---:|---:|---:|"])
    for query in QUERIES:
        for label, key in (("위계", "hierarchy_search"), ("일반", "keyword_search")):
            item = result[key]
            metrics = item["metrics"][query]
            lines.append(f"| {query} | {label} | {metrics['recall_at_5']:.3f} | {metrics['recall_at_10']:.3f} | {metrics['mrr']:.3f} | {pct(metrics['context_accuracy'])} | {item['latency_ms']['per_query'][query]:.3f} |")
    lines.extend(["", "## JSON 원자료", "", "전체 세부 점수와 질의별 지연은 함께 저장된 JSON 파일을 참조합니다.", ""])
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path("docs/benchmarks"))
    parser.add_argument("--seed", type=int, default=SEED)
    parser.add_argument("--documents", type=int, default=DOCUMENT_COUNT)
    args = parser.parse_args(argv)
    result = run_evaluation(args.out_dir, args.seed, args.documents)
    json_path = args.out_dir / "위계검색_품질_기준선_2026-11-12.json"
    markdown_path = args.out_dir / "위계검색_품질_기준선_2026-11-12.md"
    json_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    markdown_path.write_text(markdown_report(result), encoding="utf-8")
    print(json.dumps({"json": str(json_path), "markdown": str(markdown_path),
                      "indexing_seconds": result["indexing_seconds"],
                      "hierarchy": result["hierarchy_search"]["metrics"],
                      "keyword": result["keyword_search"]["metrics"]}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
