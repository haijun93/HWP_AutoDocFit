"""kfind 색인 서비스 실행 관리(PKG1, Q15·Q16).

앱이 시작될 때 kfind 파이프가 없으면 사용자 권한 모드(``kfind.exe --user``)로
배경에서 띄운다. 관리자 서비스 설치·제거는 설정에서 따로 부른다(관리자 권한 요청).

- 대체 방식 기록은 kfind.exe와 같은 파일(``%LOCALAPPDATA%\\HwpAutoDocFit\\kfind\\settings.json``)
  ``{"fallback": "user"|"task"}``을 쓴다. Rust 쪽은 이 두 값만 읽으므로 다른 값을 쓰지 않는다.
- 기록이 없으면 사용자 모드로 시작한다(한 번 묻고 기억하는 화면은 설정 쪽 후속).
- 검색 엔진 설정이 ``legacy``면 kfind를 띄우지 않는다.
- 파이프 확인은 ``WaitNamedPipeW``로 한다. 파이프를 열지 않으므로 서비스의
  "연결 하나에 요청 하나" 인스턴스를 빈 연결로 소모하지 않는다.
"""
from __future__ import annotations

import json
import logging
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

logger = logging.getLogger(__name__)

PIPE_NAME = r"\\.\pipe\HwpAutoDocFit.Index"
SERVICE_NAME = "HwpAutoDocFit.Index"
FALLBACK_MODES = ("user", "task")  # crates/kfind/src/install.rs Fallback(snake_case)와 같게
_USER_PROCESS = None
_PROCESS_LOCK = threading.Lock()

_ERROR_FILE_NOT_FOUND = 2
_ERROR_SEM_TIMEOUT = 121


def data_dir() -> Path:
    override = os.environ.get("KFIND_DATA")
    if override:
        return Path(override)
    local = os.environ.get("LOCALAPPDATA")
    if local:
        return Path(local) / "HwpAutoDocFit" / "kfind"
    return Path.home() / "AppData" / "Local" / "HwpAutoDocFit" / "kfind"


def settings_path() -> Path:
    return data_dir() / "settings.json"


def get_fallback_mode() -> str | None:
    """저장된 대체 방식("user"·"task"). 기록이 없거나 읽을 수 없으면 None."""
    try:
        data = json.loads(settings_path().read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    mode = data.get("fallback") if isinstance(data, dict) else None
    return mode if mode in FALLBACK_MODES else None


def set_fallback_mode(mode: str) -> None:
    if mode not in FALLBACK_MODES:
        raise ValueError(f"지원하지 않는 대체 방식: {mode}")
    path = settings_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            data = {}
    except (OSError, ValueError):
        data = {}
    data["fallback"] = mode
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    os.replace(tmp, path)


def search_engine_setting() -> str:
    """앱 설정의 검색 엔진("kfind"·"legacy"). 읽을 수 없으면 기본값."""
    from docfit_app.settings_schema import DEFAULTS, normalize_search_engine
    appdata = os.environ.get("APPDATA")
    value = DEFAULTS.get("search_engine")
    if appdata:
        try:
            from docfit_app.settings_service import SettingsService
            value = SettingsService(Path(appdata) / "HwpAutoDocFit" / "settings.json").read().get(
                "search_engine", value)
        except Exception:  # 설정 파일 손상은 기본값으로 진행(설정 화면이 따로 알린다)
            pass
    return normalize_search_engine(value)


def find_kfind_exe() -> Path | None:
    """kfind.exe 위치(설치 꾸러미 → 개발 트리 순)."""
    candidates = []
    if getattr(sys, "frozen", False):
        meipass = getattr(sys, "_MEIPASS", None)
        if meipass:
            candidates.append(Path(meipass) / "resources" / "kfind.exe")
        base = Path(sys.executable).parent
        candidates.append(base / "_internal" / "resources" / "kfind.exe")
        candidates.append(base / "resources" / "kfind.exe")
    root = Path(__file__).resolve().parent.parent
    candidates.append(root / "resources" / "kfind.exe")
    candidates.append(root / "crates" / "kfind" / "target" / "release" / "kfind.exe")
    for path in candidates:
        if path.is_file():
            return path
    return None


def is_pipe_alive(pipe_name: str = PIPE_NAME, timeout: float = 0.2) -> bool:
    """파이프 서버가 있는지 확인한다(연결하지 않음). 인스턴스가 바쁘면 있는 것으로 본다."""
    if os.name != "nt":
        return False
    import ctypes
    from ctypes import wintypes
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    wait = k32.WaitNamedPipeW
    wait.argtypes = [wintypes.LPCWSTR, wintypes.DWORD]
    wait.restype = wintypes.BOOL
    deadline = time.monotonic() + max(0.0, timeout)
    while True:
        if wait(pipe_name, 50):
            return True
        error = ctypes.get_last_error()
        if error == _ERROR_SEM_TIMEOUT:
            return True  # 서버는 있고 인스턴스가 모두 쓰이는 중
        if error != _ERROR_FILE_NOT_FOUND or time.monotonic() >= deadline:
            return False
        time.sleep(0.02)


def _creation_flags() -> int:
    if os.name != "nt":
        return 0
    return (getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000)
            | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0x00000200))


