"""문서·사진 라이브러리의 기본 읽기 전용 검색 범위."""
from __future__ import annotations

import os
import ntpath
from pathlib import Path
from typing import Iterable

_EXCLUDED_NAMES = frozenset({
    "windows", "program files", "program files (x86)", "programdata",
    "$recycle.bin", "system volume information", "appdata", "node_modules", ".git",
    ".svn", ".hg", "venv", ".venv",
})


def _canonical(value: str | Path) -> str:
    raw = str(value)
    if len(raw) >= 2 and raw[1] == ":":
        return ntpath.normcase(ntpath.normpath(raw))
    return os.path.normcase(os.path.abspath(raw)).rstrip("\\/")


def default_roots(
    fixed_drives: Iterable[str | Path],
    profile: str | Path,
    *,
    desktop_paths: Iterable[str | Path] = (),
    one_drive_paths: Iterable[str | Path] = (),
) -> tuple[Path, ...]:
    """주입한 고정 드라이브와 Desktop 후보를 정규화·중복 제거한다.

    실제 드라이브 열거는 호출자가 담당해 순수 함수로 시험할 수 있다.
    """
    profile_path = Path(profile)
    candidates = [*fixed_drives, profile_path / "Desktop", *desktop_paths, *one_drive_paths]
    roots: list[Path] = []
    seen: set[str] = set()
    for candidate in candidates:
        try:
            path = Path(candidate).expanduser()
            key = _canonical(path)
            if key and key not in seen:
                seen.add(key)
                roots.append(Path(key))
        except (OSError, TypeError, ValueError):
            continue
    return tuple(roots)


def is_excluded_path(path: str | Path, excluded_folders: Iterable[str | Path] = ()) -> bool:
    """예약된 시스템/개발 폴더와 사용자 제외 경로를 판정한다."""
    try:
        raw_target = str(path)
        target = Path(ntpath.normpath(raw_target) if len(raw_target) >= 2 and raw_target[1] == ":" else os.path.abspath(raw_target))
        parts = {part.casefold() for part in target.parts}
        if parts & _EXCLUDED_NAMES:
            return True
        target_key = _canonical(target)
        for item in excluded_folders:
            excluded = _canonical(item)
            sep = "\\" if len(excluded) >= 2 and excluded[1] == ":" else os.sep
            if target_key == excluded or target_key.startswith(excluded + sep):
                return True
    except (OSError, TypeError, ValueError):
        return True
    return False


def fixed_drive_roots() -> tuple[Path, ...]:
    """Windows에서 고정 드라이브 루트만 반환한다. 다른 OS에서는 홈만 쓴다."""
    if os.name != "nt":
        return (Path.home(),)
    try:
        import ctypes
        mask = ctypes.windll.kernel32.GetLogicalDrives()
        roots = []
        for index in range(26):
            if mask & (1 << index):
                root = f"{chr(65 + index)}:\\"
                if ctypes.windll.kernel32.GetDriveTypeW(root) == 3:  # DRIVE_FIXED
                    roots.append(Path(root))
        return tuple(roots)
    except Exception:
        return ()


def current_default_roots() -> tuple[Path, ...]:
    profile = Path(os.environ.get("USERPROFILE") or Path.home())
    one_drive = [Path(value) / "Desktop" for key, value in os.environ.items()
                 if key.upper() in {"ONEDRIVE", "ONEDRIVECONSUMER", "ONEDRIVECOMMERCIAL"} and value]
    return default_roots(fixed_drive_roots(), profile, one_drive_paths=one_drive)
