"""임시 트리의 타임머신 Python/Rust 스캔 결과 회귀 비교 및 단회 성능 측정."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

from docfit_core.timemachine.rules import Rules
from docfit_core.timemachine.scan import scan_roots
from docfit_core.timemachine.store import Store, path_key


def make_tree(base: Path, count: int) -> Path:
    root = base / "tree"
    root.mkdir()
    for index in range(count):
        folder = root / f"d{index // 1000:02d}"
        folder.mkdir(exist_ok=True)
        (folder / f"file{index:05d}.txt").write_bytes(b"x" * (index % 4096 + 1))
    # 규칙 회귀를 포함하되 요청된 1만 개의 정상 파일 수는 그대로 유지한다.
    (root / ".git").mkdir()
    (root / ".git" / "hidden.txt").write_text("ignored", encoding="utf-8")
    (root / "ignored.tmp").write_text("ignored", encoding="utf-8")
    (root / "unsupported.bin").write_text("ignored", encoding="utf-8")
    return root


def python_scan(root: Path, db_path: Path, rules: Rules):
    store = Store(str(db_path))
    started = time.perf_counter()
    result = scan_roots(store, [str(root)], rules)
    elapsed = time.perf_counter() - started
    # 화면 페이지 제한과 무관하게 전체 스캔 결과를 비교한다.
    with store._lock:
        records = [dict(row) for row in store._db.execute("SELECT path,name,kind,size,mtime,ctime,work_time,flags,cloud FROM files")]
    store.close()
    for row in records:
        row["flags"] = [x for x in row["flags"].split(",") if x]
        row["cloud"] = bool(row["cloud"])
    return records, elapsed, {"seen": result.seen, "written": result.written, "removed": result.removed,
                              "skipped_dirs": result.skipped_dirs, "errors": result.errors}


def rust_scan(exe: Path, root: Path, rules: Rules):
    request = {"extensions": list(rules.extensions), "extra_extensions": [], "extra_dirs": [], "extra_names": [],
               "time_mode": rules.time_mode, "include_hidden": rules.include_hidden}
    started = time.perf_counter()
    proc = subprocess.run([str(exe), "tm-scan", str(root), json.dumps(request, ensure_ascii=False)],
                          capture_output=True, text=True, encoding="utf-8", check=True)
    elapsed = time.perf_counter() - started
    value = json.loads(proc.stdout)
    return value["rows"], elapsed, {"skipped_dirs": value["skipped_dirs"], "errors": value["errors"]}


def normalize(rows, root):
    out = {}
    for r in rows:
        key = path_key(r["path"])
        out[key] = {k: r[k] for k in ("name", "kind", "size", "mtime", "ctime", "work_time", "flags", "cloud")}
    return out


def compare(py, rs, root):
    a, b = normalize(py, root), normalize(rs, root)
    differences = []
    for key in sorted(a.keys() | b.keys()):
        if key not in a or key not in b:
            differences.append({"path": key, "python": a.get(key), "rust": b.get(key)})
        else:
            changed = {field: {"python": a[key][field], "rust": b[key][field]}
                       for field in a[key] if a[key][field] != b[key][field]}
            if changed:
                differences.append({"path": key, "fields": changed})
    return differences


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--count", type=int, default=10_000)
    parser.add_argument("--klib", type=Path, default=Path(__file__).resolve().parents[1] / "klib" / "target" / "debug" / "klib.exe")
    parser.add_argument("--dump", action="store_true")
    args = parser.parse_args()
    if not args.klib.is_file():
        parser.error(f"debug klib executable not found: {args.klib}")
    with tempfile.TemporaryDirectory(prefix="tms1-regress-") as temp:
        base = Path(temp)
        root = make_tree(base, args.count)
        rules = Rules(extensions=["txt"])
        py, py_s, py_stats = python_scan(root, base / "python.sqlite", rules)
        rs, rs_s, rs_stats = rust_scan(args.klib, root, rules)
        diff = compare(py, rs, root)
        print(json.dumps({"count": args.count, "python_seconds": py_s, "rust_seconds": rs_s,
                          "python_rows": len(py), "rust_rows": len(rs), "difference_count": len(diff),
                          "python_stats": py_stats, "rust_stats": rs_stats}, ensure_ascii=False, indent=2))
        if args.dump and diff:
            print(json.dumps(diff, ensure_ascii=False, indent=2))
        return 1 if diff else 0


if __name__ == "__main__":
    raise SystemExit(main())
