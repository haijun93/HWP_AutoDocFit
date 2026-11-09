#!/usr/bin/env python3
"""Compare Python and Rust paragraph levels on supported fixture documents."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument("--exe", type=Path, default=ROOT / "klib" / "target" / "debug" / "klib.exe")
    parser.add_argument("--stage", type=int, default=6, choices=range(8), help="classification branches enabled (0-7)")
    parser.add_argument("--allow-extraction-errors", action="store_true", help="exclude unextractable files from level comparison")
    parser.add_argument("--dump", action="store_true", help="print only paragraphs with differing levels")
    args = parser.parse_args()
    files = sorted({p.resolve() for item in (args.paths or [ROOT / "tests" / "fixtures"])
                    for p in (item.rglob("*") if item.is_dir() else [item])
                    if p.is_file() and p.suffix.lower() in {".txt", ".md", ".hwpx"}})
    if not files:
        parser.error("no .txt/.md/.hwpx files found")
    os.environ["PYTHONIOENCODING"] = "utf-8"
    from docfit_core.timemachine.content_search import (
        ParagraphRecord, LEVEL_ORDER, LEVEL_RANKS, extract_paragraphs_legacy,
        classify_paragraph_levels,
    )
    from docfit_core.timemachine.extractors import extract_document

    diffs = 0
    paragraph_count = 0
    failures = 0
    for path in files:
        try:
            ext = path.suffix.lower().lstrip(".")
            metadata = extract_paragraphs_legacy(path, ext) if ext in {"hwpx", "docx"} else None
            extracted = extract_document(path, hierarchy_records=metadata)
            paragraphs = [ParagraphRecord(
                paragraph_index=r.sequence - 1, text=r.text, level="body",
                level_rank=LEVEL_RANKS["body"], is_table_cell=r.is_table_cell,
                style_name=r.style_name, outline_level=r.outline_level,
            ) for r in extracted]
            paragraphs = classify_paragraph_levels(path, paragraphs)
        except Exception as error:
            failures += 1
            print(f"PYTHON_ERROR {path}: {error}")
            continue
        payload = [{"text": p.text, "is_table_cell": p.is_table_cell,
                    "style_name": p.style_name, "outline_level": p.outline_level}
                   for p in paragraphs]
        expected = [p.level for p in paragraphs]
        run = subprocess.run([str(args.exe.resolve()), "classify-levels", str(args.stage)], input=json.dumps(payload, ensure_ascii=False),
                             cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
        if run.returncode:
            print(f"RUST_ERROR {path}: {run.stderr.strip()}")
            return 2
        actual = json.loads(run.stdout)
        if len(actual) != len(expected):
            print(f"COUNT_DIFF {path}: Python={len(expected)} Rust={len(actual)}")
            diffs += abs(len(expected) - len(actual)) or 1
        for i, (python_level, rust_level) in enumerate(zip(expected, actual)):
            if python_level != rust_level:
                diffs += 1
                if args.dump:
                    print(f"DIFF {path} paragraph={i} Python={python_level!r} Rust={rust_level!r} text={paragraphs[i].text!r}")
        paragraph_count += len(expected)
    print(f"files={len(files)} extracted={len(files)-failures} errors={failures} paragraphs={paragraph_count} level_differences={diffs}")
    return 1 if diffs or (failures and not args.allow_extraction_errors) else 0


if __name__ == "__main__":
    raise SystemExit(main())
