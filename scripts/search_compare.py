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
    "keyword": ("정책", False),
    "and": ("정책 문서", False),
    "phrase": ('"정책 안전 지침"', False),
    "hierarchy": ("정책 안전 지침", True),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=20261124)
    parser.add_argument("--klib-exe")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="srch3-compare-") as temp:
        root = Path(temp)
        corpus = root / "corpus"
        corpus.mkdir()
        # make_corpus owns the same Random(seed) sequence as the SRCH2 benchmark.
        paths = make_corpus(corpus, args.count, args.seed)
        sqlite_index = ContentSearchIndex(root / "sqlite.sqlite")
        sqlite = SqliteBackend(sqlite_index)
        rust = RustBackend(executable=args.klib_exe, index_dir=root / "tantivy")
        errors = []
        for path in paths:
            sqlite.index_file(str(path))
            try:
                rust.index_file(str(path))
            except Exception as error:
                errors.append({"path": path.name, "reason": str(error)})
                break
        comparisons = {}
        if not errors:
            for name, (query, hierarchy) in QUERIES.items():
                left = sqlite.search(query, hierarchy=hierarchy, limit=20)
                right = rust.search(query, hierarchy=hierarchy, limit=20)
                left_paths = [row.get("path") for row in left]
                right_paths = [row.get("path") for row in right]
                left_set, right_set = set(left_paths), set(right_paths)
                comparisons[name] = {
                    "query": query,
                    "sqlite_count": len(left_paths),
                    "rust_count": len(right_paths),
                    "top20_match_rate": len(left_set & right_set) / max(1, min(20, len(left_set), len(right_set))),
                    "missing_in_rust": sorted(left_set - right_set),
                    "extra_in_rust": sorted(right_set - left_set),
                }
        print(json.dumps({"documents": args.count, "seed": args.seed,
                          "status": "complete" if not errors else "rust_error",
                          "index_errors": errors, "comparisons": comparisons}, ensure_ascii=False, indent=2))
        rust.close()
        sqlite_index.close()
    return 0 if not errors else 2


if __name__ == "__main__":
    raise SystemExit(main())
