"""KHot isolated smoke test. Never writes clipboard or sends input; quit via private pipe."""
import csv
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path("E:/docfit-tmp/khot-root")
root.mkdir(parents=True, exist_ok=True)
if (root / "status.json").exists():
    raise SystemExit("Existing isolated status.json: inspect it before running")
(root / "settings.json").write_text(json.dumps({
    "hotkey": "Ctrl+Shift+V", "paused": True,
    "watch_screenshots": False, "timeline_roots": [],
}), encoding="utf-8")
proc = subprocess.Popen([str(Path(sys.argv[1]).resolve())],
                        env=dict(os.environ, KAGENT_TEST_ROOT=str(root)))
sid = next(csv.reader([subprocess.check_output(
    ["whoami", "/user", "/fo", "csv", "/nh"], text=True).strip()]))[1]
root_hash = 0xcbf29ce484222325
units = str(root).encode("utf-16-le")
for offset in range(0, len(units), 2):
    root_hash = ((root_hash ^ int.from_bytes(units[offset:offset+2], "little"))
                 * 0x100000001b3) & ((1 << 64) - 1)
pipe = rf"\\.\pipe\HwpAutoDocFit.kagent.test.{root_hash:016x}.{sid}"
checks = 0

def request(cmd, **extra):
    for attempt in range(40):
        try:
            with open(pipe, "r+b", buffering=0) as stream:
                stream.write((json.dumps(dict(cmd=cmd, **extra)) + "\n").encode())
                data = b""
                while not data.endswith(b"\n"):
                    chunk = stream.read(65536)
                    if not chunk:
                        raise RuntimeError("Pipe closed without response")
                    data += chunk
                return json.loads(data)
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("Private pipe unavailable")

def check(condition, label):
    global checks
    assert condition, label
    checks += 1
    print("PASS", label, flush=True)

try:
    status = request("status")
    check(".test." in pipe and proc.poll() is None, "private pipe and live process")
    check(status["hotkey_ok"] is False, "startup conflict")
    for hotkey in ["Ctrl+Shift+V", "V", "Ctrl+Space", "Ctrl+Alt+Shift+F24", "Ctrl+Shift+V"]:
        expected = hotkey == "Ctrl+Alt+Shift+F24"
        result = request("settings_set", settings={"hotkey": hotkey})
        check(result["ok"] and result["hotkey_ok"] is expected
              and result["settings"]["hotkey"] == hotkey, "settings_set immediate " + hotkey)
        result = request("settings_get")
        check(result["ok"] and result["hotkey_ok"] is expected
              and result["settings"]["hotkey"] == hotkey, "settings_get " + hotkey)
        result = request("status")
        check(result["hotkey_ok"] is expected and result["hotkey"] == hotkey, "status " + hotkey)
finally:
    if pipe:
        print("quit:", request("quit"), flush=True)
    code = proc.wait(timeout=10)
    check(code == 0, "isolated process exits through quit")
print(f"{checks} checks passed")
