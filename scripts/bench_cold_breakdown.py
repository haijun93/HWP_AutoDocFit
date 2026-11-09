"""Benchmark Cold Launch Breakdown: C# vs Rust.

Measures Cold launch breakdown stages:
1. Decompress (entry read/deflate)
2. File Write (file create, write_all, SHA-256, rename cache)
3. Child Spawn (spawning child process)
4. Handoff Wait (waiting for child handoff event)
Along with overall timeline (first window, launcher exit, main window).
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from scripts.build_smart_launcher import build
from scripts.bench_real_app_launch import ensure_v2248_bundle, find_matching_windows, kill_isolated_processes

user32 = ctypes.windll.user32


def measure_single_cold_breakdown(
    launcher_exe: Path,
    isolated_dir: Path,
    version: str,
    target_window_title: str = "한글편집 후처리",
    timeout: float = 45.0,
) -> dict:
    local_app_data = isolated_dir / "local"
    roaming_app_data = isolated_dir / "roaming"
    test_root = isolated_dir / "cache"
    wv2_dir = isolated_dir / "wv2"
    trace_path = isolated_dir / "launcher_trace.txt"

    local_app_data.mkdir(parents=True, exist_ok=True)
    roaming_app_data.mkdir(parents=True, exist_ok=True)
    test_root.mkdir(parents=True, exist_ok=True)
    wv2_dir.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["LOCALAPPDATA"] = str(local_app_data)
    env["APPDATA"] = str(roaming_app_data)
    env["DOCFIT_LAUNCHER_TEST_ROOT"] = str(test_root)
    env["WEBVIEW2_USER_DATA_FOLDER"] = str(wv2_dir)
    env["DOCFIT_OFFLINE"] = "1"
    env["HWP_LAUNCHER_TRACE"] = "1"
    env["DOCFIT_LAUNCHER_TRACE"] = str(trace_path)

    pids_to_kill = set()
    started = time.perf_counter()
    first_window_ms = None
    first_window_info = None
    main_window_ms = None
    main_window_info = None
    launcher_exit_ms = None
    launcher_exit_code = None

    proc = subprocess.Popen(
        [str(launcher_exe)],
        env=env,
        cwd=isolated_dir,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    pids_to_kill.add(proc.pid)

    deadline = time.perf_counter() + timeout
    while time.perf_counter() < deadline:
        now = time.perf_counter()
        elapsed = round((now - started) * 1000, 2)

        wins = find_matching_windows(target_window_title)
        for win in wins:
            pids_to_kill.add(win["pid"])
            if first_window_ms is None:
                first_window_ms = elapsed
                first_window_info = win
            if "실행 준비" not in win["title"] and main_window_ms is None:
                main_window_ms = elapsed
                main_window_info = win

        if launcher_exit_ms is None:
            ret = proc.poll()
            if ret is not None:
                launcher_exit_ms = elapsed
                launcher_exit_code = ret

        if main_window_ms is not None:
            break
        time.sleep(0.02)

    kill_isolated_processes(isolated_dir, pids_to_kill)
    try:
        _, stderr = proc.communicate(timeout=2)
    except Exception:
        stderr = ""

    time.sleep(1.0)

    # Parse trace metrics
    metrics: dict[str, float] = {}
    if trace_path.is_file():
        for line in trace_path.read_text(encoding="utf-8", errors="ignore").splitlines():
            line = line.strip()
            if not line:
                continue
            parts = line.split("\t")
            if len(parts) >= 2:
                name, val = parts[0], parts[1]
                if name.startswith("metric_"):
                    metric_name = name[len("metric_"):]
                    try:
                        metrics[metric_name] = float(val)
                    except ValueError:
                        pass

    if stderr:
        for line in stderr.splitlines():
            line = line.strip()
            if "HWP_LAUNCHER_TRACE metric=" in line:
                parts = line.split()
                name = None
                duration = None
                for p in parts:
                    if p.startswith("metric="):
                        name = p.split("=", 1)[1]
                    elif p.startswith("duration_ms="):
                        try:
                            duration = float(p.split("=", 1)[1])
                        except ValueError:
                            pass
                if name and duration is not None:
                    metrics[name] = duration

    return {
        "ok": first_window_ms is not None or main_window_ms is not None,
        "first_window_ms": first_window_ms,
        "first_window_title": first_window_info["title"] if first_window_info else None,
        "launcher_exit_ms": launcher_exit_ms,
        "launcher_exit_code": launcher_exit_code,
        "main_window_ms": main_window_ms,
        "main_window_title": main_window_info["title"] if main_window_info else None,
        "metrics": metrics,
    }


def run_breakdown_benchmark(output_json: Path | None = None) -> dict:
    version = "2.2.48"
    bundle = ensure_v2248_bundle()

    bench_dir = ROOT / "dist" / "bench_real"
    bench_dir.mkdir(parents=True, exist_ok=True)

    csharp_exe = bench_dir / "csharp" / "HWP_AutoDocFit_CSharp.exe"
    rust_exe = bench_dir / "rust" / "HWP_AutoDocFit_Rust.exe"

    print("[1/2] Building C# launcher with metrics...")
    build(bundle, csharp_exe, version, launcher="csharp")

    print("[2/2] Building Rust launcher with metrics...")
    build(bundle, rust_exe, version, launcher="rust")

    results = {}
    with tempfile.TemporaryDirectory(prefix="docfit-breakdown-") as tmp:
        root = Path(tmp)

        for name, exe in [("csharp", csharp_exe), ("rust", rust_exe)]:
            print(f"\n[{name}] Running single Cold launch breakdown measurement...")
            cold_dir = root / name / "cold"
            res = measure_single_cold_breakdown(
                exe, cold_dir, version, target_window_title="한글편집 후처리", timeout=45.0,
            )
            print(f"  -> First window: {res['first_window_ms']} ms ({res['first_window_title']})")
            print(f"  -> Launcher exit: {res['launcher_exit_ms']} ms (code: {res['launcher_exit_code']})")
            print(f"  -> Main window: {res['main_window_ms']} ms ({res['main_window_title']})")
            print(f"  -> Metrics: {json.dumps(res['metrics'], ensure_ascii=False)}")
            results[name] = res
            time.sleep(1.5)

    if output_json:
        output_json.parent.mkdir(parents=True, exist_ok=True)
        output_json.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print("\n=== Cold Launch Breakdown Summary ===")
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=ROOT / "dist" / "RL1G.breakdown.initial.json")
    args = parser.parse_args()
    run_breakdown_benchmark(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
