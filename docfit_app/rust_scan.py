"""Shared adapter for the optional klib filesystem scanner."""
from __future__ import annotations

import json
import logging
import math
import os
import subprocess
import sys
import threading
import struct
import time
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger(__name__)

KFindPipeError = OSError
KFindPipeName = r"\\.\pipe\HwpAutoDocFit.Index"
_MAX_PIPE_REQUEST = 64 << 10
_MAX_PIPE_RESPONSE = 64 << 20

# unknown → ok | outdated | unavailable; 재시작 성공 뒤 unknown, 다음 조회로 재확인.
_SERVICE_STATUS_LOCK = threading.Lock()
_SERVICE_STATUS: dict[str, Any] = {
    "status": "unknown", "outdated": False, "message": "", "version": None, "last_checked": 0.0,
}


def get_service_version_status() -> dict[str, Any]:
    with _SERVICE_STATUS_LOCK:
        return dict(_SERVICE_STATUS)


def set_service_version_status(status: str, message: str = "", version: Any = None) -> None:
    if status not in {"unknown", "ok", "outdated", "unavailable"}:
        raise ValueError("invalid service compatibility state")
    with _SERVICE_STATUS_LOCK:
        _SERVICE_STATUS.update(status=status, outdated=status == "outdated", message=message,
                               version=version, last_checked=time.time())


class _UnsupportedCommand(ValueError):
    pass


