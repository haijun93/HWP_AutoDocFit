"""조사 폴더 증분 스캔. 파일 내용은 읽지 않고 메타데이터(크기·수정·생성 시각)만 본다.

Windows의 os.scandir는 디렉터리 목록과 함께 stat 정보를 주므로 파일마다 추가 호출이 없다.
변경 없는 파일은 쓰지 않는다(변경 없음 = DB 쓰기 0).
"""
from __future__ import annotations

import os
import json
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

from .rules import EXCLUDED_DIRS, EXCLUDED_NAMES, EXT_KIND, Rules, cloud_only
from .store import path_key

CHUNK = 500


def _rust_klib():
    repository = Path(__file__).resolve().parents[2]
    base = Path(getattr(sys, "_MEIPASS", repository))
    name = "klib.exe" if os.name == "nt" else "klib"
    for candidate in (base / "resources" / name, repository / "klib" / "target" / "release" / name):
        if candidate.is_file():
            return candidate
    return None


def _rust_enabled():
    if os.environ.get("DOCFIT_RUST_TIMEMACHINE_SCAN", "1").strip().lower() in ("0", "false", "no", "off"):
        return False
    try:
        appdata = os.environ.get("APPDATA") or os.environ.get("LOCALAPPDATA")
        settings_path = Path(appdata) / "HwpAutoDocFit" / "settings.json" if appdata else None
        if settings_path and settings_path.is_file():
            settings = json.loads(settings_path.read_text(encoding="utf-8"))
            return bool(settings.get("rust_timemachine_scan", True))
    except (OSError, ValueError, TypeError):
        pass
    return True


def _scan_rust(root, rules):
    executable = _rust_klib()
    if executable is None:
        return None
    base_extensions = {"." + ext: kind for ext, kind in EXT_KIND.items()}
    request = {
        "extensions": [ext.lstrip(".") for ext, kind in rules.extensions.items() if base_extensions.get(ext) == kind],
        "extra_extensions": [ext.lstrip(".") for ext, kind in rules.extensions.items() if base_extensions.get(ext) != kind],
        "extra_dirs": sorted(rules.dirs - EXCLUDED_DIRS),
        "extra_names": [name for name in rules.names if name not in {x.casefold() for x in EXCLUDED_NAMES}],
        "time_mode": rules.time_mode, "include_hidden": rules.include_hidden,
    }
    try:
        completed = subprocess.run([str(executable), "tm-scan", root, json.dumps(request, ensure_ascii=False)],
                                   capture_output=True, text=True, encoding="utf-8", timeout=120, check=True)
        value = json.loads(completed.stdout)
        if not isinstance(value, dict) or not isinstance(value.get("rows"), list):
            return None
        return value
    except (OSError, ValueError, subprocess.SubprocessError):
        return None


@dataclass
class ScanResult:
    seen: int = 0
    written: int = 0
    removed: int = 0
    skipped_dirs: int = 0
    errors: list = field(default_factory=list)
    cancelled: bool = False


def _stat_times(info):
    ctime = getattr(info, "st_birthtime", None) or info.st_ctime
    return info.st_mtime, ctime


