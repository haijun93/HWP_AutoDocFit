"""독립 가상 폴더 창의 수명 명령 연결. 기본 꺼짐, kagent와 상태를 공유하지 않는다."""
from __future__ import annotations

import json
import subprocess
import sys
import threading
from pathlib import Path

_LOCK = threading.RLock()


def bundled_exe():
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1]))
    for path in (base / "resources/kvf.exe", base / "kvf/target/release/kvf.exe"):
        if path.is_file():
            return path
    return None


class KvfClient:
    def __init__(self, *, bundled=bundled_exe, runner=subprocess.run):
        self._bundled = bundled
        self._runner = runner

    def _request(self, command, *args):
        exe = self._bundled()
        if exe is None:
            if command in ("--status", "--upgrade-if-stale"):
                return dict(ok=True, available=False, running=False, autostart=False,
                            upgraded=False, status={})
            return dict(ok=False, message="가상 폴더 창 프로그램이 들어 있지 않습니다.")
        try:
            result = self._runner([str(exe), command, *args], capture_output=True,
                                  text=True, encoding="utf-8", timeout=60,
                                  creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            data = json.loads(result.stdout.strip().splitlines()[-1])
            if result.returncode or not isinstance(data, dict) or not isinstance(data.get("ok"), bool):
                raise ValueError("가상 폴더 창 응답을 확인하지 못했습니다.")
            if not data["ok"]:
                data.setdefault("message", str(data.get("reason") or "가상 폴더 창 명령에 실패했습니다."))
            return data
        except (OSError, ValueError, IndexError, subprocess.TimeoutExpired) as error:
            return dict(ok=False, message=str(error))

    def command(self, action, payload=None):
        payload = payload or {}
        if action not in ("status", "enable", "disable", "upgrade_if_stale"):
            return dict(ok=False, message="지원하지 않는 가상 폴더 창 명령입니다.")
        with _LOCK:
            command = {"status": "--status", "enable": "--enable",
                       "disable": "--disable", "upgrade_if_stale": "--upgrade-if-stale"}[action]
            args = ["--autostart"] if action == "enable" and payload.get("autostart", True) is True else []
            result = self._request(command, *args)
            if result.get("ok") and action in ("enable", "disable"):
                return self._request("--status")
            return result
