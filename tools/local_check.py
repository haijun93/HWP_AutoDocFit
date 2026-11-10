"""2.0.9·2.1.0 시작 시간 비교(번갈아 실행)와 2.1.0 실제 창 확인(왼쪽 메뉴, --rename 첫 실행·실행 중 전달)."""
import argparse
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
# 켜져 있는 사용자 앱과 겹치지 않게 실행 폴더·설정·요청 폴더를 모두 격리한다.
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
    # 격리 폴더에서 실행한 프로세스만 끈다(사용자 켜둔 app_main은 그대로).
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    "Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like '" + str(ISOLATED) + "*' } | "
                    "ForEach-Object { Stop-Process -Id $_.ProcessId -Force }"], capture_output=True)
    subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True)
    time.sleep(1.5)


if __name__ == "__main__":
    if not NEW.is_file():
        print(f"ERROR: Target binary not found: {NEW}", file=sys.stderr)
        sys.exit(1)
        
    result = {"times": {ver: []}}
    (ISOLATED / "local").mkdir(parents=True, exist_ok=True)
    (ISOLATED / "root").mkdir(parents=True, exist_ok=True)
    with sync_playwright() as pw:
        for exe in ():  # 캐시 채우기
            proc, page, ms, env, browser = launch(pw, exe)
            kill(proc, browser)
        for _ in range(0):
            for name, exe in ((ver, NEW),):
                proc, page, ms, env, browser = launch(pw, exe)
                result["times"][name].append(round(ms))
                print(name, round(ms), flush=True)
                kill(proc, browser)

        print("rename", flush=True)
        folder = Path(tempfile.mkdtemp(prefix="docfit-rename-"))
        first, second = folder / "첫째.txt", folder / "둘째.txt"
        first.write_text("a", encoding="utf-8")
        second.write_text("b", encoding="utf-8")
        proc, page, ms, env, browser = launch(pw, NEW, ["--rename", str(first)])
        page.wait_for_function('window.pywebview && typeof window.pywebview.api.file_rename === "function"', timeout=60000)
        page.wait_for_timeout(2500)
        page.screenshot(path=str(HERE / "rename-new.png"))
        names = lambda: [e.get("name") for e in page.evaluate("window.pywebview.api.file_rename('list', {})").get("entries", [])]  # noqa: E731
        result["rename_new_launch"] = names()
        subprocess.Popen([str(NEW), "--rename", str(second)], env=env)
        for _ in range(40):
            page.wait_for_timeout(1500)
            if len(names()) > 1:
                break
        result["rename_forwarded"] = names()
        page.screenshot(path=str(HERE / "rename-forward.png"))
        result["state_version"] = page.evaluate("window.pywebview.api.get_state()").get("version")
        kill(proc, browser)

    (HERE / "local_check.json").write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))
    print("LOCAL_CHECK PASS")
