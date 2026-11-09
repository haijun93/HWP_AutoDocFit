"""위계 엔진(hierarchy 실행 파일) 호출 브리지. 설정 hierarchy_engine이 켜졌을 때만 사용한다."""
import json
import ctypes
import os
import subprocess
import sys
from pathlib import Path

TIMEOUT_SECONDS = 5.0
_NAME = "hierarchy.exe" if sys.platform == "win32" else "hierarchy"
_DLL_NAME = "hierarchy.dll" if sys.platform == "win32" else ("libhierarchy.dylib" if sys.platform == "darwin" else "libhierarchy.so")
_DLL = None
_DLL_TRIED = False
_DLL_LOCK = __import__("threading").Lock()
_DLL_DIRECTORY_HANDLE = None


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
    for candidate in (base / "resources" / _DLL_NAME, base / _DLL_NAME,
                      repository / "hierarchy" / "target" / "release" / _DLL_NAME):
        if candidate.is_file():
            return candidate
    return None


def _load_library(path=None):
    """Load the C ABI once per process; retain the handle for app lifetime."""
    global _DLL, _DLL_TRIED, _DLL_DIRECTORY_HANDLE
    with _DLL_LOCK:
        if _DLL_TRIED and path is None:
            return _DLL
        _DLL_TRIED = True
        candidate = Path(path) if path is not None else find_library()
        if candidate is None:
            return None
        try:
            if sys.platform == "win32" and hasattr(os, "add_dll_directory"):
                # Rust cdylib dependencies are colocated in packaged resources.
                _DLL_DIRECTORY_HANDLE = os.add_dll_directory(str(candidate.parent))
            library = ctypes.CDLL(str(candidate))
            library.hierarchy_analyze_json.argtypes = (ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t)
            library.hierarchy_analyze_json.restype = ctypes.c_void_p
            library.hierarchy_free_string.argtypes = (ctypes.c_void_p,)
            library.hierarchy_free_string.restype = None
            _DLL = library
        except (OSError, AttributeError, TypeError):
            _DLL = None
        return _DLL


def _normalized_paragraphs(paragraphs):
    return [{"text": str(p.get("text", "")), "indent": int(p.get("indent", 0) or 0),
             "font_size": float(p.get("font_size", 0) or 0),
             "bold": bool(p.get("bold", False)),
             "in_attachment": bool(p.get("in_attachment", False))} for p in paragraphs]


def run_library(library, paragraphs):
    """Call the already-loaded C ABI and adapt its paragraph roles to CLI levels."""
    encoded = None
    output_ptr = None
    try:
        payload = json.dumps({"paragraphs": _normalized_paragraphs(paragraphs)}, ensure_ascii=False).encode("utf-8")
        encoded = (ctypes.c_uint8 * len(payload)).from_buffer_copy(payload)
        output_ptr = library.hierarchy_analyze_json(encoded, len(payload))
        if not output_ptr:
            return None
        raw = ctypes.string_at(output_ptr).decode("utf-8")
        result = json.loads(raw)
        items = result.get("results") if isinstance(result, dict) else None
        if not isinstance(items, list) or len(items) != len(paragraphs):
            return None
        role_levels = {"document_title": 0, "chapter": 1, "section": 2, "topic": 3,
                       "square": 4, "circle": 5, "dash": 6, "asterisk": 7,
                       "body": 8, "table_cell": 9, "ambiguous": 10}
        levels = [role_levels.get(item.get("role"), 0) for item in items]
        def format_diagnostic(diagnostic):
            if isinstance(diagnostic, str):
                return diagnostic
            if not isinstance(diagnostic, dict) or not diagnostic:
                return ""
            name, fields = next(iter(diagnostic.items()))
            if not isinstance(fields, dict):
                return str(name)
            values = [str(value) for value in fields.values()]
            return name.lower() + ":" + ":".join(values)
        return {"levels": levels, "confidence": min((int(x.get("confidence", 0)) for x in items), default=100),
                "ambiguous": [i for i, x in enumerate(items) if x.get("role") == "ambiguous"],
                "diagnostics": [format_diagnostic(d) for x in items for d in x.get("diagnostics", [])], "results": items}
    except (OSError, ValueError, TypeError, AttributeError, ctypes.ArgumentError):
        return None
    finally:
        if output_ptr:
            try:
                library.hierarchy_free_string(output_ptr)
            except (OSError, AttributeError, TypeError):
                pass


def run_executable(executable, paragraphs, timeout=TIMEOUT_SECONDS):
    """실행 파일을 호출해 결과 dict를 돌려준다. 시간 초과·오류·형식 불일치는 None."""
    try:
        payload = json.dumps({"paragraphs": [
            {"text": str(p.get("text", "")), "indent": int(p.get("indent", 0) or 0),
             "font_size": float(p.get("font_size", 0) or 0)} for p in paragraphs]},
            ensure_ascii=False)
        done = subprocess.run([str(executable)], input=payload.encode("utf-8"),
                              capture_output=True, timeout=timeout, check=False)
        if done.returncode != 0:
            return None
        result = json.loads(done.stdout.decode("utf-8"))
    except (OSError, ValueError, TypeError, subprocess.SubprocessError):
        return None
    if not isinstance(result, dict) or not isinstance(result.get("levels"), list) \
            or len(result["levels"]) != len(paragraphs):
        return None
    return result


def analyze_levels(paragraphs, settings=None, executable=None, timeout=TIMEOUT_SECONDS):
    """설정 hierarchy_engine이 True일 때만 분석한다. 꺼져 있거나 실패하면 None."""
    if settings is None:
        # Keep the engine usable in core callers without importing UI/bootstrap modules.
        try:
            import os
            from docfit_app.settings_schema import DEFAULTS
            from docfit_app.settings_service import SettingsService
            appdata = os.environ.get("APPDATA")
            settings = SettingsService(Path(appdata) / "HwpAutoDocFit" / "settings.json").read() if appdata else {}
            if "hierarchy_engine" not in settings:
                settings = {**DEFAULTS, **settings}
        except Exception:
            settings = {}
    if not (settings or {}).get("hierarchy_engine", False):
        return None
    if not paragraphs:
        return {"levels": [], "confidence": 100, "ambiguous": [], "diagnostics": []}
    if executable is None:
        library = _load_library()
        if library is not None:
            result = run_library(library, paragraphs)
            if result is not None:
                return result
        executable = find_executable()
    if executable is None:
        return None
    return run_executable(executable, paragraphs, timeout)


# 엔진 Role 번호(hierarchy/src/lib.rs) → 쪽 맞춤 문단 역할. 그 밖의 값은 판정 보류(None).
ENGINE_ROLE_NAMES = {4: "소제목", 5: "본문", 6: "내용", 7: "부연설명"}


def engine_role_of_last(paragraphs, settings=None, executable=None, timeout=TIMEOUT_SECONDS):
    """문단 목록(맥락 포함)의 마지막 문단 역할을 엔진으로 판정한다.

    꺼져 있거나 엔진이 실패·판정 보류면 None이며, 호출한 쪽은 기존 규칙을 그대로 쓴다.
    """
    result = analyze_levels(paragraphs, settings, executable=executable, timeout=timeout)
    if not result or not result["levels"]:
        return None
    return ENGINE_ROLE_NAMES.get(result["levels"][-1])