def _read_exact(pipe: Any, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        chunk = pipe.read(size - len(chunks))
        if not chunk:
            raise OSError("kfind pipe closed")
        chunks.extend(chunk)
    return bytes(chunks)


def _pipe_request(command: str, pipe_name: str, opener: Any, **fields: Any) -> dict[str, Any]:
    payload = json.dumps(dict(command=command, **fields), ensure_ascii=False,
                         separators=(",", ":")).encode("utf-8")
    if len(payload) > _MAX_PIPE_REQUEST:
        raise ValueError("request too large")
    with opener(pipe_name) as pipe:
        pipe.write(struct.pack("<I", len(payload)) + payload)
        length = struct.unpack("<I", _read_exact(pipe, 4))[0]
        if not 0 < length <= _MAX_PIPE_RESPONSE:
            raise ValueError("invalid response length")
        envelope = json.loads(_read_exact(pipe, length).decode("utf-8"))
    if not isinstance(envelope, dict):
        raise ValueError("invalid envelope")
    if envelope.get("ok") is not True:
        error = str(envelope.get("error", "")).casefold()
        if any(x in error for x in ("unknown variant", "unknown command", "unsupported command", "모르는 명령")):
            raise _UnsupportedCommand("unsupported service command")
        raise ValueError("service request failed")
    result = envelope.get("result")
    if not isinstance(result, dict):
        raise ValueError("invalid result")
    return result


def _open_service(pipe_name: str, opener: Any) -> Any:
    if opener is not None:
        return opener
    if os.name != "nt" or (pipe_name == KFindPipeName and os.environ.get("DOCFIT_FORBID_REAL_DATA") == "1"):
        raise OSError("service access disabled")
    from .kfind_process import is_pipe_alive
    if not is_pipe_alive(pipe_name, timeout=0.1):
        raise OSError("service unavailable")
    return lambda name: open(name, "r+b", buffering=0)


def _check_capabilities(status: dict[str, Any]) -> None:
    for field in ("features", "commands"):
        advertised = status.get(field)
        if advertised is not None and (not isinstance(advertised, list) or "names_list" not in advertised):
            raise _UnsupportedCommand("names_list unsupported")


def _record_service_capabilities(status: dict[str, Any]) -> None:
    # names_list만 되는 이전 서비스도 목록은 계속 쓰되, 새 메타데이터 기능 적용을 안내한다.
    if "metadata_v2" not in (status.get("features") or []):
        set_service_version_status("outdated", "색인 서비스가 이전 버전입니다. 목록 정보 개선을 적용하려면 최신 앱 설치 후 서비스를 다시 시작해 주세요.",
                                   version=status.get("version"))
    else:
        set_service_version_status("ok", version=status.get("version"))


def _failed_service_check(exc: Exception) -> None:
    if isinstance(exc, _UnsupportedCommand):
        set_service_version_status("outdated", "색인 서비스가 이전 버전입니다. 최신 앱 설치 후 서비스를 다시 시작해 주세요.")
    else:
        set_service_version_status("unavailable", "색인 서비스를 사용할 수 없어 파일 검색으로 목록을 표시합니다.")


def check_service_version(*, pipe_name: str = KFindPipeName, opener: Any = None) -> dict[str, Any]:
    """기능 광고가 없는 이전 프로토콜은 작은 names_list 요청으로 실제 지원을 확인한다."""
    try:
        open_pipe = _open_service(pipe_name, opener)
        status = _pipe_request("names_status", pipe_name, open_pipe)
        _check_capabilities(status)
        if "features" not in status and "commands" not in status:
            if status.get("state") != "ready":
                set_service_version_status("unknown", "색인 준비가 끝나면 기능을 다시 확인합니다.")
                return get_service_version_status()
            _pipe_request("names_list", pipe_name, open_pipe, extensions=["hwpx"], limit=1)
        _record_service_capabilities(status)
    except (OSError, ValueError, TypeError) as exc:
        _failed_service_check(exc)
    return get_service_version_status()


def names_query(extensions: set[str] | frozenset[str], excluded: Any = (), *,
                pipe_name: str = KFindPipeName, opener: Any = None,
                on_row: Callable[[dict[str, Any]], None] | None = None) -> list[dict[str, Any]] | None:
    """ready 서비스 조회. offset/next는 결과 개수가 아닌 색인 위치이며 다음 쪽으로 이어 간다."""
    try:
        open_pipe = _open_service(pipe_name, opener)
        status = _pipe_request("names_status", pipe_name, open_pipe)
        _check_capabilities(status)
        if status.get("state") != "ready" or status.get("mode") != "service":
            set_service_version_status("unavailable", "색인 서비스가 준비 중이어서 파일 검색으로 목록을 표시합니다.")
            return None
        from .library_scope import current_default_roots
        roots = [str(p) for p in current_default_roots()]
        rows = []
        offset = 0
        wanted = {x.lstrip(".").casefold() for x in extensions}
        while True:
            result = _pipe_request("names_list", pipe_name, open_pipe, extensions=sorted(wanted),
                                   roots=roots, exclude=list(map(str, excluded)), offset=offset, limit=5000)
            items, next_offset, done = result.get("items"), result.get("next"), result.get("done")
            if (not isinstance(items, list) or type(next_offset) is not int or next_offset < offset
                    or not isinstance(done, bool) or (next_offset == offset and not done)):
                raise ValueError("invalid names_list cursor")
            for item in items:
                if not isinstance(item, dict) or not isinstance(item.get("path"), str) or not isinstance(item.get("name"), str):
                    raise ValueError("invalid names_list row")
                path, name = item["path"], item["name"]
                ext = os.path.splitext(name)[1].lstrip(".").casefold()
                if ext not in wanted:
                    continue
                row = {"path": path, "name": name, "ext": ext}
                for key in ("size", "mtime", "mtime_ns", "cloud"):
                    if key in item:
                        row[key] = item[key]
                if (('size' in row and (type(row['size']) is not int or row['size'] < 0))
                        or ('mtime' in row and (type(row['mtime']) not in (int, float) or not math.isfinite(row['mtime'])))
                        or ('mtime_ns' in row and type(row['mtime_ns']) is not int)
                        or ('cloud' in row and not isinstance(row['cloud'], bool))):
                    raise ValueError("invalid names_list metadata")
                rows.append(row)
                if on_row:
                    on_row(row)
            offset = next_offset
            if done:
                break
        _record_service_capabilities(status)
        return rows
    except (OSError, ValueError, TypeError) as exc:
        _failed_service_check(exc)
        logger.debug("kfind names query unavailable; falling back (%s)", type(exc).__name__)
        return None


def _name_row_to_scan(row: dict[str, Any]) -> dict[str, Any]:
    path, name = row["path"], row["name"]
    out = {"path": path, "name": name, "ext": Path(name).suffix.lstrip(".").lower(),
           "size": row.get("size", 0), "mtime": row.get("mtime", 0), "cloud": row.get("cloud", False)}
    if "mtime_ns" in row:
        out["mtime_ns"] = row["mtime_ns"]
    return out


def scan_default(extensions: set[str] | frozenset[str], excluded: Any = (), *, executable: Path | None = None,
                 cancel_event: threading.Event | None = None, on_progress: Any | None = None) -> tuple[list[dict[str, Any]] | None, str]:
    if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python":
        return None, "python"
    rows = service_names_query(extensions, excluded)
    if rows is not None:
        return [_name_row_to_scan(row) for row in rows], "service"
    rows = scan_default_roots(extensions, excluded, executable=executable, cancel_event=cancel_event,
                              on_progress=on_progress)
    return (rows, "klib") if rows is not None else (None, "python")


def service_names_query(extensions: set[str] | frozenset[str], excluded: Any = (), *,
                        pipe_name: str = KFindPipeName, opener: Any = None,
                        on_row: Callable[[dict[str, Any]], None] | None = None) -> list[dict[str, Any]] | None:
    if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python": return None
    extra: dict[str, Any] = {"on_row": on_row} if on_row else {}
    return names_query(extensions, excluded, pipe_name=pipe_name, opener=opener, **extra)


def scan_default_roots(extensions: set[str] | frozenset[str], excluded: Any = (), *, executable: Path | None = None,
                       cancel_event: threading.Event | None = None, on_progress: Any | None = None,
                       roots: Any = None,
                       on_row: Callable[[dict[str, Any]], None] | None = None) -> list[dict[str, Any]] | None:
    """Rust scan all default desktop/fixed roots in one concurrent process."""
    if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python": return None
    executable = executable or find_klib_executable()
    if not executable: return None
    from .library_scope import current_default_roots
    scan_roots = [Path(p) for p in (roots if roots is not None else current_default_roots()) if Path(p).is_dir()]
    if not scan_roots: return None
    command = [str(executable), "scan", "--roots", *map(str, scan_roots), "--ext",
               ",".join(sorted(x.lstrip(".").lower() for x in extensions)), "--json-lines"]
    # 예약 폴더는 klib 이 스스로(루트 아래 폴더 이름으로) 건너뛴다. 루트×이름 목록까지 넘기면 klib 이
    # 파일마다 그 목록을 모두 대조해 느려지므로, win32 창처럼 사용자 제외 폴더만 넘긴다.
    excludes = list(map(str, excluded))
    if excludes: command += ["--exclude", *excludes]
    process = None
    rows = []
    try:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   text=True, encoding="utf-8", errors="strict", bufsize=1)
        watcher = threading.Thread(target=_terminated, args=(process, cancel_event or threading.Event()), daemon=True)
        watcher.start()
        assert process.stdout is not None
        for line in process.stdout:
            if cancel_event is not None and cancel_event.is_set(): break
            if not line.strip(): continue
            row = json.loads(line)
            if not isinstance(row, dict) or not all(k in row for k in ("path", "name", "ext", "size", "mtime", "cloud")):
                raise ValueError("invalid klib scan JSON row")
            rows.append(row)
            if on_row: on_row(row)
            if on_progress: on_progress(len(rows), os.path.dirname(row["path"]))
        process.wait()
        if cancel_event is not None and cancel_event.is_set(): return rows
        return rows if process.returncode == 0 else None
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError, subprocess.SubprocessError):
        if process is not None and process.poll() is None: process.kill(); process.wait()
        return None
    finally:
        if process is not None and process.stdout is not None: process.stdout.close()


