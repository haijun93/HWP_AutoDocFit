"""Path containment helpers that treat different Windows drives as unrelated."""
from __future__ import annotations

import ntpath
import os


def path_is_within(path: str | os.PathLike[str], root: str | os.PathLike[str]) -> bool:
    """Return whether *path* is at or below *root*, including across mixed path inputs.

    ``os.path.commonpath`` raises ``ValueError`` for different Windows drives. Use
    ``ntpath`` for Windows-shaped paths even when tests run on another platform,
    and treat a drive mismatch as ``False``.
    """
    candidate, base = os.fspath(path), os.fspath(root)
    windows = _is_windows_path(candidate) or _is_windows_path(base)
    module = ntpath if windows else os.path
    try:
        candidate = module.normcase(module.normpath(module.abspath(candidate)))
        base = module.normcase(module.normpath(module.abspath(base)))
        return module.commonpath((candidate, base)) == base
    except (OSError, ValueError, TypeError):
        return False


def _is_windows_path(value: str) -> bool:
    return (len(value) >= 2 and value[1] == ":") or value.startswith(("\\\\", "//"))
