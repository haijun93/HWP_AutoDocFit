"""Persistent cache for the legacy HWP font folder scan."""
from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path


def _cache_file() -> Path | None:
    local_app_data = os.environ.get("LOCALAPPDATA")
    if not local_app_data:
        return None
    return Path(local_app_data) / "HwpAutoDocFit" / "font_catalog.json"


def _path_key(path) -> str:
    return os.path.normcase(os.path.abspath(os.fspath(path)))


def _stamp(path):
    try:
        stat = Path(path).stat()
        return [True, stat.st_mtime_ns]
    except OSError:
        return [False, None]


def load(folder, candidate_paths):
    """Return cached names if every source directory still has its saved mtime."""
    cache_file = _cache_file()
    if cache_file is None:
        return None
    try:
        data = json.loads(cache_file.read_text(encoding="utf-8"))
        if data.get("version") != 1 or data.get("folder") != _path_key(folder or "."):
            return None
        candidates = sorted({_path_key(path) for path in candidate_paths})
        if data.get("candidates") != candidates:
            return None
        dependencies = data.get("dependencies")
        if not isinstance(dependencies, dict):
            return None
        if any(_stamp(path) != stamp for path, stamp in dependencies.items()):
            return None
        names = data.get("names")
        return names if isinstance(names, list) and all(isinstance(name, str) for name in names) else None
    except (OSError, ValueError, TypeError, AttributeError):
        return None


def store(folder, candidate_paths, source_dirs, names):
    """Atomically persist names and the mtimes that make the scan reusable."""
    cache_file = _cache_file()
    if cache_file is None:
        return
    candidates = sorted({_path_key(path) for path in candidate_paths})
    dependencies = sorted({_path_key(path) for path in [folder or ".", *candidate_paths, *source_dirs]})
    data = {
        "version": 1,
        "folder": _path_key(folder or "."),
        "candidates": candidates,
        "dependencies": {path: _stamp(path) for path in dependencies},
        "names": list(names),
    }
    temporary = None
    try:
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=cache_file.parent,
                                         prefix=cache_file.name + ".", suffix=".tmp", delete=False) as stream:
            temporary = Path(stream.name)
            json.dump(data, stream, ensure_ascii=False)
        os.replace(temporary, cache_file)
    except OSError:
        if temporary is not None:
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass
