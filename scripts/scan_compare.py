#!/usr/bin/env python3
"""Python 서비스와 klib scan의 동일 입력 전수 비교 및 재현 가능한 1회 측정."""
from __future__ import annotations

import argparse
import cProfile
import io
import json
import os
import pstats
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
if str(REPO) not in sys.path:
    sys.path.insert(0, str(REPO))
from docfit_app.doc_grid_service import DOC_EXTENSIONS, DocGridService
from docfit_app.library_scope import _EXCLUDED_NAMES, is_excluded_path
from docfit_app.photo_album_service import PHOTO_EXTENSIONS, PhotoAlbumService
from docfit_core.timemachine.rules import cloud_only, excluded_name

EXCLUDED = tuple(sorted(_EXCLUDED_NAMES | {
    "temp", "packages", "user data", "cache", "code cache", "gpucache", "__pycache__",
    ".dart_tool", ".gradle", ".idea", ".vscode", "target", "build",
}))


def python_scan(root: Path, extensions: set[str], excluded: tuple[Path, ...], *, profiled: bool = False):
    if extensions == DOC_EXTENSIONS:
        with tempfile.TemporaryDirectory(prefix="scan-cache-") as td:
            service = DocGridService(thumb_dir=Path(td) / "thumbs", drive_roots=[root])
            profiler = cProfile.Profile() if profiled else None
            start = time.perf_counter()
            if profiler: profiler.enable()
            results = service.list_documents(root, recursive=True, excluded_folders=excluded)
            if profiler: profiler.disable()
            elapsed = time.perf_counter() - start
            rows = []
            for r in results:
                ns = Path(r["path"]).stat().st_mtime_ns
                rows.append({"path": r["path"], "name": r["name"], "ext": r["ext"], "size": r["size"],
                             "mtime": ns / 1e9, "mtime_ns": ns, "cloud": r["cloud"]})
            return rows, elapsed, {"walk": _count_dirs(root, excluded), "excluded": len(excluded), "stat": len(rows), "cache": len(rows)}, profiler
    if extensions == PHOTO_EXTENSIONS:
        with tempfile.TemporaryDirectory(prefix="scan-photo-") as td:
            service = PhotoAlbumService(db_path=Path(td) / "photos.sqlite", thumb_dir=Path(td) / "thumbs", drive_roots=[root])
            profiler = cProfile.Profile() if profiled else None
            start = time.perf_counter()
            if profiler: profiler.enable()
            # 기준 스캔의 메타데이터 경로를 측정한다. 서비스가 변경 감지 후 DB에 쓰는 비용은
            # 임시 DB에서 격리되며, 매번 메타데이터 재처리를 위해 DB는 비워 둔다.
            result = service.scan_folder(root, recursive=True, excluded_folders=excluded)
            if profiler: profiler.disable()
            elapsed = time.perf_counter() - start
            rows = []
            for current, dirs, files in os.walk(root):
                dirs[:] = [d for d in dirs if not d.startswith(".") and not excluded_name(d)
                           and not is_excluded_path(Path(current) / d, excluded, base=root)]
                for name in files:
                    path = Path(current) / name
                    if name.startswith(".") or excluded_name(name) or path.suffix.lower() not in extensions or is_excluded_path(path, excluded, base=root): continue
                    st = path.stat()
                    rows.append({"path": str(path.resolve()), "name": name, "ext": path.suffix.lower().lstrip("."), "size": st.st_size,
                                 "mtime": st.st_mtime, "mtime_ns": st.st_mtime_ns, "cloud": cloud_only(getattr(st, "st_file_attributes", 0))})
            return rows, elapsed, {"walk": _count_dirs(root, excluded), "excluded": len(excluded), "stat": result["scanned"], "cache": len(rows)}, profiler
    rows = []
    counts = {"walk": 0, "excluded": 0, "stat": 0, "cache": 0}
    started = time.perf_counter()

    def walk(folder: Path):
        counts["walk"] += 1
        try:
            with os.scandir(folder) as it:
                for entry in it:
                    name = entry.name
                    if name.startswith(".") or excluded_name(name):
                        continue
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            if not is_excluded_path(entry.path, excluded, base=root):
                                walk(Path(entry.path))
                            counts["excluded"] += 1
                            continue
                        if not entry.is_file(follow_symlinks=False):
                            continue
                        counts["excluded"] += 1
                        if is_excluded_path(entry.path, excluded, base=root):
                            continue
                        ext = Path(name).suffix.lower()
                        if ext not in extensions:
                            continue
                        counts["stat"] += 1
                        st = entry.stat(follow_symlinks=False)
                        attrs = getattr(st, "st_file_attributes", 0)
                        rows.append({"path": os.path.abspath(entry.path), "name": name,
                                     "ext": ext.lstrip("."), "size": st.st_size,
                                     "mtime": st.st_mtime, "mtime_ns": st.st_mtime_ns,
                                     "cloud": cloud_only(attrs)})
                        # 서비스는 결과를 만든 뒤 썸네일 캐시를 조회한다. 기준 비교 모드에서 비용을 재현한다.
                        counts["cache"] += 1
                    except OSError:
                        continue
        except OSError:
            pass

    profiler = cProfile.Profile() if profiled else None
    if profiler: profiler.enable()
    walk(root)
    if profiler: profiler.disable()
    rows.sort(key=lambda r: r["name"].casefold())
    elapsed = time.perf_counter() - started
    return rows, elapsed, counts, profiler



def _count_dirs(root, excluded):
    count = 0
    for current, dirs, _files in os.walk(root):
        dirs[:] = [d for d in dirs if not d.startswith(".") and not excluded_name(d)
                   and not is_excluded_path(Path(current) / d, excluded, base=root)]
        count += 1
    return count

