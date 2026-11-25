"""합성 seed 20261124의 전체 집합·순위·묶음 근거 SQLite/Rust 대조.

사용: python scripts/verify_semicolon_search.py --klib-exe <빌드 exe> --output <JSON>
"""
import argparse
import json
import os
from pathlib import Path
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.search_synthetic import make_corpus
from docfit_core.timemachine.content_search import ContentSearchIndex, extract_paragraphs
from docfit_core.timemachine.search_backend import SqliteBackend, RustBackend


def all_rows(backend, query, filenames):
    rows = []
    while True:
        page = backend.search(query, related_groups=True, filenames=filenames,
                              limit=500, offset=len(rows))
        rows.extend(page)
        if len(page) < 500:
            return rows


def run(executable, count, *, native_index=False):
    with tempfile.TemporaryDirectory(prefix="semi1-compare-") as temp:
        root = Path(temp)
        corpus = root / "corpus"
        corpus.mkdir()
        paths = make_corpus(corpus, count, 20261124)
        controls = {
            "2026 공사 계약 R&D.md": "# 2026\n공사 계약\nR&D a;b\n",
            "split.md": "2026\n공사\n계약\n",
            "2026 공사계약 RD.txt": "2026\n공사 계약\nRD\n",
            "count.txt": "2026\n2026\n공사 계약\n공사 계약\n",
            "Report R&D.txt": "İ REPORT a;b\n공사 계약\n",
        }
        for name, text in controls.items():
            path = corpus / name
            path.write_text(text, encoding="utf-8")
            paths.append(path)
        index = ContentSearchIndex(root / "content.sqlite")
        sqlite = SqliteBackend(index)
        rust = RustBackend(executable, index_dir=root / "tantivy", timeout=90)
        cases = []
        try:
            with index.batch(max_files=10000, max_seconds=60):
                for path in paths:
                    sqlite.index_file(path)
            if native_index:
                rust.index_files(paths)
            else:
                rust.index_many((str(path), extract_paragraphs(path)) for path in paths)
            # A late intersection must survive >500 candidates in an earlier group.
            queries = ["행정; 정책", '행정; "안전 점검 계획"', "행정 정책; 안전 지원",
                       "2026; 공사 계약", '; 2026;; "공사 계약";', 'R&D; "a;b"',
                       "행정; 없는검색어", "2026; 2026", "synthetic; 000", '""; ;',
                       "report; R&D", 'i; "a;b"']
            for filenames in (False, True):
                for query in queries:
                    left = all_rows(sqlite, query, filenames)
                    right = all_rows(rust, query, filenames)
                    a = [Path(r["path"]).name for r in left]
                    b = [Path(r["path"]).name for r in right]
                    info = {"query": query, "filenames": filenames,
                            "sqlite": len(a), "rust": len(b),
                            "missing": sorted(set(a)-set(b)), "extra": sorted(set(b)-set(a)),
                            "order_equal": a == b,
                            "evidence_equal": [r["group_matches"] for r in left] == [r["group_matches"] for r in right]}
                    cases.append(info)
            return {"seed": 20261124, "generated_documents": count, "native_index": native_index,
                    "control_documents": len(controls), "cases": cases,
                    "ok": all(not c["missing"] and not c["extra"] and c["order_equal"] and c["evidence_equal"] for c in cases)}
        finally:
            rust.close()
            index.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--klib-exe", required=True)
    parser.add_argument("--documents", type=int, default=1000)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--native-index", action="store_true", help="공통 SRCH index_batch 경로도 대조")
    args = parser.parse_args()
    result = run(args.klib_exe, args.documents, native_index=args.native_index)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result["ok"] else 1)
