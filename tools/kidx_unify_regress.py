#!/usr/bin/env python3
"""Compare Python and Rust unify_marker results across tests/fixtures paragraphs."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from docfit_core.style_unify import unify_marker
from tools.hier_regress import load_hwpx, load_txt


def main() -> int:
    dump = "--dump" in sys.argv[1:]
    fixture_root = REPO / "tests" / "fixtures"
    texts: list[str] = []
    for path in sorted(fixture_root.rglob("*")):
        if path.is_file() and path.suffix.lower() in {".hwpx", ".txt"}:
            loader = load_hwpx if path.suffix.lower() == ".hwpx" else load_txt
            texts.extend(str(p.get("text", "")) for p in loader(path))
    expected = [list(unify_marker(text)) for text in texts]
    exe = REPO / "klib" / "target" / "debug" / "klib.exe"
    if not exe.exists():
        exe = REPO / "klib" / "target" / "release" / "klib.exe"
    proc = subprocess.run([str(exe), "unify-marker"], input=json.dumps(texts, ensure_ascii=False), text=True,
                          encoding="utf-8", capture_output=True, check=True)
    actual = json.loads(proc.stdout)
    differences = [(i, texts[i], expected[i], actual[i]) for i in range(min(len(texts), len(actual))) if expected[i] != actual[i]]
    if len(expected) != len(actual):
        print(f"count mismatch: python={len(expected)} rust={len(actual)}")
        return 1
    if dump:
        for i, text in enumerate(texts):
            print(f"[{i}] input={text!r}\n  Python: {expected[i]!r}\n  Rust:   {actual[i]!r}")
    else:
        for i, text, py, rust in differences[:30]:
            print(f"[{i}] input={text!r}\n  Python: {py!r}\n  Rust:   {rust!r}")
    print(f"paragraphs={len(texts)} differences={len(differences)}")
    return 1 if differences else 0


if __name__ == "__main__":
    raise SystemExit(main())
