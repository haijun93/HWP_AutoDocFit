"""Bounded, externally observed one-document HWP COM reproduction for HANG1."""
from __future__ import annotations

import argparse
import csv
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from datetime import datetime, timezone

REPO = Path(__file__).resolve().parents[1]
if str(REPO) not in sys.path:
    sys.path.insert(0, str(REPO))
from docfit_core.save_guard import DEFAULT_COM_LOCK_PATH, com_test_lock

LOCK = DEFAULT_COM_LOCK_PATH


def validate_timeout(seconds: int) -> int:
    seconds = int(seconds)
    if not 1 <= seconds <= 900:
        raise ValueError("타임아웃은 1~900초여야 합니다.")
    return seconds


def unfinished_stage(records: list[dict]) -> str | None:
    active = []
    for record in records:
        if record.get("kind") == "stage_start":
            active.append(record.get("stage"))
        elif record.get("kind") in {"stage_end", "stage_error"}:
            stage = record.get("stage")
            for index in range(len(active) - 1, -1, -1):
                if active[index] == stage:
                    active.pop(index)
                    break
    return active[-1] if active else None


def taskkill_command(pid: int, *, tree: bool) -> list[str]:
    command = ["taskkill", "/PID", str(pid)]
    if tree:
        command.append("/T")
    command.append("/F")
    return command


def hwp_pids() -> set[int]:
    try:
        result = subprocess.run(
            ["tasklist", "/FO", "CSV", "/NH", "/FI", "IMAGENAME eq Hwp.exe"],
            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return set()
    pids = set()
    for row in csv.reader(result.stdout.splitlines()):
        if len(row) >= 2 and row[0].casefold() == "hwp.exe":
            try:
                pids.add(int(row[1].replace(",", "")))
            except ValueError:
                pass
    return pids


def windows() -> list[dict]:
    if os.name != "nt":
        return []
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    enum_proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.IsWindowVisible.argtypes = [wintypes.HWND]
    user32.IsWindowVisible.restype = wintypes.BOOL
    user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
    user32.GetWindowTextLengthW.restype = ctypes.c_int
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.GetWindowTextW.restype = ctypes.c_int
    user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.GetClassNameW.restype = ctypes.c_int
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetWindowThreadProcessId.restype = wintypes.DWORD
    user32.EnumWindows.argtypes = [enum_proc, wintypes.LPARAM]
    user32.EnumWindows.restype = wintypes.BOOL
    records = []

    @enum_proc
    def visit(hwnd, _):
        if not user32.IsWindowVisible(hwnd):
            return True
        title_length = user32.GetWindowTextLengthW(hwnd)
        title = ctypes.create_unicode_buffer(title_length + 1)
        user32.GetWindowTextW(hwnd, title, len(title))
        class_name = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(hwnd, class_name, len(class_name))
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if title.value or class_name.value == "#32770":
            records.append({"pid": pid.value, "title": title.value,
                            "class": class_name.value,
                            "dialog_class": class_name.value == "#32770"})
        return True

    user32.EnumWindows(visit, 0)
    return records


def kill_pid(pid: int, *, tree: bool) -> None:
    subprocess.run(taskkill_command(pid, tree=tree), capture_output=True,
                   text=True, timeout=15)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--docs-dir", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=900,
                        help="문서당 타임아웃 초(최대 900초)")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows + 한/글 COM 환경이 필요합니다.")
    try:
        validate_timeout(args.timeout)
    except ValueError as error:
        parser.error(str(error))
    source = args.source.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    args.docs_dir.mkdir(parents=True, exist_ok=True)
    copy_path = args.docs_dir / source.name
    if not copy_path.exists():
        import shutil
        shutil.copy2(source, copy_path)
    import hashlib
    sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    original_hash = sha(source)
    copy_hash = sha(copy_path)
    if copy_hash != original_hash:
        raise RuntimeError("입력 사본 해시가 원본과 다릅니다.")

    trace_path = args.out / "trace.jsonl"
    console_path = args.out / "worker.log"
    monitor_path = args.out / "windows.jsonl"
    report_path = args.out / "result.json"
    env = dict(os.environ, DOCFIT_COM_PERF="1", PYTHONUTF8="1",
               DOCFIT_COM_LOCK_PATH=str(LOCK.resolve()))
    command = [sys.executable, str(REPO / "scripts" / "smoke_engine_pc.py"),
               str(copy_path), "--mode", "all", "--out", str(args.out / "engine"),
               "--trace-jsonl", str(trace_path)]
    started = datetime.now(timezone.utc).isoformat()
    before_pids = set()
    timed_out = False
    returncode = None
    last_windows = []
    with com_test_lock("HANG1", f"재현 {source.name}; timeout={args.timeout}s", lock_path=LOCK):
        before_pids = hwp_pids()
        with console_path.open("w", encoding="utf-8") as log:
            process = subprocess.Popen(command, cwd=REPO, env=env,
                                       stdout=log, stderr=subprocess.STDOUT)
            deadline = time.monotonic() + args.timeout
            while process.poll() is None and time.monotonic() < deadline:
                current_pids = hwp_pids()
                all_windows = windows()
                owned = [row for row in all_windows if row["pid"] in current_pids]
                last_windows = owned
                with monitor_path.open("a", encoding="utf-8") as monitor:
                    monitor.write(json.dumps({
                        "time": datetime.now(timezone.utc).isoformat(),
                        "worker_pid": process.pid,
                        "hwp_pids": sorted(current_pids),
                        "new_hwp_pids": sorted(current_pids - before_pids),
                        "windows": owned,
                    }, ensure_ascii=False) + "\n")
                time.sleep(min(2, max(0, deadline - time.monotonic())))
            if process.poll() is None:
                timed_out = True
                try:
                    kill_pid(process.pid, tree=True)
                except (OSError, subprocess.TimeoutExpired):
                    process.kill()
                process.wait(timeout=20)
                for pid in sorted(hwp_pids() - before_pids):
                    try:
                        kill_pid(pid, tree=False)
                    except (OSError, subprocess.TimeoutExpired):
                        pass
            returncode = process.returncode
    records = []
    if trace_path.is_file():
        records = [json.loads(line) for line in trace_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    last_stage = unfinished_stage(records)
    snapshots = []
    if monitor_path.is_file():
        snapshots = [json.loads(line) for line in monitor_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    dialogs = [window for snapshot in snapshots for window in snapshot.get("windows", [])
               if window.get("dialog_class")]
    result = {
        "source": source.name, "source_sha256_before": original_hash,
        "source_sha256_after": sha(source), "copy_sha256_before": copy_hash,
        "copy_sha256_after": sha(copy_path), "started_at": started,
        "timeout_seconds": args.timeout, "timed_out": timed_out,
        "worker_returncode": returncode, "last_unfinished_stage": last_stage,
        "last_windows": last_windows, "dialog_windows_seen": dialogs,
        "trace_file": str(trace_path), "windows_file": str(monitor_path),
        "worker_log": str(console_path),
    }
    report_path.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 124 if timed_out else int(bool(returncode))


if __name__ == "__main__":
    raise SystemExit(main())
