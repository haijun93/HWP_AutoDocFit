"""Atomic persistent snapshots for the default document and photo views."""
from __future__ import annotations

import hashlib
import json
import os
import tempfile
from pathlib import Path
from typing import Any


def _data_root() -> Path:
    from .kagent_client import data_dir
    return data_dir()


def _signature(extensions: Any, excluded: Any, roots: Any = ()) -> str:
    value = json.dumps({"extensions": sorted(str(x).casefold() for x in extensions),
                        "excluded": sorted(os.path.normcase(os.path.normpath(str(x))) for x in excluded),
                        "roots": sorted(os.path.normcase(os.path.normpath(str(x))) for x in roots)},
                       ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def load(view: str, extensions: Any, excluded: Any, *, root: Path | None = None, roots: Any = ()) -> list[dict[str, Any]] | None:
    path = (root or _data_root()) / "scan-cache" / f"{view}.json"
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(payload, dict):
            return None
        if payload.get("version") != 1 or payload.get("signature") != _signature(extensions, excluded, roots):
            return None
        rows = payload.get("items")
        return rows if isinstance(rows, list) and all(isinstance(row, dict) for row in rows) else None
    except (OSError, ValueError, TypeError):
        return None


def save(view: str, extensions: Any, excluded: Any, items: list[dict[str, Any]], *, root: Path | None = None, roots: Any = ()) -> None:
    directory = (root or _data_root()) / "scan-cache"
    directory.mkdir(parents=True, exist_ok=True)
    payload = {"version": 1, "signature": _signature(extensions, excluded, roots), "items": items}
    _write(directory, view, payload)


def _write(directory: Path, view: str, payload: dict[str, Any]) -> None:
    fd, tmp_name = tempfile.mkstemp(prefix=f".{view}-", suffix=".tmp", dir=directory)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
            json.dump(payload, stream, ensure_ascii=False, separators=(",", ":"))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp_name, directory / f"{view}.json")
    finally:
        try:
            os.unlink(tmp_name)
        except FileNotFoundError:
            pass


def rewrite(view: str, change: Any, *, root: Path | None = None) -> None:
    """저장된 목록의 항목만 바꾼다(`change(items) -> items`). 범위 서명은 그대로 둔다."""
    directory = (root or _data_root()) / "scan-cache"
    try:
        payload = json.loads((directory / f"{view}.json").read_text(encoding="utf-8"))
        items = payload["items"]
    except (OSError, ValueError, TypeError, KeyError):
        return
    if not isinstance(items, list) or not all(isinstance(row, dict) for row in items):
        return
    payload["items"] = change(items)
    _write(directory, view, payload)
