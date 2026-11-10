from __future__ import annotations

import json
import os
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from docfit_core.file_rename import Entry, build_plan

KLIB = ROOT / "klib" / "target" / "release" / "klib.exe"
FIXED = 1_700_000_000


def rules_for(name: str) -> list[dict]:
    return {
        "replace_add_ext": [
            {"kind": "replace", "find": "IMG_", "to": "Image_"},
            {"kind": "add", "text": "_final", "position": "suffix", "sep": ""},
            {"kind": "ext_case", "mode": "lower"},
        ],
        "digits_cleanup_ext": [
            {"kind": "digits_pad", "which": "last", "digits": 6},
            {"kind": "cleanup"},
            {"kind": "ext_case", "mode": "lower"},
        ],
        "date_number_ext": [
            {"kind": "date", "source": "mtime", "format": "YYYYMMDD", "position": "prefix", "sep": "_"},
            {"kind": "number", "digits": 4, "position": "suffix", "sep": "_"},
            {"kind": "ext_case", "mode": "lower"},
        ],
        "digits_only": [{"kind": "digits_pad", "which": "last", "digits": 6}],
        "cleanup_only": [{"kind": "cleanup"}],
    }[name]


def run_case(count: int, case: str, rounds: int = 7) -> dict:
    rules = rules_for(case)
    with tempfile.TemporaryDirectory(prefix="rnrs7-") as td:
        root = Path(td)
        entries = []
        for i in range(count):
            folder = root / f"set{i % 20:02d}"
            folder.mkdir(exist_ok=True)
            path = folder / f"IMG_{i:06d} 보고서(최종).JPG"
            path.touch()
            ts = FIXED + i
            os.utime(path, (ts, ts))
            entries.append(Entry(f"id{i:06d}", str(path), 0, ts, ts, False))
        request = {
            "cmd": "rename_preview", "rules": rules, "on_conflict": "block",
            "entries": [{"id": e.id, "path": e.path, "name": e.name, "size": e.size,
                         "mtime": e.mtime, "ctime": e.ctime, "is_folder": e.is_folder} for e in entries],
            "existing_paths": [],
        }
        request_path = root / "request.json"
        request_path.write_text(json.dumps(request, ensure_ascii=False), encoding="utf-8")
        py_times, rust_times = [], []
        for _ in range(rounds):
            start = time.perf_counter()
            py = build_plan(entries, rules)
            py_times.append((time.perf_counter() - start) * 1000)
            start = time.perf_counter()
            proc = subprocess.run([str(KLIB), "--rename-preview-json", f"@{request_path}"],
                                  capture_output=True, text=True, encoding="utf-8", check=True)
            rust_times.append((time.perf_counter() - start) * 1000)
            rust = json.loads(proc.stdout)
            if [x["new_name"] for x in py["items"]] != [x["new_name"] for x in rust["items"]]:
                raise AssertionError(f"output mismatch in {case}/{count}")
        return {"count": count, "case": case,
                "python_ms": round(statistics.median(py_times), 3),
                "rust_ms": round(statistics.median(rust_times), 3),
                "python_samples": [round(x, 3) for x in py_times],
                "rust_samples": [round(x, 3) for x in rust_times]}


if __name__ == "__main__":
    results = []
    for n in (1000, 5000):
        for case in ("replace_add_ext", "digits_cleanup_ext", "date_number_ext"):
            results.append(run_case(n, case))
    for n in (1000, 5000):
        for case in ("digits_only", "cleanup_only"):
            results.append(run_case(n, case))
    print(json.dumps(results, ensure_ascii=False, indent=2))
