"""Benchmark actual app GUI launch: C# launcher vs Rust launcher.

Uses the real HWP_AutoDocFit v2.2.48 app bundle as the child process.
Measures time until the visible window with title substring "한글편집 후처리" appears.
Ensures APPDATA isolation and strictly terminates only child/launcher processes by PID.
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

user32 = ctypes.windll.user32


def find_matching_windows(target_substring: str = "한글편집 후처리") -> list[dict]:
    found = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def callback(hwnd, _lparam):
        if not user32.IsWindowVisible(hwnd):
            return True
        length = user32.GetWindowTextLengthW(hwnd)
        if length > 0:
            buf = ctypes.create_unicode_buffer(length + 1)
            user32.GetWindowTextW(hwnd, buf, length + 1)
            title = buf.value
            if target_substring in title:
                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                found.append({"hwnd": hwnd, "title": title, "pid": pid.value})
        return True

    user32.EnumWindows(callback, 0)
    return found


def kill_pids(pids: set[int]):
    for pid in list(pids):
        if pid and pid > 0:
            subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True, check=False)


def measure_single_launch(
    launcher_exe: Path,
    isolated_dir: Path,
    version: str,
    target_window_title: str = "한글편집 후처리",
    timeout: float = 35.0,
) -> dict:
    local_app_data = isolated_dir / "local"
    roaming_app_data = isolated_dir / "roaming"
    test_root = isolated_dir / "cache"
    wv2_dir = isolated_dir / "wv2"

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

        # Check matching windows
        wins = find_matching_windows(target_window_title)
        for win in wins:
            pids_to_kill.add(win["pid"])
            if first_window_ms is None:
                first_window_ms = elapsed
                first_window_info = win
            if "실행 준비" not in win["title"] and main_window_ms is None:
                main_window_ms = elapsed
                main_window_info = win

        # Check launcher process exit
        if launcher_exit_ms is None:
            ret = proc.poll()
            if ret is not None:
                launcher_exit_ms = elapsed
                launcher_exit_code = ret

        # If both first window and launcher exited, or main window appeared
        if (first_window_ms is not None and launcher_exit_ms is not None) or main_window_ms is not None:
            # If main window not seen yet and launcher already exited, give a little time for main window
            if main_window_ms is None and launcher_exit_ms is not None and (now - started) < 15.0:
                time.sleep(0.05)
                continue
            break
        time.sleep(0.02)

    # Clean up launcher and child processes strictly
    kill_pids(pids_to_kill)
    try:
        _, stderr = proc.communicate(timeout=2)
    except Exception:
        stderr = ""

    # Extra verification: wait for killed processes to disappear
    time.sleep(0.5)

    trace_lines = []
    if stderr:
        trace_lines = [l for l in stderr.splitlines() if "HWP_LAUNCHER_TRACE" in l]

    return {
        "ok": first_window_ms is not None,
        "first_window_ms": first_window_ms,
        "first_window_title": first_window_info["title"] if first_window_info else None,
        "main_window_ms": main_window_ms,
        "main_window_title": main_window_info["title"] if main_window_info else None,
        "launcher_exit_ms": launcher_exit_ms,
        "launcher_exit_code": launcher_exit_code,
        "window_info": first_window_info,
        "trace": trace_lines,
    }


def ensure_v2248_bundle() -> Path:
    bundle_dir = ROOT / "dist" / "v2.2.48_bundle"
    if (bundle_dir / "HWP_AutoDocFit.exe").is_file() and (bundle_dir / "_internal").is_dir():
        return bundle_dir

    bundle_dir.mkdir(parents=True, exist_ok=True)
    release_exe = ROOT / "release-assets" / "HWP_AutoDocFit_v2.2.48_Portable.exe"
    if not release_exe.is_file():
        alt_exe = Path(r"C:\HWP_AUTODOCFIT\v1803\rel2248\HWP_AutoDocFit_v2.2.48_Portable.exe")
        if alt_exe.is_file():
            release_exe.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(alt_exe, release_exe)
        else:
            raise FileNotFoundError(f"v2.2.48 portable exe not found: {release_exe}")

    with tempfile.TemporaryDirectory(prefix="docfit-extract-bundle-") as tmp:
        cache_dir = Path(tmp) / "cache"
        cache_dir.mkdir()
        env = os.environ.copy()
        env["LOCALAPPDATA"] = str(tmp)
        env["APPDATA"] = str(tmp)
        env["DOCFIT_LAUNCHER_TEST_ROOT"] = str(cache_dir)
        subprocess.run([str(release_exe), "--self-test"], env=env, check=True)

        app_dirs = list(cache_dir.glob("app_v*"))
        if not app_dirs:
            raise RuntimeError("failed to extract bundle from release portable exe")
        extracted = app_dirs[0]

        for item in extracted.iterdir():
            if item.name == ".docfit-complete":
                continue
            dest_name = "HWP_AutoDocFit.exe" if item.name == "app_main.exe" else item.name
            dest = bundle_dir / dest_name
            if item.is_dir():
                shutil.copytree(item, dest, dirs_exist_ok=True)
            else:
                shutil.copy2(item, dest)

    return bundle_dir


def run_benchmark(output_report: Path | None = None) -> dict:
    version = "2.2.48"
    bundle = ensure_v2248_bundle()

    bench_dir = ROOT / "dist" / "bench_real"
    bench_dir.mkdir(parents=True, exist_ok=True)

    # 1. Ensure Rust launcher
    rust_exe = bench_dir / "rust" / "HWP_AutoDocFit_Rust.exe"
    print("[1/3] Ensuring Rust launcher...")
    build(bundle, rust_exe, version, launcher="rust")

    # 2. Locate C# release launcher
    release_exe = ROOT / "release-assets" / "HWP_AutoDocFit_v2.2.48_Portable.exe"
    if not release_exe.is_file():
        raise FileNotFoundError(f"Missing release asset: {release_exe}")

    results = {}
    targets = [
        ("csharp", release_exe),
        ("rust", rust_exe),
    ]

    with tempfile.TemporaryDirectory(prefix="docfit-realapp-bench-") as temporary:
        root = Path(temporary)

        for name, exe in targets:
            print(f"\n[{name}] Measuring Cold launch (on-demand unpack)...")
            cold_dir = root / name / "cold"
            cold_res = measure_single_launch(
                exe, cold_dir, version, target_window_title="한글편집 후처리", timeout=45.0,
            )
            print(f"  -> Cold First window: {cold_res['first_window_ms']} ms ({cold_res['first_window_title']})")
            print(f"  -> Cold Main window: {cold_res['main_window_ms']} ms, Launcher exit: {cold_res['launcher_exit_ms']} ms, ok: {cold_res['ok']}")

            # Settle and cleanup
            time.sleep(1.0)

            print(f"[{name}] Measuring Warm launch (cached)...")
            warm_dir = root / name / "warm"
            warm_dir.mkdir(parents=True, exist_ok=True)
            if (cold_dir / "cache").is_dir():
                shutil.copytree(cold_dir / "cache", warm_dir / "cache")
            if (cold_dir / "local").is_dir():
                shutil.copytree(cold_dir / "local", warm_dir / "local")

            warm_res = measure_single_launch(
                exe, warm_dir, version, target_window_title="한글편집 후처리", timeout=25.0,
            )
            print(f"  -> Warm First window: {warm_res['first_window_ms']} ms ({warm_res['first_window_title']})")
            print(f"  -> Warm Main window: {warm_res['main_window_ms']} ms, Launcher exit: {warm_res['launcher_exit_ms']} ms, ok: {warm_res['ok']}")

            # Settle
            time.sleep(1.0)

            results[name] = {
                "cold": cold_res,
                "warm": warm_res,
            }

    if output_report:
        output_report.parent.mkdir(parents=True, exist_ok=True)
        output_report.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print("\n=== Benchmark Summary ===")
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, default=ROOT / "dist" / "RL1F.initial.bench.json")
    args = parser.parse_args()
    run_benchmark(args.report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
