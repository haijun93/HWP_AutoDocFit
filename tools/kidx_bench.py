#!/usr/bin/env python3
"""Measure full-document ContentSearchIndex indexing for a directory."""
from __future__ import annotations

import argparse
import json
import random
import shutil
import sys
import tempfile
import time
import tracemalloc
from pathlib import Path

SUPPORTED = {"hwpx", "txt", "md"}


def make_synthetic(source: Path, count: int, destination: Path) -> Path:
    candidates = [p for p in source.rglob("*") if p.is_file() and p.suffix.lower().lstrip(".") in SUPPORTED]
    if not candidates:
        raise ValueError(f"no .txt/.md/.hwpx fixtures under {source}")
    destination.mkdir(parents=True, exist_ok=True)
    rng = random.Random(0x4B494458)
    text_sources: dict[Path, list[str]] = {}
    for i in range(count):
        original = candidates[i % len(candidates)]
        target = destination / f"synth_{i + 1:04d}{original.suffix.lower()}"
        if original.suffix.lower() == ".hwpx":
            # Preserve valid package structure; copying gives realistic repeated HWPX parsing.
            shutil.copy2(original, target)
        else:
            lines = text_sources.get(original)
            if lines is None:
                raw = original.read_bytes()
                decoded = None
                for encoding in ("utf-8-sig", "cp949", "utf-16"):
                    try:
                        decoded = raw.decode(encoding)
                        break
                    except UnicodeError:
                        pass
                lines = (decoded or "").splitlines()
                if not lines:
                    lines = ["empty fixture"]
                text_sources[original] = lines
            mixed = list(lines)
            rng.shuffle(mixed)
            target.write_text("\n".join(mixed) + "\n", encoding="utf-8")
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("documents", type=Path, help="directory containing documents to index")
    parser.add_argument("database", type=Path, help="path for a new temporary SQLite index database")
    parser.add_argument("--synth", type=int, metavar="N", help="generate N synthetic documents from tests/fixtures")
    parser.add_argument("--fixtures", type=Path, default=Path("tests/fixtures"), help="fixture source directory")
    args = parser.parse_args()
    if args.synth is not None and args.synth < 0:
        parser.error("--synth must be zero or greater")

    temp_context = tempfile.TemporaryDirectory(prefix="kidx-synth-") if args.synth is not None else None
    try:
        docs = make_synthetic(args.fixtures, args.synth, Path(temp_context.name)) if temp_context else args.documents
        if not docs.is_dir():
            parser.error(f"document directory does not exist: {docs}")
        database = args.database.resolve()
        database.parent.mkdir(parents=True, exist_ok=True)
        if database.exists():
            parser.error(f"database already exists (use a new path): {database}")

        # Import after CLI parsing so --help works without loading the full application.
        sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
        from docfit_core.timemachine.content_search import ContentSearchIndex

        paths = sorted(p for p in docs.rglob("*") if p.is_file() and p.suffix.lower().lstrip(".") in
                       {"hwpx", "hwp", "txt", "md", "docx", "pptx", "xlsx", "xls", "pdf", "eml"})
        index = ContentSearchIndex(database)
        indexed = 0
        paragraphs = 0
        tracemalloc.start()
        started = time.perf_counter()
        try:
            for path in paths:
                result = index.index_file(path)
                if result["error"]:
                    continue
                row = index.db.execute("SELECT COUNT(*) FROM content_paragraph_fts WHERE path=?", (str(path.resolve()),)).fetchone()
                if row:
                    paragraphs += row[0]
                if result["indexed"]:
                    indexed += 1
            elapsed = time.perf_counter() - started
            _, peak = tracemalloc.get_traced_memory()
            errors = len(index.errors(roots=[str(docs.resolve())]))
            output = {"documents": indexed, "paragraphs": paragraphs, "errors": errors,
                      "wall_seconds": round(elapsed, 6), "peak_memory_bytes": peak,
                      "peak_memory_mib": round(peak / (1024 * 1024), 3),
                      "database": str(database), "source": str(docs.resolve())}
            print(json.dumps(output, ensure_ascii=False, separators=(",", ":")))
        finally:
            tracemalloc.stop()
            index.close()
        return 0
    finally:
        if temp_context:
            temp_context.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
