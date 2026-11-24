#!/usr/bin/env python3
"""SRCH5 synthetic ranking evaluation for SQLite and Rust backends."""
from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine.search_backend import RustBackend, SqliteBackend

QUERIES = {"행정 절차": ("행정 절차",), "예산 편성": ("예산 편성",), "안전 점검": ("안전 점검",)}


def build_corpus(root: Path) -> dict[str, list[str]]:
    gold: dict[str, list[str]] = {}
    for query_index, (query, (terms,)) in enumerate(QUERIES.items()):
        gold[query] = []
        # 정답: 검색어가 문서 제목에 있고 본문에서도 반복되는 문서.
        for n in range(4):
            path = root / f"gold_{query_index}_{n}.md"
            body = (f"# {terms} 기준과 실행 계획\n\n" + (f"{terms} 추진 절차와 지원 기준을 정리한다. " * (4 - n)))
            path.write_text(body, encoding="utf-8")
            gold[query].append(path.name)
        # 본문에만 검색어가 한 번 나타나는 낮은 우선순위 자료.
        for n in range(36):
            path = root / f"body_{query_index}_{n}.md"
            path.write_text(f"# 일반 업무 기록 {n}\n\n{terms} 관련 현황을 확인한다.\n", encoding="utf-8")
    return gold


def metrics(ranked: list[str], relevant) -> dict[str, float | int]:
    relevant_set = set(relevant)
    top10 = ranked[:10]
    first = next((i for i, name in enumerate(ranked, 1) if name in relevant_set), None)
    result = {"recall_at_10": round(len(set(top10) & relevant_set) / max(1, len(relevant_set)), 4),
            "mrr": round(1 / first, 4) if first else 0.0,
            "relevant_in_top_10": len(set(top10) & relevant_set), "relevant_total": len(relevant_set),
            "first_relevant_rank": first}
    if isinstance(relevant, (list, tuple)):
        observed = [name for name in ranked if name in relevant_set]
        positions = {name: i for i, name in enumerate(observed)}
        pairs = [(a, b) for i, a in enumerate(relevant) for b in relevant[i + 1:]]
        result["gold_order_pairwise_accuracy"] = round(
            sum(positions.get(a, 10**9) < positions.get(b, 10**9) for a, b in pairs) / max(1, len(pairs)), 4)
    return result


def evaluate(backend, queries, gold, root):
    result = {}
    for query in queries:
        rows = backend.search(query, roots=[str(root)], hierarchy=True, limit=100)
        ranking = [Path(row["path"]).name for row in rows]
        result[query] = metrics(ranking, gold[query])
        result[query]["top_10"] = ranking[:10]
        result[query]["top_10_breadcrumbs"] = [row.get("breadcrumb_text", "") for row in rows[:10]]
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--klib-exe")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="srch5-quality-") as temp:
        root = Path(temp) / "corpus"
        root.mkdir()
        gold = build_corpus(root)
        paths = list(root.glob("*.md"))
        sqlite_index = ContentSearchIndex(Path(temp) / "sqlite.sqlite")
        sqlite = SqliteBackend(sqlite_index)
        executable = Path(args.klib_exe).resolve() if args.klib_exe else None
        rust = RustBackend(executable=executable, index_dir=Path(temp) / "tantivy")
        try:
            sqlite.index_files(paths)
            rust_errors = [r for r in rust.index_files(paths) if r.get("error")]
            result = {"documents": len(paths), "queries": len(QUERIES), "gold_per_query": 4,
                      "status": "complete" if not rust_errors else "rust_index_errors",
                      "sqlite": evaluate(sqlite, QUERIES, gold, root),
                      "rust": evaluate(rust, QUERIES, gold, root), "rust_index_errors": rust_errors}
            print(json.dumps(result, ensure_ascii=False, indent=2))
        finally:
            rust.close()
            sqlite_index.close()
        return 0 if not rust_errors else 2


if __name__ == "__main__":
    raise SystemExit(main())
