#!/usr/bin/env python3
"""Compare ContentSearchIndex rows and search results with klib index-content."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

QUERIES = ["행정", "주민", "제1장", "목적", "사업", "지원", "신청", "기준", "계획", "제출"]
SUPPORTED = {".txt", ".md", ".hwpx", ".docx", ".hwp", ".xlsx", ".pptx"}
FIXTURE_QUERIES = ["노트북", "운반비", "슬라이드", "설명"]


def rows(db: Path) -> dict[str, list[tuple[int, str, str]]]:
    con = sqlite3.connect(db)
    try:
        out: dict[str, list[tuple[int, str, str]]] = {}
        for path, idx, level, body in con.execute(
            "SELECT path,paragraph_index,level,body FROM content_paragraph_fts ORDER BY path,CAST(paragraph_index AS INTEGER)"
        ):
            out.setdefault(path, []).append((int(idx), level, body))
        return out
    finally:
        con.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", type=Path, help="files or folders; defaults to supported fixtures")
    parser.add_argument("--exe", type=Path, default=ROOT / "klib" / "target" / "debug" / "klib.exe")
    parser.add_argument("--queries", nargs="+", default=QUERIES + FIXTURE_QUERIES, help="search terms; defaults include the ten legacy terms and OOXML fixture terms")
    parser.add_argument("--dump", action="store_true", help="print differing paragraph numbers and fields")
    args = parser.parse_args()
    files = sorted({p.resolve() for item in (args.paths or [ROOT / "tests" / "fixtures"])
                    for p in (item.rglob("*") if item.is_dir() else [item])
                    if p.is_file() and p.suffix.lower() in SUPPORTED})
    if not files:
        parser.error("no supported fixture files found")
    os.environ["PYTHONIOENCODING"] = "utf-8"
    from docfit_core.timemachine.content_search import ContentSearchIndex

    with tempfile.TemporaryDirectory(prefix="kidx-index-regress-") as temp:
        root = Path(temp)
        py_db, rs_db = root / "python.db", root / "rust.db"
        py = ContentSearchIndex(py_db)
        try:
            for path in files:
                result = py.index_file(path)
                if result["error"]:
                    print(f"PYTHON_ERROR {path}: {result['error']}")
            done = subprocess.run([str(args.exe.resolve()), "index-content", str(rs_db), *map(str, files)],
                                  cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
            if done.returncode:
                print(done.stderr.strip() or f"klib exited {done.returncode}")
                return 2
            progress = [json.loads(line) for line in done.stdout.splitlines() if line.strip()]
            print(f"Rust progress records: {len(progress)}; indexed: {sum(r['status']=='indexed' for r in progress)}; errors: {sum(r['status']=='error' for r in progress)}")
            py_rows, rs_rows = rows(py_db), rows(rs_db)
            doc_diffs = 0
            level_diffs = 0
            body_diffs = 0
            for path in sorted(set(py_rows) | set(rs_rows)):
                left, right = py_rows.get(path, []), rs_rows.get(path, [])
                if left != right:
                    doc_diffs += 1
                    first = next((i for i, pair in enumerate(zip(left, right)) if pair[0] != pair[1]), min(len(left), len(right)))
                    print(f"DOC_DIFF {path}: Python={len(left)} Rust={len(right)} first_difference={first}")
                    if args.dump:
                        for position in range(max(len(left), len(right))):
                            py_row = left[position] if position < len(left) else None
                            rs_row = right[position] if position < len(right) else None
                            if py_row == rs_row:
                                continue
                            paragraph = (py_row or rs_row)[0]
                            for field, py_value, rs_value in zip(("paragraph_index", "level", "body"), py_row or (None, None, None), rs_row or (None, None, None)):
                                if py_value != rs_value:
                                    print(f"  PARAGRAPH {paragraph} {field}: Python={py_value!r} Rust={rs_value!r}")
                for position in range(max(len(left), len(right))):
                    py_row = left[position] if position < len(left) else None
                    rs_row = right[position] if position < len(right) else None
                    if (py_row or (None, None, None))[1] != (rs_row or (None, None, None))[1]:
                        level_diffs += 1
                    if (py_row or (None, None, None))[2] != (rs_row or (None, None, None))[2]:
                        body_diffs += 1
            search_diffs = 0
            rs = ContentSearchIndex(rs_db)
            try:
                for query in args.queries:
                    a = py.search(query, limit=20, sort="hierarchy_relevance")
                    b = rs.search(query, limit=20, sort="hierarchy_relevance")
                    sig = lambda hits: [(os.path.normcase(os.path.abspath(h["path"])), h.get("best_level"), h.get("total_matches", 0)) for h in hits]
                    if sig(a) != sig(b):
                        search_diffs += 1
                        print(f"SEARCH_DIFF {query!r}: Python={len(a)} Rust={len(b)}")
            finally:
                rs.close()
            print(f"Compared {len(files)} files, {sum(map(len, py_rows.values()))} Python paragraphs, {sum(map(len, rs_rows.values()))} Rust paragraphs; document diffs={doc_diffs}; level diffs={level_diffs}; body diffs={body_diffs}; query diffs={search_diffs}")
            return 1 if doc_diffs or search_diffs else 0
        finally:
            py.close()


if __name__ == "__main__":
    raise SystemExit(main())
