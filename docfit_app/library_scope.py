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


def is_excluded_path(path: str | Path, excluded_folders: Iterable[str | Path] = (), base: str | Path | None = None) -> bool:
    """예약된 시스템/개발 폴더와 사용자 제외 경로를 판정한다.

    base(스캔 시작 폴더)가 주어지면 예약 폴더 이름은 base 아래 상대 경로에서만 본다. 그래야 사용자가
    AppData 아래 폴더를 직접 지정하거나 임시 폴더가 AppData 아래인 환경에서도 그 폴더가 통째로 제외되지 않는다.
    """
    try:
        raw_target = str(path)
        windows_target = _is_windows_path(raw_target)
        target = Path(ntpath.normpath(raw_target) if windows_target else os.path.abspath(raw_target))
        name_parts = ntpath.normpath(raw_target).replace("/", "\\").split("\\") if windows_target else target.parts
        if base is not None:
            raw_base = str(base)
            windows_base = _is_windows_path(raw_base)
            base_path = Path(ntpath.normpath(raw_base) if windows_base else os.path.abspath(raw_base))
            base_key = _canonical(base_path)
            target_key0 = _canonical(target)
            sep0 = "\\" if windows_base else os.sep
            if target_key0 == base_key:
                name_parts = ()
            elif target_key0.startswith(base_key + sep0):
                if windows_base:
                    name_parts = ntpath.normpath(raw_target).replace("/", "\\").split("\\")[len(ntpath.normpath(raw_base).replace("/", "\\").split("\\")):]
                else:
                    name_parts = target.parts[len(base_path.parts):]
        parts = {part.casefold() for part in name_parts}
        if parts & _EXCLUDED_NAMES:
            return True
        target_key = _canonical(target)
        for item in excluded_folders:
            excluded = _canonical(item)
            sep = "\\" if _is_windows_path(str(item)) else os.sep
            if target_key == excluded or target_key.startswith(excluded + sep):
                return True
    except (OSError, TypeError, ValueError):
        return True
    return False


def _is_windows_path(raw: str) -> bool:
    return (len(raw) >= 2 and raw[1] == ":") or raw.startswith(("\\\\", "//"))


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
