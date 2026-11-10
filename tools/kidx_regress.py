#!/usr/bin/env python3
"""Compare Python paragraph extraction with klib's JSON extract command."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from docfit_core.timemachine.extractors import ExtractionRecord
from docfit_core.timemachine.extractors_text import extract_plain_text
from docfit_core.timemachine.extractors_hwp import extract_hwp
from docfit_core.timemachine.extractors_hwpx import extract_hwpx
from docfit_core.timemachine.extractors_ooxml import extract_docx


def python_records(path: Path) -> list[dict]:
    try:
        ext = path.suffix.lower()
        if ext == ".hwpx":
            records = extract_hwpx(path)
        elif ext == ".docx":
            records = extract_docx(path)
        elif ext == ".hwp":
            records = extract_hwp(path)
        else:
            records = extract_plain_text(path)
    except Exception as exc:
        code = getattr(exc, "reason_code", "extract_failed")
        return [{"error": code}]
    return [
        {
            "text": r.text,
            "location_kind": r.location_kind,
            "location_label": r.location_label,
            "sequence": r.sequence,
            "section": r.section,
            "hierarchy_level": r.hierarchy_level,
            "is_table_cell": r.is_table_cell,
            "style_name": r.style_name,
            "outline_level": r.outline_level,
        }
        for r in records
    ]


def rust_records(path: Path, exe: Path) -> list[dict]:
    result = subprocess.run(
        [str(exe), "extract", str(path)], cwd=ROOT,
        capture_output=True, text=True, encoding="utf-8",
    )
    if result.returncode:
        error = result.stderr.strip() or f"klib exited {result.returncode}"
        code = error.split(":", 1)[0]
        if code in {"empty", "corrupt", "encrypted", "unsupported", "decode_error"}:
            return [{"error": code}]
        raise RuntimeError(error)
    data = json.loads(result.stdout)
    return data["paragraphs"] if isinstance(data, dict) else data


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", nargs="*", type=Path, help="files or directories; defaults to tests/fixtures")
    parser.add_argument("--exe", type=Path, default=ROOT / "klib" / "target" / "debug" / "klib.exe")
    parser.add_argument("--dump", action="store_true", help="print differing paragraph numbers and values")
    args = parser.parse_args()
    paths = args.paths or [ROOT / "tests" / "fixtures"]
    files = sorted({p.resolve() for item in paths for p in (item.rglob("*") if item.is_dir() else [item])
                    if p.is_file() and p.suffix.lower() in {".txt", ".md", ".hwpx", ".docx", ".hwp"}})
    failures = 0
    for path in files:
        try:
            py = python_records(path)
            rs = rust_records(path, args.exe.resolve())
        except Exception as exc:
            failures += 1
            print(f"ERROR {path.relative_to(ROOT)}: {exc}")
            continue
        if py == rs:
            print(f"OK {path.relative_to(ROOT)} ({len(py)} paragraphs)")
            continue
        failures += 1
        print(f"DIFF {path.relative_to(ROOT)}: Python={len(py)} Rust={len(rs)}")
        if args.dump:
            for i in range(max(len(py), len(rs))):
                p = py[i] if i < len(py) else "<missing>"
                r = rs[i] if i < len(rs) else "<missing>"
                if p != r:
                    print(f"  paragraph {i + 1}\n    Python: {json.dumps(p, ensure_ascii=False)}\n    Rust:   {json.dumps(r, ensure_ascii=False)}")
    print(f"Compared {len(files)} files; {failures} mismatches/errors")
    return 1 if failures else 0


if __name__ == "__main__":
    os.environ.setdefault("PYTHONIOENCODING", "utf-8")
    raise SystemExit(main())
