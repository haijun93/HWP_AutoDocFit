#!/usr/bin/env python3
"""Compare Python and klib batch rename previews using temporary files only."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from docfit_core.file_rename import Entry, RULE_KINDS, build_plan  # noqa: E402


def fixture(root: Path, count: int):
    entries = []
    for i in range(count):
        folder = root / f"set{i % 7}"
        folder.mkdir(exist_ok=True)
        suffix = ".JPG" if i % 10 else ""
        path = folder / f"IMG_{i:04d} test {i % 11}{suffix}"
        path.write_bytes(b"rename-regression-fixture")
        entries.append(Entry.from_path(str(path), f"id{i:05d}"))
    # Existing unlisted targets exercise on-disk conflict detection.
    for i in range(0, count, 97):
        (root / f"set{i % 7}" / f"IMG_{i:04d} test {i % 11}_x.jpg").write_bytes(b"occupied")
    return entries


def python_preview(entries, rules, options):
    result = build_plan(entries, rules, overrides=options.get("overrides", {}),
                      excluded=options.get("excluded", []),
                      on_conflict=options.get("on_conflict", "block"))
    return result


def compare(count: int, klib: Path, rules: list[dict], options: dict, dump: int = 0):
    with tempfile.TemporaryDirectory(prefix="rename-regress-") as temp:
        root = Path(temp)
        entries = fixture(root, count)
        req = {"cmd": "rename_preview", "rules": rules, "on_conflict": options.get("on_conflict", "block"),
               "overrides": options.get("overrides", {}), "excluded": options.get("excluded", []),
               "entries": [{"id": e.id, "path": e.path, "name": e.name, "size": e.size,
                            "mtime": e.mtime, "ctime": e.ctime, "is_folder": e.is_folder} for e in entries],
               "existing_paths": [str(p) for p in root.rglob("*") if p.is_file() and
                                  not any(e.path == str(p) for e in entries)]}
        req["existing_paths"] = [os.path.normpath(p) for p in req["existing_paths"]]
        request_file = root / "request.json"
        request_file.write_text(json.dumps(req, ensure_ascii=False), encoding="utf-8")
        t0 = time.perf_counter()
        py = python_preview(entries, rules, options)
        py_ms = (time.perf_counter() - t0) * 1000
        t0 = time.perf_counter()
        proc = subprocess.run([str(klib), "--rename-preview-json", f"@{request_file}"],
                              capture_output=True, text=True, encoding="utf-8", check=True)
        rust_ms = (time.perf_counter() - t0) * 1000
        rust = json.loads(proc.stdout)
        diffs = []
        def walk(a, b, path="$", cap=100):
            if len(diffs) >= cap:
                return
            if isinstance(a, dict) and isinstance(b, dict):
                for key in sorted(set(a) | set(b)):
                    if path == "$" and key == "plan_id":
                        continue  # hash serialization is implementation-specific
                    if key not in a or key not in b:
                        diffs.append({"field": f"{path}.{key}", "python": a.get(key), "rust": b.get(key)})
                    else:
                        walk(a[key], b[key], f"{path}.{key}", cap)
            elif isinstance(a, list) and isinstance(b, list):
                if len(a) != len(b):
                    diffs.append({"field": f"{path}.length", "python": len(a), "rust": len(b)})
                for index, (left, right) in enumerate(zip(a, b)):
                    walk(left, right, f"{path}[{index}]", cap)
            elif a != b:
                diffs.append({"field": path, "python": a, "rust": b})
        walk(py, rust)
        result = {"count": count, "differences": len(diffs), "python_ms": round(py_ms, 3),
                  "rust_ms": round(rust_ms, 3), "samples": diffs[:20]}
        if dump:
            result["dump"] = diffs[:dump]
        return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--klib", type=Path, required=True)
    parser.add_argument("--count", type=int, default=1000)
    parser.add_argument("--rules-json", type=Path)
    parser.add_argument("--dump", type=int, nargs="?", const=5, default=0,
                        help="print up to N differing fields (default: 5)")
    args = parser.parse_args()
    if args.rules_json:
        rules = json.loads(args.rules_json.read_text(encoding="utf-8"))
    else:
        rules = [{"kind": "add", "text": "x", "position": "suffix", "sep": "_"}]
    result = compare(args.count, args.klib.resolve(), rules, {"on_conflict": "block"}, args.dump)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["differences"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
