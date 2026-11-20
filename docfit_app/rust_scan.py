"""Shared adapter for the optional klib filesystem scanner."""
from __future__ import annotations

import json
import logging
import os
import subprocess
import sys
import threading
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger(__name__)


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
