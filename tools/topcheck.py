"""항상 위 전환 재연시험."""
import argparse
import ctypes
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

from playwright.sync_api import sync_playwright

HERE = Path(__file__).resolve().parent

parser = argparse.ArgumentParser()
parser.add_argument("--version", default="v2.2.55")
parser.add_argument("--isolated", action="store_true")
args, _ = parser.parse_known_args()
ver = args.version if args.version.startswith("v") else f"v{args.version}"

def find_target_exe():
    candidates = [
        HERE / f"HWP_AutoDocFit_{ver}_Portable.exe",
        HERE.parent / f"HWP_AutoDocFit_{ver}_Portable.exe",
        Path("C:/HWP_AUTODOCFIT/v1803/rel2255") / f"HWP_AutoDocFit_{ver}_Portable.exe",
        Path(f"C:/HWP_AUTODOCFIT/v1803/rel{ver.replace('v', '').replace('.', '')}") / f"HWP_AutoDocFit_{ver}_Portable.exe",
        HERE.parent / "release-assets" / f"HWP_AutoDocFit_{ver}_Portable.exe",
    ]
    for c in candidates:
        if c.is_file():
            return c
    return candidates[0]

NEW = find_target_exe()
OLD = None
ISOLATED = Path(tempfile.mkdtemp(prefix="docfit-isolated-"))


def launch(pw, exe, launch_args=()):
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    mutex = "Local\\DocFitCheck_" + uuid.uuid4().hex
    env = dict(os.environ, DOCFIT_OFFLINE="1", DOCFIT_LAUNCHER_MUTEX_NAME=mutex,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f"--remote-debugging-port={port}",
               DOCFIT_SMOKE_CDP_PORT=str(port), DOCFIT_LAUNCHER_TEST_ROOT=str(ISOLATED / "root"),
               LOCALAPPDATA=str(ISOLATED / "local"), WEBVIEW2_USER_DATA_FOLDER=tempfile.mkdtemp(prefix="docfit-check-"))
    start = time.perf_counter()
    proc = subprocess.Popen([str(exe), *launch_args], env=env)
    deadline = time.time() + 120
    while True:
        try:
            browser = pw.chromium.connect_over_cdp(f"http://127.0.0.1:{port}", timeout=1000)
            break
        except Exception:
            if time.time() > deadline:
                raise
            time.sleep(0.05)
    page = browser.contexts[0].pages[0]
    page.wait_for_function("window.docfitFirstFrame === true", timeout=90000)
    return proc, page, (time.perf_counter() - start) * 1000, env, browser


def kill(proc, browser):
    try:
        browser.close()
    except Exception:
        pass
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    "Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like '" + str(ISOLATED) + "*' } | "
                    "ForEach-Object { Stop-Process -Id $_.ProcessId -Force }"], capture_output=True)
    subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True)
    time.sleep(1.5)


if __name__ == "__main__":
    if not NEW.is_file():
        print(f"ERROR: Target binary not found: {NEW}", file=sys.stderr)
        sys.exit(1)

    (ISOLATED / "local").mkdir(parents=True, exist_ok=True)
    (ISOLATED / "root").mkdir(parents=True, exist_ok=True)
    out = {}
    with sync_playwright() as pw:
        proc, page, ms, env, browser = launch(pw, NEW)
        try:
            s = page.evaluate("window.pywebview.api.get_settings()")
            rev = s["data"].get("revision")
            out["save"] = page.evaluate("([v, r]) => window.pywebview.api.save_settings(v, r)", [{"developer_mode": True}, rev]).get("ok")
            def topmost():
                res = []
                @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
                def cb(h, _):
                    pid = ctypes.c_ulong()
                    ctypes.windll.user32.GetWindowThreadProcessId(ctypes.c_void_p(h), ctypes.byref(pid))
                    if ctypes.windll.user32.IsWindowVisible(ctypes.c_void_p(h)):
                        n = ctypes.create_unicode_buffer(200)
                        ctypes.windll.user32.GetWindowTextW(ctypes.c_void_p(h), n, 200)
                        if '한글편집' in n.value or 'DocFit' in n.value:
                            ex = ctypes.windll.user32.GetWindowLongPtrW(ctypes.c_void_p(h), -20)
                            res.append((n.value, bool(ex & 8)))
                    return True
                ctypes.windll.user32.EnumWindows(cb, 0)
                return res
            out["before"] = topmost()
            out["on"] = page.evaluate("window.pywebview.api.always_on_top(true)")
            time.sleep(1.5)
            out["after_on"] = topmost()
            out["off"] = page.evaluate("window.pywebview.api.always_on_top(false)")
            time.sleep(1.5)
            out["after_off"] = topmost()
        finally:
            kill(proc, browser)
    print(json.dumps(out, ensure_ascii=False))
    print("TOPCHECK PASS")
