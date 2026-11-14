"""몽땅검색 단축키 앱(ktm.exe) 연결: 상태·켜기·끄기·앱 시작 때 갱신.

ktm.exe는 Ctrl+Alt+Space로 뜨는 독립 상주 프로그램이다(기본 켬). 앱은 수명 명령만 보낸다.
- 켜기: ``--install`` (사용자 폴더로 복사·로그인 자동 실행·숨긴 채 시작, 끈 표시 지움)
- 끄기: ``--disable`` (끄고 자동 실행 해제, 끈 표시를 남겨 다음 갱신 때 다시 켜지 않음)
- 갱신: ``--install --respect-disabled`` (앱 시작 때, 설치본이 없거나 옛 버전이거나 꺼져 있을 때만)
결과는 표준 출력의 JSON 한 줄이다.
"""
from __future__ import annotations

import json
import logging
import subprocess
import sys
from pathlib import Path

_log = logging.getLogger(__name__)
SETTING_KEY = "mongttang_search"
_TIMEOUT = 20


def bundled_exe():
    """앱에 들어 있는 ktm.exe(배포판: resources, 개발: cargo 출력)."""
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1]))
    for candidate in (base / "resources" / "ktm.exe", base / "ktm" / "target" / "release" / "ktm.exe"):
        if candidate.is_file():
            return candidate
    return None


def status_text(status):
    """설정 화면에 보일 상태 한 줄."""
    if not isinstance(status, dict):
        return "상태를 알 수 없어요."
    if not status.get("ok", False):
        return f"오류: {status.get('error') or status.get('message') or '알 수 없음'}"
    if status.get("running"):
        if status.get("hotkey_error"):
            return f"실행 중이지만 단축키를 쓰지 못해요: {status['hotkey_error']}"
        tail = "" if status.get("autostart") else " (로그인 자동 실행 꺼짐)"
        return f"켜짐 · Ctrl+Alt+Space로 열기{tail}"
    if status.get("disabled"):
        return "꺼짐"
    if status.get("installed"):
        return "설치됨 · 실행 중 아님"
    return "설치 안 됨"


def _same_file(a, b):
    """설치본이 앱에 든 것과 같은 바이트인지(버전 번호가 그대로여도 새 빌드면 바꾼다)."""
    import filecmp
    try:
        return bool(b) and filecmp.cmp(str(a), str(b), shallow=False)
    except OSError:
        return False


class KtmService:
    def __init__(self, *, bundled=bundled_exe, runner=None):
        self._bundled = bundled
        self._runner = runner or subprocess.run

    def _run(self, exe, *args):
        if exe is None:
            return {"ok": False, "error": "몽땅검색 실행 파일이 앱에 없어요."}
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        try:
            done = self._runner([str(exe), *args], capture_output=True, timeout=_TIMEOUT,
                                creationflags=flags, stdin=subprocess.DEVNULL)
        except (OSError, subprocess.SubprocessError) as exc:
            return {"ok": False, "error": f"몽땅검색 실행 실패: {exc}"}
        line = (done.stdout or b"").decode("utf-8", "replace").strip().splitlines()
        try:
            result = json.loads(line[-1]) if line else None
        except ValueError:
            result = None
        if not isinstance(result, dict):
            return {"ok": False, "error": f"몽땅검색 응답을 읽지 못했어요(종료 코드 {done.returncode})."}
        return result

    def status(self):
        return self._run(self._bundled(), "--status")

    def enable(self):
        return self._run(self._bundled(), "--install")

    def disable(self):
        return self._run(self._bundled(), "--disable")

    def apply_setting(self, enabled):
        return self.enable() if enabled else self.disable()

    def sync(self, enabled=True):
        """앱 시작 때: 설정을 설치 상태에 맞춘다. 할 일이 없으면 아무것도 띄우지 않는다."""
        exe = self._bundled()
        if exe is None:
            return {"ok": True, "skipped": "no-bundled-exe"}
        status = self._run(exe, "--status")
        if not status.get("ok"):
            return status
        if not enabled:
            if status.get("running") or status.get("autostart") or not status.get("disabled"):
                return self._run(exe, "--disable")
            return {**status, "skipped": "already-off"}
        if status.get("disabled"):
            # 앱 설정은 켬인데 끈 표시가 남아 있음(예: 설정 파일 복원) → 설정을 따른다.
            return self._run(exe, "--install")
        if status.get("running") and _same_file(exe, status.get("installed_path")):
            return {**status, "skipped": "up-to-date"}
        return self._run(exe, "--install", "--respect-disabled")


def sync_in_background(enabled):
    import threading

    def work():
        try:
            result = KtmService().sync(enabled)
            if not result.get("skipped"):
                _log.info("몽땅검색 점검: %s", {k: result.get(k) for k in ("ok", "actions", "running", "error")})
        except Exception:
            _log.exception("몽땅검색 점검 실패")

    thread = threading.Thread(target=work, name="ktm-sync", daemon=True)
    thread.start()
    return thread
