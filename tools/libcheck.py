"""가상 폴더·업무자료함 생성·출처 추가·검색 재연시험."""
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
            out["save"] = page.evaluate("([v, r]) => window.pywebview.api.save_settings(v, r)", [{"developer_mode": True}, rev])
            out["dev"] = page.evaluate("window.pywebview.api.get_state()").get("developer_mode")
            out["create"] = page.evaluate("window.pywebview.api.library({cmd: 'box_create', name: '실행 확인 자료함'})")
            out["list"] = page.evaluate("window.pywebview.api.library({cmd: 'box_list'})")
            src = ISOLATED / "출처.txt"
            src.write_text("2026년 3분기 추진율은 87%입니다." + chr(10) + "예산 집행률은 64%로 집계되었습니다." + chr(10), encoding="utf-8")
            nb = page.evaluate("window.pywebview.api.notebook({cmd: 'nb_create', name: '실행 확인 노트북'})")
            out["nb_create"] = nb
            nid = nb.get("id") or nb.get("notebook_id")
            out["nb_source"] = page.evaluate("([n, p]) => window.pywebview.api.notebook({cmd: 'nb_source_add', notebook_id: n, source_path: p, display_name: '출처.txt'})", [nid, str(src)])
            out["nb_search"] = page.evaluate("(n) => window.pywebview.api.notebook({cmd: 'nb_search', notebook_id: n, query: '추진율'})", nid)
        finally:
            kill(proc, browser)
    print(json.dumps(out, ensure_ascii=False, default=str)[:1500])
    assert out["dev"] is True, "개발자 모드 켜기 실패"
    assert out["create"].get("ok") is True, out["create"]
    assert any(b.get("name") == "실행 확인 자료함" for b in out["list"].get("boxes", [])), out["list"]
    print("LIBRARY OK")
    assert out["nb_create"].get("ok") is True, out["nb_create"]
    assert out["nb_source"].get("ok") is True, out["nb_source"]
    assert "87%" in json.dumps(out["nb_search"], ensure_ascii=False), out["nb_search"]
    print("NOTEBOOK OK")
    print("LIBCHECK PASS")
