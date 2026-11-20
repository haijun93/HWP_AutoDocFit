"""Shared adapter for the optional klib filesystem scanner."""
from __future__ import annotations

import json
import logging
import os
import subprocess
import sys
import threading
import struct
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger(__name__)

KFindPipeError = OSError
KFindPipeName = r"\\.\pipe\HwpAutoDocFit.Index"
_MAX_PIPE_REQUEST = 64 << 10
_MAX_PIPE_RESPONSE = 64 << 20


def _read_exact(pipe: Any, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        chunk = pipe.read(size - len(chunks))
        if not chunk:
            raise OSError("kfind pipe closed")
        chunks.extend(chunk)
    return bytes(chunks)


def names_query(extensions: set[str] | frozenset[str], excluded: Any = (), *,
                pipe_name: str = KFindPipeName, opener: Any = None) -> list[dict[str, Any]] | None:
    """Query the ready service index. Return None for unavailable or invalid replies."""
    if os.name != "nt":
        return None
    try:
        from .kfind_process import is_pipe_alive
        if opener is None and not is_pipe_alive(pipe_name, timeout=0.1): return None
        open_pipe = opener or (lambda name: open(name, "r+b", buffering=0))
        counter = 1

        def request(command: str, **fields: Any) -> dict[str, Any]:
            nonlocal counter
            payload = json.dumps(dict(command=command, **({"id": counter} if command == "names_query" else {}), **fields),
                                 ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            counter += 1
            if len(payload) > _MAX_PIPE_REQUEST:
                raise ValueError("request too large")
            with open_pipe(pipe_name) as pipe:
                pipe.write(struct.pack("<I", len(payload)) + payload)
                length = struct.unpack("<I", _read_exact(pipe, 4))[0]
                if not 0 < length <= _MAX_PIPE_RESPONSE:
                    raise ValueError("invalid response length")
                envelope = json.loads(_read_exact(pipe, length).decode("utf-8"))
            if not isinstance(envelope, dict) or envelope.get("ok") is not True:
                raise ValueError("kfind returned an error")
            result = envelope.get("result")
            if not isinstance(result, dict):
                raise ValueError("invalid kfind result")
            return result

        status = request("names_status")
        if status.get("state") != "ready" or status.get("mode") != "service":
            return None
        from .library_scope import _EXCLUDED_NAMES
        from .library_scope import current_default_roots
        roots = [str(p) for p in current_default_roots()]
        full_excluded = [os.path.join(root, name) for root in roots for name in _EXCLUDED_NAMES]
        full_excluded.extend(str(x) for x in excluded)
        rows = []
        offset = 0
        while True:
            result = request("names_list", extensions=sorted(x.lstrip(".").casefold() for x in extensions),
                             roots=roots, exclude=full_excluded, offset=offset, limit=5000)
            items, next_offset, done = result.get("items"), result.get("next"), result.get("done")
            if not isinstance(items, list) or not isinstance(next_offset, int) or not isinstance(done, bool): return None
            for item in items:
                if not isinstance(item, dict) or not isinstance(item.get("path"), str) or not isinstance(item.get("name"), str):
                    raise ValueError("invalid names_list row")
                path, name = item["path"], item["name"]
                ext = Path(name).suffix.lstrip(".").casefold()
                if ext not in {x.lstrip(".").casefold() for x in extensions}: continue
                row = {"path": path, "name": name, "ext": ext}
                for key in ("size", "mtime", "mtime_ns", "cloud"):
                    if key in item: row[key] = item[key]
                rows.append(row)
            if next_offset <= offset and not done: return None
            offset = next_offset
            if done: break
        return rows
    except Exception as exc:
        logger.debug("kfind names query unavailable; falling back", exc_info=exc)
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
                        pipe_name: str = KFindPipeName, opener: Any = None) -> list[dict[str, Any]] | None:
    if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python": return None
    return names_query(extensions, excluded, pipe_name=pipe_name, opener=opener)


def scan_default_roots(extensions: set[str] | frozenset[str], excluded: Any = (), *, executable: Path | None = None,
                       cancel_event: threading.Event | None = None, on_progress: Any | None = None,
                       roots: Any = None) -> list[dict[str, Any]] | None:
    """Rust scan all default desktop/fixed roots in one concurrent process."""
    if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python": return None
    executable = executable or find_klib_executable()
    if not executable: return None
    from .library_scope import current_default_roots, _EXCLUDED_NAMES
    scan_roots = [Path(p) for p in (roots if roots is not None else current_default_roots()) if Path(p).is_dir()]
    if not scan_roots: return []
    command = [str(executable), "scan", "--roots", *map(str, scan_roots), "--ext",
               ",".join(sorted(x.lstrip(".").lower() for x in extensions)), "--json-lines"]
    excludes = [str(root / name) for root in scan_roots for name in _EXCLUDED_NAMES]
    excludes.extend(map(str, excluded))
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
            if on_progress: on_progress(len(rows), str(Path(row["path"]).parent))
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
