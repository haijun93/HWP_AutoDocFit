"""위계 엔진(hierarchy 실행 파일) 호출 브리지. 설정 hierarchy_engine이 켜졌을 때만 사용한다."""
import json
import os
from threading import Lock
import ctypes
import subprocess
import sys
from pathlib import Path

TIMEOUT_SECONDS = 5.0
_NAME = "hierarchy.exe" if sys.platform == "win32" else "hierarchy"
_DLL_NAME = "hierarchy.dll" if sys.platform == "win32" else "libhierarchy.so"


_LIBRARY_LOCK = Lock()
_LIBRARIES = {}
_SETTINGS_LOCK = Lock()
_SETTINGS_CACHE = None


def _load_library(library):
    key = os.path.normcase(str(Path(library).resolve()))
    with _LIBRARY_LOCK:
        if key not in _LIBRARIES:
            dll = ctypes.CDLL(key)
            dll.hierarchy_analyze_json.argtypes = [ctypes.c_char_p]
            dll.hierarchy_analyze_json.restype = ctypes.c_void_p
            dll.hierarchy_json_free.argtypes = [ctypes.c_void_p]
            dll.hierarchy_json_free.restype = None
            _LIBRARIES[key] = dll
        return _LIBRARIES[key]


def _read_settings():
    from docfit_app.settings_schema import DEFAULTS
    from docfit_app.settings_service import SettingsService
    global _SETTINGS_CACHE
    appdata = os.environ.get("APPDATA")
    if not appdata:
        return dict(DEFAULTS)
    path = Path(appdata) / "HwpAutoDocFit" / "settings.json"
    with _SETTINGS_LOCK:
        try:
            stat = path.stat()
            signature = (stat.st_mtime_ns, stat.st_size)
        except FileNotFoundError:
            signature = None
        key = (os.path.normcase(str(path.resolve())), signature)
        if _SETTINGS_CACHE is not None and _SETTINGS_CACHE[0] == key:
            return _SETTINGS_CACHE[1]
        settings = SettingsService(path).read()
        if "hierarchy_engine" not in settings:
            settings = {**DEFAULTS, **settings}
        # If replaced during the read, the old signature forces a reload next call.
        _SETTINGS_CACHE = (key, settings)
        return settings


def find_executable():
    repository = Path(__file__).resolve().parent.parent
    base = Path(getattr(sys, "_MEIPASS", repository))
    for candidate in (base / "resources" / _NAME,
                      base / "resources" / "hierarchy.exe",
                      base / _NAME,
                      repository / "hierarchy" / "target" / "release" / _NAME,
                      repository / "hierarchy" / "target" / "release" / "hierarchy.exe"):
        if candidate.is_file():
            return candidate
    return None


def find_library():
    repository = Path(__file__).resolve().parent.parent
    base = Path(getattr(sys, "_MEIPASS", repository))
    for candidate in (base / "resources" / _DLL_NAME,
                      repository / "hierarchy" / "target" / "release" / _DLL_NAME):
        if candidate.is_file():
            return candidate
    return None


def run_library(library, paragraphs):
    """Invoke the C ABI and return its native Python-result-shaped JSON."""
    try:
        dll = _load_library(library)
        payload = json.dumps({"paragraphs": paragraphs}, ensure_ascii=False).encode("utf-8")
        ptr = dll.hierarchy_analyze_json(payload)
        if not ptr:
            return None
        try:
            result = json.loads(ctypes.string_at(ptr).decode("utf-8"))
        finally:
            dll.hierarchy_json_free(ptr)
        return result if isinstance(result, dict) and isinstance(result.get("role_sequence"), list) else None
    except (OSError, ValueError, TypeError, AttributeError):
        return None


def run_executable(executable, paragraphs, timeout=TIMEOUT_SECONDS):
    """실행 파일을 호출해 결과 dict를 돌려준다. 시간 초과·오류·형식 불일치는 None."""
    try:
        payload = json.dumps({"paragraphs": paragraphs}, ensure_ascii=False)
        done = subprocess.run([str(executable)], input=payload.encode("utf-8"),
                              capture_output=True, timeout=timeout, check=False)
        if done.returncode != 0:
            return None
        result = json.loads(done.stdout.decode("utf-8"))
    except (OSError, ValueError, TypeError, subprocess.SubprocessError):
        return None
    if not isinstance(result, dict) or not isinstance(result.get("role_sequence"), list) \
            or len(result["role_sequence"]) != len(paragraphs):
        return None
    return result


def analyze_levels(paragraphs, settings=None, executable=None, timeout=TIMEOUT_SECONDS):
    """설정 hierarchy_engine이 True일 때만 분석한다. 꺼져 있거나 실패하면 None."""
    if settings is None:
        # Keep the engine usable in core callers without importing UI/bootstrap modules.
        try:
            settings = _read_settings()
        except Exception:
            settings = {}
    if not (settings or {}).get("hierarchy_engine", False):
        return None
    dll = find_library() if executable is None else None
    if dll is not None:
        return run_library(dll, paragraphs)
    exe = executable or find_executable()
    if exe is None:
        return None
    return run_executable(exe, paragraphs, timeout)


# 엔진 Role 번호(hierarchy/src/lib.rs) → 쪽 맞춤 문단 역할. 그 밖의 값은 판정 보류(None).
ENGINE_ROLE_NAMES = {4: "소제목", 5: "본문", 6: "내용", 7: "부연설명"}


def engine_role_of_last(paragraphs, settings=None, executable=None, timeout=TIMEOUT_SECONDS):
    """문단 목록(맥락 포함)의 마지막 문단 역할을 엔진으로 판정한다.

    꺼져 있거나 엔진이 실패·판정 보류면 None이며, 호출한 쪽은 기존 규칙을 그대로 쓴다.
    """
    result = analyze_levels(paragraphs, settings, executable=executable, timeout=timeout)
    if not result:
        return None
    roles = result.get("role_sequence")
    if isinstance(roles, list) and roles:
        role = roles[-1]
        return role if role in ENGINE_ROLE_NAMES.values() else None
    # Keep callers that inject the former compact engine response working.
    levels = result.get("levels")
    if isinstance(levels, list) and levels:
        return ENGINE_ROLE_NAMES.get(levels[-1])
    return None