def scan_root(store, root, rules=None, *, cancel=None, progress=None, chunk=CHUNK, recalculate=None):
    """root 아래를 훑어 색인을 갱신한다. cancel()이 참이면 지금까지 한 것만 쓰고 멈춘다(삭제 정리는 생략)."""
    rules = rules or Rules()
    if recalculate is None:
        prev_mode = store.get_meta("time_mode")
        recalculate = prev_mode is not None and prev_mode != rules.time_mode
    root = os.path.abspath(root)
    result = ScanResult()
    store.purge_excluded()
    known = store.known(root)
    seen_keys = set()
    pending = []
    native = _scan_rust(root, rules) if _rust_enabled() and not (cancel and cancel()) else None
    if native is not None:
        result.skipped_dirs = int(native.get("skipped_dirs", 0))
        result.errors.extend(tuple(error) for error in native.get("errors", []))
        for row in native["rows"]:
            key = path_key(row["path"])
            seen_keys.add(key)
            result.seen += 1
            if not recalculate and known.get(key) == (row["size"], row["mtime"], row["ctime"]):
                continue
            pending.append({k: row[k] for k in ("path", "root", "name", "kind", "size", "mtime", "ctime", "work_time", "flags", "cloud")})
            if len(pending) >= chunk:
                result.written += store.upsert_many(pending)
                pending = []
                if progress:
                    progress(result)
        result.written += store.upsert_many(pending)
        if not result.cancelled:
            gone = [key for key in known if key not in seen_keys]
            result.removed = store.delete_keys(gone) if gone else 0
            store.set_meta("time_mode", rules.time_mode)
        if progress:
            progress(result)
        return result
    stack = [root]
    while stack:
        if cancel and cancel():
            result.cancelled = True
            break
        folder = stack.pop()
        try:
            iterator = os.scandir(folder)
        except OSError as error:
            result.errors.append((folder, error.__class__.__name__))
            continue
        with iterator:
            for item in iterator:
                try:
                    if item.is_symlink():
                        continue
                    info = item.stat(follow_symlinks=False)
                    attributes = getattr(info, "st_file_attributes", 0)
                    if item.is_dir(follow_symlinks=False):
                        if attributes & 0x400 or rules.skip_dir(item.name, attributes):  # 0x400 = 정션
                            result.skipped_dirs += 1
                        else:
                            stack.append(item.path)
                        continue
                    if rules.skip_file(item.name, attributes):
                        continue
                except OSError as error:
                    result.errors.append((item.path, error.__class__.__name__))
                    continue
                key = path_key(item.path)
                seen_keys.add(key)
                result.seen += 1
                mtime, ctime = _stat_times(info)
                if not recalculate and known.get(key) == (info.st_size, mtime, ctime):
                    continue
                work_time, flags = rules.work_time(ctime, mtime)
                pending.append(dict(path=item.path, root=root, name=item.name, kind=rules.kind(item.name),
                                    size=info.st_size, mtime=mtime, ctime=ctime, work_time=work_time, flags=flags,
                                    cloud=cloud_only(attributes)))
                if len(pending) >= chunk:
                    result.written += store.upsert_many(pending)
                    pending = []
                    if progress:
                        progress(result)
    result.written += store.upsert_many(pending)
    if not result.cancelled:
        gone = [key for key in known if key not in seen_keys]
        result.removed = store.delete_keys(gone) if gone else 0
        store.set_meta("time_mode", rules.time_mode)
    if progress:
        progress(result)
    return result


def scan_roots(store, roots, rules=None, *, cancel=None, progress=None):
    """여러 조사 폴더. 목록에서 빠진 폴더의 색인은 따로 drop_root로 지운다."""
    rules = rules or Rules()
    prev_mode = store.get_meta("time_mode")
    mode_changed = prev_mode is not None and prev_mode != rules.time_mode
    total = ScanResult()
    for root in roots:
        part = scan_root(store, root, rules, cancel=cancel, progress=progress, recalculate=mode_changed)
        total.seen += part.seen
        total.written += part.written
        total.removed += part.removed
        total.skipped_dirs += part.skipped_dirs
        total.errors.extend(part.errors)
        if part.cancelled:
            total.cancelled = True
            break
    if not total.cancelled:
        store.set_meta("time_mode", rules.time_mode)
    return total


def default_roots(home=None, environ=None):
    """사용자 결정 1: 바탕 화면·문서 폴더(OneDrive로 옮겨진 경우 포함). 다운로드는 기본 제외."""
    home = home or os.path.expanduser("~")
    environ = os.environ if environ is None else environ
    candidates = []
    for name in ("Desktop", "Documents", "바탕 화면", "문서"):
        for base in (home, environ.get("OneDrive") or "", environ.get("OneDriveCommercial") or ""):
            if base:
                candidates.append(os.path.join(base, name))
    seen, roots = set(), []
    for path in candidates:
        key = path_key(path)
        if key not in seen and os.path.isdir(path):
            seen.add(key)
            roots.append(path)
    return roots
