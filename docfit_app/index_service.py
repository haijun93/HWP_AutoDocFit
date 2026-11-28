"""전체 드라이브 파일명 색인 서비스(`HwpAutoDocFit.Index`)를 설정에서 켜고 끄는 제어부.

서비스는 관리자(UAC) 승인으로 한 번 설치·시작한다. 서비스가 켜지면 고정 드라이브 전체 파일명 색인을 스스로 만들어
퀵몽땅검색이 Everything처럼 전체 드라이브를 찾는다. 서비스가 없으면 사용자 모드 kfind가 바탕 화면·문서·다운로드만 색인한다.
설치·제거는 UAC 승인과 시간이 걸리므로 백그라운드 스레드에서 돌리고, 화면은 `status`로 진행을 읽는다.
"""
from __future__ import annotations

import os
import re
import subprocess
import threading

from docfit_app import kfind_process as kp

ACTIONS = ("status", "install", "uninstall")


def _query_service_state() -> str | None:
    """`sc query`로 서비스 상태를 읽는다: RUNNING·STOPPED·…, 서비스가 없으면 None."""
    if os.name != "nt":
        return None
    try:
        result = subprocess.run(["sc.exe", "query", kp.SERVICE_NAME], stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=10,
                                creationflags=kp._creation_flags())
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0:
        return None
    match = re.search(r"STATE\s*:\s*\d+\s+([A-Z_]+)", result.stdout or "")
    return match.group(1) if match else None


class IndexServiceController:
    def __init__(self, *, query_state=None, install=None, uninstall=None, pipe_alive=None):
        self._query_state = query_state or _query_service_state
        self._install = install or kp.install_service
        self._uninstall = uninstall or kp.uninstall_service
        self._pipe_alive = pipe_alive or kp.is_pipe_alive
        self._lock = threading.Lock()
        self._job = None  # "install" | "uninstall" | None
        self._message = ""
        self._ok = None

    def status(self):
        state = self._query_state()
        with self._lock:
            job, message, ok = self._job, self._message, self._ok
        return {"ok": True, "supported": os.name == "nt" or self._query_state is not _query_service_state,
                "installed": state is not None, "running": state == "RUNNING", "service_state": state,
                "pipe_alive": bool(self._pipe_alive()), "busy": job, "message": message, "last_ok": ok}

    def _start(self, job, func):
        with self._lock:
            if self._job:
                return {"ok": False, "code": "BUSY", "message": "이미 다른 작업을 진행 중입니다. 잠시 뒤 다시 시도해 주세요."}
            self._job, self._message, self._ok = job, "관리자 승인 창에서 '예'를 눌러 주세요.", None

        def run():
            try:
                ok, message = func()
            except Exception as exc:  # 설치 실패가 앱을 멈추지 않게 한다.
                ok, message = False, f"{type(exc).__name__}: {exc}"
            with self._lock:
                self._ok = bool(ok)
                self._message = message or ("완료했습니다." if ok else "완료하지 못했습니다. 관리자 승인을 거절했거나 정책으로 막혔을 수 있습니다.")
                self._job = None

        threading.Thread(target=run, name=f"index-service-{job}", daemon=True).start()
        return self.status()

    def command(self, action, payload=None):
        if action not in ACTIONS:
            return {"ok": False, "code": "INVALID_INPUT", "message": "지원하지 않는 명령입니다."}
        if action == "status":
            return self.status()
        if os.environ.get("DOCFIT_FORBID_REAL_DATA") == "1":
            return {"ok": False, "code": "FORBIDDEN", "message": "시험 중에는 서비스를 설치하거나 제거하지 않습니다."}
        if action == "install":
            return self._start("install", lambda: self._install())
        return self._start("uninstall", lambda: self._uninstall())
