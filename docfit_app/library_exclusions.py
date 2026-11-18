"""Shared persistence for folders excluded from document and photo scans."""
from __future__ import annotations

import ntpath
import os
from pathlib import Path
from typing import Any

SETTING_KEY = "library_excluded_folders"


def _key(value: str | Path) -> str:
    raw = str(value).strip()
    if len(raw) >= 2 and raw[1] == ":":
        return ntpath.normcase(ntpath.normpath(raw))
    return os.path.normcase(os.path.abspath(raw)).rstrip("\\/")


def read_exclusions(settings: Any | None) -> list[str]:
    if settings is None:
        return []
    raw = settings.read().get(SETTING_KEY, [])
    if not isinstance(raw, (list, tuple)):
        return []
    values = {}
    for item in raw:
        if isinstance(item, str) and item.strip():
            values.setdefault(_key(item), item.strip())
    return sorted(values.values(), key=str.casefold)


def update_exclusion(settings: Any | None, folder: str, *, add: bool) -> list[str]:
    if settings is None:
        raise ValueError("settings 저장소가 연결되지 않았습니다.")
    clean = _key(folder)
    if not clean:
        raise ValueError("폴더 경로를 입력해 주세요.")
    def change(current: dict[str, Any]) -> None:
        existing = current.get(SETTING_KEY, [])
        values = ({_key(item): item.strip() for item in existing if isinstance(item, str) and item.strip()}
                  if isinstance(existing, list) else {})
        if add:
            values.setdefault(clean, folder.strip())
        else:
            values.pop(clean, None)
        current[SETTING_KEY] = sorted(values.values(), key=str.casefold)
    settings.update(change)
    return read_exclusions(settings)