def find_klib_executable() -> Path | None:
    env_exe = os.environ.get("KLIB_EXE")
    if env_exe and Path(env_exe).is_file():
        return Path(env_exe)
    base = Path(getattr(sys, "_MEIPASS", ""))
    name = "klib.exe" if os.name == "nt" else "klib"
    candidates = ([base / "resources" / name] if base.is_dir() else [])
    repo = Path(__file__).resolve().parents[1]
    candidates.extend((repo / "resources" / name, repo / "klib" / "target" / "release" / name,
                       repo / "klib" / "target" / "debug" / name))
    return next((candidate for candidate in candidates if candidate.is_file()), None)


def _terminated(process: subprocess.Popen, cancel_event: threading.Event) -> None:
    while process.poll() is None:
        if cancel_event.wait(0.025):
            try:
                process.terminate()
                process.wait(timeout=1)
            except (OSError, subprocess.TimeoutExpired):
                try:
                    process.kill()
                except OSError:
                    pass
            return


def scan(root: Path, extensions: set[str] | frozenset[str], excluded: Any = (), *, executable: Path | None = None,
         cancel_event: threading.Event | None = None,
         on_progress: Callable[[int, str], None] | None = None) -> list[dict[str, Any]] | None:
    """Return validated Rust rows, or None when Rust cannot be used."""
    if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python":
        return None
    executable = executable or find_klib_executable()
    if not executable:
        return None
    command = [str(executable), "scan", "--roots", str(root), "--ext",
               ",".join(sorted(ext.lstrip(".").lower() for ext in extensions)), "--json-lines"]
    from .library_scope import _EXCLUDED_NAMES
    excludes = [str(Path(root) / name) for name in _EXCLUDED_NAMES]
    excludes.extend(str(path) for path in excluded)
    if excludes:
        command += ["--exclude", *excludes]
    process = None
    rows: list[dict[str, Any]] = []
    try:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   text=True, encoding="utf-8", errors="strict", bufsize=1)
        watcher = threading.Thread(target=_terminated, args=(process, cancel_event or threading.Event()), daemon=True)
        watcher.start()
        assert process.stdout is not None
        for line in process.stdout:
            if cancel_event is not None and cancel_event.is_set():
                break
            if not line.strip():
                continue
            row = json.loads(line)
            if not isinstance(row, dict) or not all(k in row for k in ("path", "name", "ext", "size", "mtime", "cloud")):
                raise ValueError("invalid klib scan JSON row")
            if not isinstance(row["path"], str) or not isinstance(row["name"], str) or not isinstance(row["ext"], str):
                raise ValueError("invalid klib scan row types")
            if not isinstance(row["size"], int) or not isinstance(row["mtime"], (int, float)) or not isinstance(row["cloud"], bool):
                raise ValueError("invalid klib scan row values")
            rows.append(row)
            if on_progress is not None:
                on_progress(len(rows), str(Path(row["path"]).parent))
        process.wait()
        if cancel_event is not None and cancel_event.is_set():
            return rows
        if process.returncode != 0:
            return None
        return rows
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError, subprocess.SubprocessError):
        logger.warning("klib scan failed; falling back to Python", exc_info=True)
        if process is not None and process.poll() is None:
            process.kill()
            process.wait()
        return None
    finally:
        if process is not None and process.stdout is not None:
            process.stdout.close()
