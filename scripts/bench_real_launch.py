"""Benchmark actual GUI launch path: launcher -> app window display (no --self-test)."""
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

from scripts.build_smart_launcher import build, compile_launcher, find_csc, make_payload

user32 = ctypes.windll.user32


def find_visible_window(substring: str) -> dict | None:
    found = None

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def callback(hwnd, _lparam):
        nonlocal found
        if not user32.IsWindowVisible(hwnd):
            return True
        length = user32.GetWindowTextLengthW(hwnd)
        if length > 0:
            buf = ctypes.create_unicode_buffer(length + 1)
            user32.GetWindowTextW(hwnd, buf, length + 1)
            title = buf.value
            if substring in title:
                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                found = {"hwnd": hwnd, "title": title, "pid": pid.value}
                return False
        return True

    user32.EnumWindows(callback, 0)
    return found


def kill_pids(pids: set[int]):
    for pid in list(pids):
        if pid:
            subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True, check=False)


def measure_single_launch(
    launcher_exe: Path,
    manifest_path: Path,
    payload_path: Path,
    isolated_dir: Path,
    version: str,
    timeout: float = 15.0,
) -> dict:
    local_app_data = isolated_dir / "local"
    roaming_app_data = isolated_dir / "roaming"
    test_root = isolated_dir / "cache"
    ready_file = isolated_dir / "app_ready.json"

    local_app_data.mkdir(parents=True, exist_ok=True)
    roaming_app_data.mkdir(parents=True, exist_ok=True)
    test_root.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["LOCALAPPDATA"] = str(local_app_data)
    env["APPDATA"] = str(roaming_app_data)
    env["DOCFIT_LAUNCHER_TEST_ROOT"] = str(test_root)
    env["DOCFIT_LAUNCHER_MANIFEST"] = str(manifest_path)
    env["DOCFIT_LAUNCHER_PAYLOAD"] = str(payload_path)
    env["DOCFIT_LAUNCHER_PATH"] = str(launcher_exe)
    env["DOCFIT_TEST_VERSION"] = version
    env["DOCFIT_READY_FILE"] = str(ready_file)
    env["HWP_LAUNCHER_TRACE"] = "1"

    pids_to_kill = set()
    started = time.perf_counter()
    window_ready_ms = None
    launcher_exit_ms = None
    launcher_exit_code = None
    shown_file_ms = None
    window_info = None

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
        # 1. Check window visibility
        if window_ready_ms is None:
            win = find_visible_window(version)
            if win:
                window_ready_ms = round((now - started) * 1000, 2)
                window_info = win
                pids_to_kill.add(win["pid"])

        # 2. Check ready file
        if shown_file_ms is None and ready_file.is_file():
            shown_file_ms = round((now - started) * 1000, 2)

        # 3. Check launcher process exit
        if launcher_exit_ms is None:
            ret = proc.poll()
            if ret is not None:
                launcher_exit_ms = round((now - started) * 1000, 2)
                launcher_exit_code = ret

        if window_ready_ms is not None and launcher_exit_ms is not None:
            break
        time.sleep(0.01)

    # Clean up launcher and child processes
    kill_pids(pids_to_kill)
    try:
        _, stderr = proc.communicate(timeout=2)
    except Exception:
        stderr = ""

    trace_lines = []
    if stderr:
        trace_lines = [l for l in stderr.splitlines() if "HWP_LAUNCHER_TRACE" in l]

    return {
        "ok": window_ready_ms is not None,
        "window_ready_ms": window_ready_ms,
        "shown_file_ms": shown_file_ms,
        "launcher_exit_ms": launcher_exit_ms,
        "launcher_exit_code": launcher_exit_code,
        "window_info": window_info,
        "trace": trace_lines,
    }


def run_benchmark(output_report: Path | None = None) -> dict:
    child_source = ROOT / "tests" / "fixtures" / "real_gui_child.cs"
    csc = find_csc()

    with tempfile.TemporaryDirectory(prefix="docfit-real-bench-") as temporary:
        root = Path(temporary)
        bundle = root / "bundle"
        bundle.mkdir()
        app = bundle / "HWP_AutoDocFit.exe"

        subprocess.run(
            [str(csc), "/nologo", "/target:winexe",
             "/reference:System.Windows.Forms.dll,System.Drawing.dll",
             f"/out:{app}", str(child_source)],
            check=True,
        )
        (bundle / "asset.txt").write_text("real-bundle-fixture", encoding="utf-8")

        version = "9.99.1-rl1e"
        results = {}

        for kind in ("csharp", "rust"):
            out = root / kind
            out.mkdir()
            payload = out / "smart-launcher-build" / "payload.zip"
            _, manifest = make_payload(bundle, payload, version)

            if kind == "csharp":
                compile_launcher(
                    payload, manifest, out / "launcher.exe", version,
                    ROOT / "resources" / "app_icon.ico", ROOT / "resources" / "splash.png",
                )
                launcher_exe = out / "launcher.exe"
                launcher_manifest = manifest
                launcher_payload = payload
            else:
                build(bundle, out / "launcher.exe", version, "rust")
                launcher_exe = out / "launcher.exe"
                launcher_manifest = out / "launcher.manifest.txt"
                launcher_payload = out / "launcher.payload.zip"
                shutil.copy2(manifest, launcher_manifest)
                shutil.copy2(payload, launcher_payload)

            print(f"[{kind}] Measuring cold launch...")
            cold_res = measure_single_launch(
                launcher_exe, launcher_manifest, launcher_payload,
                out / "cold", version, timeout=15.0,
            )

            print(f"[{kind}] Measuring warm launch (cached)...")
            warm_dir = out / "warm"
            warm_dir.mkdir(parents=True, exist_ok=True)
            if (out / "cold" / "cache").is_dir():
                shutil.copytree(out / "cold" / "cache", warm_dir / "cache")
            if (out / "cold" / "local").is_dir():
                shutil.copytree(out / "cold" / "local", warm_dir / "local")

            warm_res = measure_single_launch(
                launcher_exe, launcher_manifest, launcher_payload,
                warm_dir, version, timeout=15.0,
            )

            results[kind] = {
                "cold": cold_res,
                "warm": warm_res,
            }

        if output_report:
            output_report.parent.mkdir(parents=True, exist_ok=True)
            output_report.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

        print("\n=== Benchmark Results ===")
        print(json.dumps(results, ensure_ascii=False, indent=2))
        return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, default=ROOT / "dist" / "RL1E.bench.json")
    args = parser.parse_args()
    run_benchmark(args.report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