def rust_scan(binary: Path, root: Path, excluded: tuple[Path, ...], extensions: set[str]):
    command = [str(binary), "scan", "--roots", str(root), "--ext", ",".join(sorted(x.lstrip(".") for x in extensions)), "--json-lines"]
    if excluded:
        command += ["--exclude", *(str(x) for x in excluded)]
    started = time.perf_counter()
    proc = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", check=True)
    elapsed = time.perf_counter() - started
    rows = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
    return rows, elapsed


def compare(py_rows, rust_rows):
    py = {os.path.normcase(os.path.normpath(x["path"])): x for x in py_rows}
    rs = {os.path.normcase(os.path.normpath(x["path"])): x for x in rust_rows}
    differences = []
    for path in sorted(py.keys() - rs.keys()): differences.append({"kind": "python_only", "path": py[path]["path"]})
    for path in sorted(rs.keys() - py.keys()): differences.append({"kind": "rust_only", "path": rs[path]["path"]})
    for path in sorted(py.keys() & rs.keys()):
        for field in ("size", "mtime_ns", "cloud"):
            if py[path][field] != rs[path][field]:
                differences.append({"kind": "field", "field": field, "path": py[path]["path"], "python": py[path][field], "rust": rs[path][field]})
    return differences


def profile_rows(profiler, elapsed):
    stats = pstats.Stats(profiler, stream=io.StringIO()).sort_stats("cumulative")
    functions = []
    for (filename, line, name), (cc, nc, tt, ct, callers) in stats.stats.items():
        functions.append((ct, tt, nc, name, filename, line))
    functions.sort(reverse=True)
    return [{"function": f"{name} ({Path(filename).name}:{line})", "calls": calls,
             "cumulative_s": round(cum, 6), "cumulative_pct": round(cum / elapsed * 100, 2) if elapsed else 0}
            for cum, total, calls, name, filename, line in functions[:20]]


def write_profile(path: Path, data: dict):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path)
    ap.add_argument("--binary", type=Path, default=REPO / "klib" / "target" / "release" / "klib.exe")
    ap.add_argument("--profile-out", type=Path)
    ap.add_argument("--mode", choices=("docs", "photos", "both"), default="both")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)
    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="scan-compare-") as td:
            root = Path(td) / "한글"
            root.mkdir(parents=True, exist_ok=True)
            (root / "문서").mkdir(parents=True)
            long_dir = root / "문서" / ("긴 경로_" * 5)
            long_dir.mkdir(parents=True, exist_ok=True)
            (long_dir / "자료.pdf").write_bytes(b"x")
            (root / "문서" / ".숨김.pdf").touch()
            (root / "node_modules" / "nested").mkdir(parents=True)
            (root / "node_modules" / "nested" / "hidden.pdf").touch()
            (root / "Temp" / "nested").mkdir(parents=True)
            (root / "Temp" / "nested" / "temp.pdf").touch()
            (root / "자료.hwp.tmp").touch()
            rows, _, _, _ = python_scan(root, DOC_EXTENSIONS, tuple(root / n for n in EXCLUDED))
            assert [r["name"] for r in rows] == ["자료.pdf"]
        print("scan_compare self-test: PASS (임시 트리, 숨김·제외 규칙)")
        return 0
    if os.name != "nt":
        print("scan_compare: Windows에서만 실행할 수 있습니다", file=sys.stderr); return 2
    if not args.root or not args.root.is_dir():
        ap.error("--root must name an existing directory")
    if not args.binary.is_file():
        ap.error(f"Rust scanner executable not found: {args.binary}")
    root = args.root.resolve()
    excluded = tuple(root / name for name in EXCLUDED)
    modes = {"docs": (DOC_EXTENSIONS,), "photos": (PHOTO_EXTENSIONS,), "both": (DOC_EXTENSIONS | PHOTO_EXTENSIONS,)}[args.mode]
    outputs = {}
    for extensions in modes:
        py_rows, py_time, counts, profiler = python_scan(root, extensions, excluded, profiled=True)
        rust_rows, rust_time = rust_scan(args.binary.resolve(), root, excluded, extensions)
        diffs = compare(py_rows, rust_rows)
        outputs["+".join(sorted(extensions))] = {
            "root": str(root), "python": {"seconds": py_time, "count": len(py_rows), "per_second": len(py_rows)/py_time if py_time else 0,
                                            "counts": counts, "profile_top": profile_rows(profiler, py_time)},
            "rust": {"seconds": rust_time, "count": len(rust_rows), "per_second": len(rust_rows)/rust_time if rust_time else 0},
            "difference_count": len(diffs), "differences": diffs,
        }
        print(f"{args.mode}: python={len(py_rows)} {py_time:.3f}s ({len(py_rows)/py_time if py_time else 0:.1f}/s), rust={len(rust_rows)} {rust_time:.3f}s ({len(rust_rows)/rust_time if rust_time else 0:.1f}/s), differences={len(diffs)}")
        for row in outputs["+".join(sorted(extensions))]["python"]["profile_top"][:10]:
            print(f"PROFILE {row['function']} calls={row['calls']} cum={row['cumulative_s']:.6f}s ({row['cumulative_pct']:.2f}%)")
        if diffs:
            for diff in diffs[:100]: print("DIFF", json.dumps(diff, ensure_ascii=False))
    if args.profile_out: write_profile(args.profile_out, outputs)
    return 1 if any(x["difference_count"] for x in outputs.values()) else 0


if __name__ == "__main__":
    raise SystemExit(main())
