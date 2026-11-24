#!/usr/bin/env python3
"""Compare SQLite and klib top-20 results on the SRCH2 synthetic corpus."""
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
sys.path.insert(0, str(ROOT / "scripts"))
from search_bench import make_corpus


QUERIES = {
    "keyword": ("행정", False),
    "and": ("행정 정책", False),
    "phrase": ('"행정 절차 지원"', False),
    "hierarchy": ("행정 정책 문서 안전 지원", True),
}


def collect_matches(backend, query, *, hierarchy, corpus, count):
    # 100개 자료함으로 범위를 나눠 각 SQLite/Rust API의 500행 상한보다 작게 전체를 수집한다.
    rows = []
    for start in range(0, count, 100):
        root = corpus / f"part_{start // 100:03d}"
        rows.extend(backend.search(query, roots=[str(root)], hierarchy=hierarchy, limit=500))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=20261124)
    parser.add_argument("--klib-exe")
    parser.add_argument("--output", type=Path, help="save the full comparison JSON")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="srch4-compare-") as temp:
        root = Path(temp)
        corpus = root / "corpus"
        corpus.mkdir()
        # make_corpus owns the same Random(seed) sequence as the SRCH2 benchmark.
        paths = make_corpus(corpus, args.count, args.seed, partition_size=100)
        sqlite_index = ContentSearchIndex(root / "sqlite.sqlite")
        sqlite = SqliteBackend(sqlite_index)
        rust = RustBackend(executable=args.klib_exe, index_dir=root / "tantivy")
        errors = []
        sqlite.index_files(paths)
        try:
            batch_results = rust.index_files(paths, batch_size=250)
            batch_failures = [row for row in batch_results if row.get("error")]
        except Exception as error:
            errors.append({"reason": str(error)})
            batch_failures = []
        comparisons = {}
        if not errors:
            for name, (query, hierarchy) in QUERIES.items():
                left = collect_matches(sqlite, query, hierarchy=hierarchy, corpus=corpus, count=args.count)
                right = collect_matches(rust, query, hierarchy=hierarchy, corpus=corpus, count=args.count)
                left_paths = [Path(row.get("path", "")).name for row in left]
                right_paths = [Path(row.get("path", "")).name for row in right]
                left_set, right_set = set(left_paths), set(right_paths)
                left_top = {Path(row.get("path", "")).name for row in sqlite.search(query, hierarchy=hierarchy, limit=20)}
                right_top = {Path(row.get("path", "")).name for row in rust.search(query, hierarchy=hierarchy, limit=20)}
                comparisons[name] = {
                    "query": query,
                    "sqlite_match_count": len(left_set),
                    "rust_match_count": len(right_set),
                    "result_sets_truncated_by_api": False,
                    "result_set_match_rate": len(left_set & right_set) / max(1, len(left_set | right_set)),
                    "top20_overlap": len(left_top & right_top),
                    "top20_match_rate": len(left_top & right_top) / max(1, min(20, len(left_top), len(right_top))),
                    "missing_in_rust": sorted(left_set - right_set),
                    "extra_in_rust": sorted(right_set - left_set),
                }
        result = {"documents": args.count, "seed": args.seed,
                  "status": "complete" if not errors else "rust_error",
                  "index_errors": errors, "batch_error_count": len(batch_failures),
                  "rust_status": rust.status(), "comparisons": comparisons}
        encoded = json.dumps(result, ensure_ascii=False, indent=2)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(encoded + "\n", encoding="utf-8")
        print(encoded)
        rust.close()
        sqlite_index.close()
    return 0 if not errors else 2


if __name__ == "__main__":
    raise SystemExit(main())