def start_user_mode(exe: Path | str | None = None,
                    pipe_name: str | None = None,
                    index_root: Path | str | None = None) -> subprocess.Popen:
    """kfind.exe를 사용자 권한 모드로 배경 실행한다. 앱이 끝나도 계속 돈다."""
    global _USER_PROCESS
    with _PROCESS_LOCK:
        if _USER_PROCESS is not None and _USER_PROCESS.poll() is None:
            return _USER_PROCESS
        exe_path = Path(exe) if exe else find_kfind_exe()
        if not exe_path or not exe_path.is_file():
            raise FileNotFoundError(f"kfind.exe를 찾을 수 없습니다: {exe_path}")
        args = [str(exe_path), "--user"]
        if pipe_name:
            args += ["--pipe-name", str(pipe_name)]
        if index_root:
            args += ["--index-root", str(index_root)]
        _USER_PROCESS = subprocess.Popen(
            args, creationflags=_creation_flags(), close_fds=True, cwd=str(exe_path.parent),
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return _USER_PROCESS


def stop_user_mode() -> None:
    """이 앱이 띄운 사용자 모드 kfind를 끝낸다."""
    global _USER_PROCESS
    with _PROCESS_LOCK:
        if _USER_PROCESS is not None and _USER_PROCESS.poll() is None:
            try:
                _USER_PROCESS.terminate()
            except OSError:
                pass
        _USER_PROCESS = None


def ensure_kfind_running(timeout: float = 3.0, *, engine_provider=None) -> bool:
    """kfind 파이프가 없으면 사용자 권한 모드로 띄우고 파이프가 생길 때까지 기다린다.

    실패해도 예외를 내지 않는다. 검색은 kfind_client 쪽 legacy 대체가 맡는다.
    """
    try:
        engine = (engine_provider or search_engine_setting)()
        if engine == "legacy":
            return False
        if is_pipe_alive(timeout=0.1):
            return True
        mode = get_fallback_mode() or "user"
        if mode == "task":
            # 예약 작업은 로그온 때 kfind가 스스로 뜬다. 지금 없으면 이번만 사용자 모드로.
            logger.info("kfind 예약 작업 방식이지만 파이프가 없어 사용자 모드로 띄웁니다.")
        exe = find_kfind_exe()
        if exe is None:
            logger.warning("kfind.exe를 찾지 못해 색인 서비스를 띄우지 않습니다.")
            return False
        start_user_mode(exe)
        deadline = time.monotonic() + max(0.5, timeout)
        while time.monotonic() < deadline:
            if is_pipe_alive(timeout=0.05):
                return True
            if _USER_PROCESS is not None and _USER_PROCESS.poll() is not None:
                logger.warning("kfind 사용자 모드가 바로 끝났습니다(종료 코드 %s).", _USER_PROCESS.returncode)
                return False
            time.sleep(0.05)
        return is_pipe_alive(timeout=0.1)
    except Exception as exc:  # 앱 시작을 막지 않는다
        logger.warning("kfind 실행 확인 실패: %s", exc)
        return False


def _run_elevated(exe: Path, argument: str, timeout: float) -> tuple[bool, str]:
    """관리자 권한 요청(UAC)으로 kfind.exe를 실행하고 끝날 때까지 기다린다."""
    script = ("$p = Start-Process -FilePath $env:KFIND_EXE -ArgumentList $env:KFIND_ARG "
              "-Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode")
    env = dict(os.environ, KFIND_EXE=str(exe), KFIND_ARG=argument)
    try:
        res = subprocess.run(
            ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
            env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True,
            timeout=timeout, creationflags=_creation_flags())
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, str(exc)
    message = (res.stdout or res.stderr or "").strip()
    return res.returncode == 0, message


def _stop_own_user_processes() -> None:
    """현재 사용자 계정의 kfind.exe만 끝낸다(서비스 계정 프로세스는 건드리지 않음)."""
    user = os.environ.get("USERNAME")
    if os.name != "nt" or not user:
        return
    try:
        subprocess.run(["taskkill.exe", "/F", "/IM", "kfind.exe", "/FI", f"USERNAME eq {user}"],
                       stdin=subprocess.DEVNULL, capture_output=True, timeout=15,
                       creationflags=_creation_flags())
    except (OSError, subprocess.TimeoutExpired):
        pass
    time.sleep(0.3)


def install_service(exe: Path | str | None = None, timeout: float = 120.0) -> tuple[bool, str]:
    """관리자 서비스 설치(설정에서 부름). 성공하면 사용자 모드 kfind는 끝낸다."""
    exe_path = Path(exe) if exe else find_kfind_exe()
    if not exe_path:
        return False, "kfind.exe를 찾을 수 없습니다."
    ok, message = _run_elevated(exe_path, "--install", timeout)
    if ok:
        stop_user_mode()
        _stop_own_user_processes()  # 지난 실행이 띄운 사용자 모드도 파이프 이름을 비켜 준다
        ok, start_message = _run_elevated(exe_path, "--start", timeout)
        message = "\n".join(m for m in (message, start_message) if m)
    return ok, message


def uninstall_service(exe: Path | str | None = None, timeout: float = 120.0) -> tuple[bool, str]:
    """관리자 서비스 제거와 공개 색인 삭제(Q16, 설정에서 부름)."""
    exe_path = Path(exe) if exe else find_kfind_exe()
    if not exe_path:
        return False, "kfind.exe를 찾을 수 없습니다."
    return _run_elevated(exe_path, "--uninstall", timeout)
