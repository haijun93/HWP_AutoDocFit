"""사진뷰(PhotoAlbumService): 사진 라이브러리 백엔드 서비스 (MPLB1).

사진 폴더 등록·증분 스캔, EXIF 촬영일 및 회전 방향(Orientation 1~8) 보정 썸네일,
연·월 묶음 격자 목록, 즐겨찾기·태그·검색(SQLite DB),
공문 첨부용 일괄 처리(축소 & EXIF 회전 정방향 반영 사본 저장),
사진대지 HWPX 생성 연동 기반(2×2·2×3 표에 촬영일/설명 캡션 넣기)을 제공한다.

동시 썸네일 생성 제한(기본 2개)을 지키며, 클라우드 전용 파일(OneDrive 등)은 내용을 절대 읽지 않는다.
원본 사진 파일은 어떠한 경우에도 수정하거나 변경하지 않는다(읽기 전용).
"""
from __future__ import annotations

import base64
import copy
import io
import json
import logging
import os
import re
import sqlite3
import tempfile
import threading
import zipfile
from contextlib import closing
from datetime import datetime
from pathlib import Path
from typing import Any

from defusedxml import ElementTree as ET
from PIL import Image, ImageOps

from docfit_core.abbreviation_doc import default_base_hwpx
from docfit_core.batch_format import _serialize
from docfit_core.hwpx import open_hwpx_safely
from docfit_core.table_builder import _Header, _cell, _child, _element, _table, _tag
from docfit_core.timemachine import thumbs
from docfit_core.timemachine.rules import cloud_only, excluded_name

logger = logging.getLogger(__name__)

PHOTO_EXTENSIONS: frozenset[str] = frozenset(
    (".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff", ".heic")
)

MAX_SLOTS: int = 2
SLOT_TIMEOUT: float = 5.0
DEFAULT_THUMB_SIZE: int = 160

EXIF_ORIENTATION_TAG: int = 0x0112  # 274
EXIF_DATETIME_ORIGINAL: int = 0x9003
EXIF_DATETIME: int = 0x0132
EXIF_IFD: int = 0x8769

HP = "http://www.hancom.co.kr/hwpml/2011/paragraph"
HC = "http://www.hancom.co.kr/hwpml/2011/core"
HH = "http://www.hancom.co.kr/hwpml/2011/head"


def read_exif_metadata(path: Path | str) -> tuple[datetime | None, int]:
    """사진 파일에서 (촬영일시, EXIF 회전방향 1~8)을 안전하게 읽는다.

    실패하거나 EXIF가 없으면 (None, 1)을 반환한다.
    원본 파일은 절대 수정하지 않는다.
    """
    try:
        with Image.open(path) as img:
            exif = img.getexif()
            if not exif:
                return None, 1

            # EXIF 회전 방향 (Orientation: 1~8)
            orientation = int(exif.get(EXIF_ORIENTATION_TAG, 1))
            if orientation not in range(1, 9):
                orientation = 1

            # 촬영일시 (DateTimeOriginal 없으면 DateTime)
            dt_raw = None
            try:
                ifd = exif.get_ifd(EXIF_IFD)
                dt_raw = ifd.get(EXIF_DATETIME_ORIGINAL)
            except Exception:
                pass
            if not dt_raw:
                dt_raw = exif.get(EXIF_DATETIME)

            dt = None
            if isinstance(dt_raw, str):
                try:
                    dt = datetime.strptime(dt_raw.strip("\x00 ")[:19], "%Y:%m:%d %H:%M:%S")
                except ValueError:
                    pass

            return dt, orientation
    except Exception:
        return None, 1


def _norm_path(p: str | Path) -> str:
    """경로를 표준 절대/실제 경로 문자열로 정규화한다."""
    return str(Path(p).resolve())


class PhotoAlbumDb:
    """사용자 데이터 디렉터리 SQLite 기반 사진 메타데이터·태그·즐겨찾기 저장소."""

    def __init__(self, db_path: Path | str) -> None:
        self.db_path = Path(db_path).resolve()
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()
        self._init_db()

    def _connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(str(self.db_path), timeout=10.0)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys = ON")
        return conn

    def _init_db(self) -> None:
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.executescript(
                    """
                    CREATE TABLE IF NOT EXISTS registered_folders (
                        path TEXT PRIMARY KEY,
                        registered_at TEXT,
                        last_scanned_at TEXT
                    );

                    CREATE TABLE IF NOT EXISTS photos (
                        path TEXT PRIMARY KEY,
                        folder TEXT,
                        filename TEXT,
                        size INTEGER,
                        mtime REAL,
                        date_taken TEXT,
                        orientation INTEGER DEFAULT 1,
                        favorite INTEGER DEFAULT 0,
                        cloud INTEGER DEFAULT 0
                    );

                    CREATE TABLE IF NOT EXISTS tags (
                        photo_path TEXT,
                        tag TEXT,
                        PRIMARY KEY (photo_path, tag),
                        FOREIGN KEY (photo_path) REFERENCES photos(path) ON DELETE CASCADE
                    );

                    CREATE INDEX IF NOT EXISTS idx_photos_folder ON photos(folder);
                    CREATE INDEX IF NOT EXISTS idx_photos_date ON photos(date_taken);
                    CREATE INDEX IF NOT EXISTS idx_photos_favorite ON photos(favorite);
                    CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
                    """
                )

    def register_folder(self, folder_path: str) -> None:
        norm = _norm_path(folder_path)
        now = datetime.now().isoformat()
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute(
                    """
                    INSERT INTO registered_folders (path, registered_at, last_scanned_at)
                    VALUES (?, ?, ?)
                    ON CONFLICT(path) DO UPDATE SET registered_at=excluded.registered_at
                    """,
                    (norm, now, None),
                )

    def unregister_folder(self, folder_path: str) -> bool:
        norm = _norm_path(folder_path)
        with self._lock, closing(self._connect()) as conn:
            with conn:
                cur = conn.execute("DELETE FROM registered_folders WHERE path = ?", (norm,))
                return cur.rowcount > 0

    def list_folders(self) -> list[dict[str, Any]]:
        with self._lock, closing(self._connect()) as conn:
            rows = conn.execute(
                "SELECT path, registered_at, last_scanned_at FROM registered_folders ORDER BY path"
            ).fetchall()
            return [dict(r) for r in rows]

    def update_folder_scan_time(self, folder_path: str) -> None:
        norm = _norm_path(folder_path)
        now = datetime.now().isoformat()
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute(
                    "UPDATE registered_folders SET last_scanned_at = ? WHERE path = ?",
                    (now, norm),
                )

    def upsert_photo(
        self,
        *,
        path: str,
        folder: str,
        filename: str,
        size: int,
        mtime: float,
        date_taken: str,
        orientation: int = 1,
        cloud: int = 0,
    ) -> None:
        norm = _norm_path(path)
        folder_norm = _norm_path(folder)
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute(
                    """
                    INSERT INTO photos (path, folder, filename, size, mtime, date_taken, orientation, cloud)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(path) DO UPDATE SET
                        folder=excluded.folder,
                        filename=excluded.filename,
                        size=excluded.size,
                        mtime=excluded.mtime,
                        date_taken=excluded.date_taken,
                        orientation=excluded.orientation,
                        cloud=excluded.cloud
                    """,
                    (norm, folder_norm, filename, size, mtime, date_taken, orientation, cloud),
                )

    def get_photo(self, path: str) -> dict[str, Any] | None:
        norm = _norm_path(path)
        with self._lock, closing(self._connect()) as conn:
            row = conn.execute("SELECT * FROM photos WHERE path = ?", (norm,)).fetchone()
            if not row:
                return None
            data = dict(row)
            tags = [
                r["tag"]
                for r in conn.execute(
                    "SELECT tag FROM tags WHERE photo_path = ? ORDER BY tag", (norm,)
                ).fetchall()
            ]
            data["tags"] = tags
            data["favorite"] = bool(data.get("favorite", 0))
            data["cloud"] = bool(data.get("cloud", 0))
            return data

    def toggle_favorite(self, path: str) -> bool:
        norm = _norm_path(path)
        with self._lock, closing(self._connect()) as conn:
            with conn:
                row = conn.execute("SELECT favorite FROM photos WHERE path = ?", (norm,)).fetchone()
                if not row:
                    stat_size = 0
                    stat_mtime = 0.0
                    try:
                        st = os.stat(norm)
                        stat_size = st.st_size
                        stat_mtime = st.st_mtime
                    except OSError:
                        pass
                    dt_str = datetime.fromtimestamp(stat_mtime or datetime.now().timestamp()).strftime(
                        "%Y-%m-%d %H:%M:%S"
                    )
                    conn.execute(
                        """
                        INSERT INTO photos (path, folder, filename, size, mtime, date_taken, orientation, favorite, cloud)
                        VALUES (?, ?, ?, ?, ?, ?, 1, 1, 0)
                        """,
                        (norm, os.path.dirname(norm), os.path.basename(norm), stat_size, stat_mtime, dt_str),
                    )
                    return True
                new_val = 0 if row["favorite"] else 1
                conn.execute("UPDATE photos SET favorite = ? WHERE path = ?", (new_val, norm))
                return bool(new_val)

    def add_tag(self, path: str, tag: str) -> list[str]:
        norm = _norm_path(path)
        clean_tag = tag.strip()
        if not clean_tag:
            return self.get_tags(norm)
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute(
                    """
                    INSERT OR IGNORE INTO photos (path, folder, filename, size, mtime, date_taken, orientation, favorite, cloud)
                    VALUES (?, ?, ?, 0, 0, '', 1, 0, 0)
                    """,
                    (norm, os.path.dirname(norm), os.path.basename(norm)),
                )
                conn.execute(
                    "INSERT OR IGNORE INTO tags (photo_path, tag) VALUES (?, ?)",
                    (norm, clean_tag),
                )
        return self.get_tags(norm)

    def remove_tag(self, path: str, tag: str) -> list[str]:
        norm = _norm_path(path)
        clean_tag = tag.strip()
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute("DELETE FROM tags WHERE photo_path = ? AND tag = ?", (norm, clean_tag))
        return self.get_tags(norm)

    def get_tags(self, path: str) -> list[str]:
        norm = _norm_path(path)
        with self._lock, closing(self._connect()) as conn:
            rows = conn.execute(
                "SELECT tag FROM tags WHERE photo_path = ? ORDER BY tag", (norm,)
            ).fetchall()
            return [r["tag"] for r in rows]

    def set_tags(self, path: str, tags: list[str]) -> list[str]:
        norm = _norm_path(path)
        cleaned = sorted({t.strip() for t in tags if t and t.strip()})
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute(
                    """
                    INSERT OR IGNORE INTO photos (path, folder, filename, size, mtime, date_taken, orientation, favorite, cloud)
                    VALUES (?, ?, ?, 0, 0, '', 1, 0, 0)
                    """,
                    (norm, os.path.dirname(norm), os.path.basename(norm)),
                )
                conn.execute("DELETE FROM tags WHERE photo_path = ?", (norm,))
                for t in cleaned:
                    conn.execute("INSERT INTO tags (photo_path, tag) VALUES (?, ?)", (norm, t))
        return cleaned

    def list_photos(self, folder: str | None = None) -> list[dict[str, Any]]:
        with self._lock, closing(self._connect()) as conn:
            if folder:
                folder_norm = _norm_path(folder)
                rows = conn.execute(
                    "SELECT * FROM photos WHERE folder = ? ORDER BY date_taken DESC, filename ASC",
                    (folder_norm,),
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT * FROM photos ORDER BY date_taken DESC, filename ASC"
                ).fetchall()

            tag_rows = conn.execute("SELECT photo_path, tag FROM tags ORDER BY tag").fetchall()
            tag_map: dict[str, list[str]] = {}
            for r in tag_rows:
                tag_map.setdefault(r["photo_path"], []).append(r["tag"])

            results = []
            for r in rows:
                item = dict(r)
                p = item["path"]
                item["tags"] = tag_map.get(p, [])
                item["favorite"] = bool(item.get("favorite", 0))
                item["cloud"] = bool(item.get("cloud", 0))
                results.append(item)
            return results

    def search(
        self,
        query: str = "",
        *,
        folder: str | None = None,
        tag: str | None = None,
        start_date: str | None = None,
        end_date: str | None = None,
        favorite_only: bool = False,
    ) -> list[dict[str, Any]]:
        conditions: list[str] = []
        params: list[Any] = []

        if folder:
            conditions.append("p.folder = ?")
            params.append(_norm_path(folder))

        if favorite_only:
            conditions.append("p.favorite = 1")

        if start_date:
            conditions.append("p.date_taken >= ?")
            params.append(start_date.strip())

        if end_date:
            ed = end_date.strip()
            if len(ed) == 10:
                ed = ed + " 23:59:59"
            conditions.append("p.date_taken <= ?")
            params.append(ed)

        if query:
            q_clean = f"%{query.strip().lower()}%"
            conditions.append("(LOWER(p.filename) LIKE ? OR LOWER(p.path) LIKE ?)")
            params.extend([q_clean, q_clean])

        if tag:
            conditions.append(
                "EXISTS (SELECT 1 FROM tags t WHERE t.photo_path = p.path AND LOWER(t.tag) = ?)"
            )
            params.append(tag.strip().lower())

        where_clause = ("WHERE " + " AND ".join(conditions)) if conditions else ""
        sql = f"SELECT p.* FROM photos p {where_clause} ORDER BY p.date_taken DESC, p.filename ASC"

        with self._lock, closing(self._connect()) as conn:
            rows = conn.execute(sql, params).fetchall()
            tag_rows = conn.execute("SELECT photo_path, tag FROM tags ORDER BY tag").fetchall()
            tag_map: dict[str, list[str]] = {}
            for r in tag_rows:
                tag_map.setdefault(r["photo_path"], []).append(r["tag"])

            results = []
            for r in rows:
                item = dict(r)
                p = item["path"]
                item["tags"] = tag_map.get(p, [])
                item["favorite"] = bool(item.get("favorite", 0))
                item["cloud"] = bool(item.get("cloud", 0))
                results.append(item)
            return results


class PhotoAlbumService:
    """사진뷰 백엔드 서비스(MPLB1)."""

    def __init__(
        self,
        db_path: Path | str | None = None,
        thumb_dir: Path | str | None = None,
        max_slots: int = MAX_SLOTS,
        opener: Any | None = None,
    ) -> None:
        if db_path is None:
            try:
                from .kagent_client import data_dir
                db_path = data_dir() / "photo_album.db"
            except Exception:
                db_path = Path.home() / ".hwp_autodocfit" / "photo_album.db"

        if thumb_dir is None:
            try:
                from .kagent_client import data_dir
                thumb_dir = data_dir() / "thumbs"
            except Exception:
                thumb_dir = Path.home() / ".hwp_autodocfit" / "thumbs"

        self.db = PhotoAlbumDb(db_path)
        self.cache = thumbs.ThumbnailCache(thumb_dir)
        self._slots = threading.BoundedSemaphore(max_slots)
        self.max_slots = max_slots
        self.opener = opener

    # ---- 1. 폴더 등록 및 증분 스캔 ----

    def register_folder(self, folder: str | Path) -> dict[str, Any]:
        p = Path(folder).resolve()
        if not p.is_dir():
            return {"ok": False, "code": "DIR_NOT_FOUND", "message": f"폴더가 존재하지 않습니다: {folder}"}
        self.db.register_folder(str(p))
        return {"ok": True, "path": str(p)}

    def unregister_folder(self, folder: str | Path) -> dict[str, Any]:
        p = Path(folder).resolve()
        removed = self.db.unregister_folder(str(p))
        return {"ok": True, "removed": removed, "path": str(p)}

    def list_registered_folders(self) -> dict[str, Any]:
        folders = self.db.list_folders()
        return {"ok": True, "folders": folders}

    def scan_folder(
        self,
        folder: str | Path,
        *,
        recursive: bool = False,
    ) -> dict[str, Any]:
        """지정 폴더의 사진을 증분 스캔하여 DB에 등록/갱신한다.

        클라우드 전용 파일은 내용을 절대 읽지 않는다.
        원본 파일은 절대 수정하지 않는다.
        """
        folder_path = Path(folder).resolve()
        if not folder_path.is_dir():
            return {"ok": False, "code": "DIR_NOT_FOUND", "message": f"디렉터리가 아닙니다: {folder}"}

        scanned_count = 0
        added_count = 0
        updated_count = 0

        # 기존 DB의 사진 정보를 캐싱하여 증분 비교 최적화
        existing_photos = {p["path"]: p for p in self.db.list_photos(str(folder_path))}

        def scan_dir(target_dir: Path) -> None:
            nonlocal scanned_count, added_count, updated_count
            try:
                with os.scandir(target_dir) as iterator:
                    for entry in iterator:
                        try:
                            name = entry.name
                            if name.startswith(".") or excluded_name(name):
                                continue

                            if entry.is_dir(follow_symlinks=False):
                                if recursive:
                                    scan_dir(Path(entry.path))
                                continue

                            if not entry.is_file(follow_symlinks=False):
                                continue

                            ext = os.path.splitext(name)[1].lower()
                            if ext not in PHOTO_EXTENSIONS:
                                continue

                            scanned_count += 1
                            stat_info = entry.stat(follow_symlinks=False)
                            attrs = getattr(stat_info, "st_file_attributes", 0)
                            is_cloud = cloud_only(attrs)

                            mtime = stat_info.st_mtime
                            size = stat_info.st_size
                            abs_path = _norm_path(entry.path)

                            # 증분 검사: 기존에 있고 mtime/size/cloud 상태가 같으면 재추출 생략
                            cached = existing_photos.get(abs_path)
                            if (
                                cached
                                and abs(cached["mtime"] - mtime) < 0.001
                                and cached["size"] == size
                                and cached["cloud"] == is_cloud
                            ):
                                continue

                            if is_cloud:
                                # 클라우드 전용 파일은 내용 읽기 절대 금지!
                                orientation = 1
                                dt_str = datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M:%S")
                            else:
                                dt, orientation = read_exif_metadata(entry.path)
                                if dt is not None:
                                    dt_str = dt.strftime("%Y-%m-%d %H:%M:%S")
                                else:
                                    dt_str = datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M:%S")

                            self.db.upsert_photo(
                                path=abs_path,
                                folder=_norm_path(target_dir),
                                filename=name,
                                size=size,
                                mtime=mtime,
                                date_taken=dt_str,
                                orientation=orientation,
                                cloud=1 if is_cloud else 0,
                            )
                            if cached:
                                updated_count += 1
                            else:
                                added_count += 1
                        except OSError:
                            continue
            except OSError:
                pass

        scan_dir(folder_path)
        self.db.update_folder_scan_time(str(folder_path))

        return {
            "ok": True,
            "folder": str(folder_path),
            "scanned": scanned_count,
            "added": added_count,
            "updated": updated_count,
        }

    # ---- 2. 연·월 묶음 목록 및 검색 ----

    def list_by_date(self, folder: str | Path | None = None) -> list[dict[str, Any]]:
        """촬영일(또는 수정일) 기준 연·월 묶음 목록을 반환한다."""
        folder_str = str(Path(folder).resolve()) if folder else None
        photos = self.db.list_photos(folder_str)

        # 연·월별로 그룹화
        groups: dict[tuple[int, int], list[dict[str, Any]]] = {}
        for p in photos:
            dt_str = p.get("date_taken") or ""
            year, month = 0, 0
            if len(dt_str) >= 7:
                try:
                    year = int(dt_str[:4])
                    month = int(dt_str[5:7])
                except ValueError:
                    pass

            if year == 0 or month == 0:
                # fallback mtime
                try:
                    dt = datetime.fromtimestamp(p.get("mtime", 0))
                    year, month = dt.year, dt.month
                except Exception:
                    year, month = 1970, 1

            key = (year, month)
            item = {
                "path": p["path"],
                "name": p["filename"],
                "date": p["date_taken"],
                "size": p["size"],
                "favorite": p["favorite"],
                "tags": p["tags"],
                "orientation": p["orientation"],
                "cloud": p["cloud"],
                "has_thumb": False,
            }
            # 썸네일 캐시 존재 여부 검사
            try:
                st = os.stat(p["path"])
                thumb_key = self.cache.key(p["path"], st.st_mtime_ns, st.st_size)
                item["thumb_key"] = thumb_key
                item["has_thumb"] = bool(not p["cloud"] and self.cache.get(thumb_key) is not None)
            except OSError:
                item["thumb_key"] = ""
                item["has_thumb"] = False

            groups.setdefault(key, []).append(item)

        # 연·월 내림차순 정렬
        sorted_keys = sorted(groups.keys(), key=lambda ym: (ym[0], ym[1]), reverse=True)
        results: list[dict[str, Any]] = []
        for ym in sorted_keys:
            items = groups[ym]
            # 월 내부에서도 최신 날짜순 정렬
            items.sort(key=lambda x: x["date"], reverse=True)
            results.append({
                "year": ym[0],
                "month": ym[1],
                "items": items,
            })
        return results

    def search(
        self,
        query: str = "",
        *,
        folder: str | Path | None = None,
        tag: str | None = None,
        start_date: str | None = None,
        end_date: str | None = None,
        favorite_only: bool = False,
    ) -> list[dict[str, Any]]:
        """파일명, 태그, 날짜 범위, 즐겨찾기로 사진을 검색한다."""
        folder_str = str(Path(folder).resolve()) if folder else None
        photos = self.db.search(
            query=query,
            folder=folder_str,
            tag=tag,
            start_date=start_date,
            end_date=end_date,
            favorite_only=favorite_only,
        )
        results: list[dict[str, Any]] = []
        for p in photos:
            item = {
                "path": p["path"],
                "name": p["filename"],
                "date": p["date_taken"],
                "size": p["size"],
                "favorite": p["favorite"],
                "tags": p["tags"],
                "orientation": p["orientation"],
                "cloud": p["cloud"],
                "has_thumb": False,
            }
            try:
                st = os.stat(p["path"])
                thumb_key = self.cache.key(p["path"], st.st_mtime_ns, st.st_size)
                item["thumb_key"] = thumb_key
                item["has_thumb"] = bool(not p["cloud"] and self.cache.get(thumb_key) is not None)
            except OSError:
                item["thumb_key"] = ""
                item["has_thumb"] = False
            results.append(item)
        return results

    # ---- 3. 즐겨찾기 및 태그 관리 ----

    def toggle_favorite(self, path: str | Path) -> dict[str, Any]:
        p = str(Path(path).resolve())
        new_val = self.db.toggle_favorite(p)
        return {"ok": True, "path": p, "favorite": new_val}

    def add_tag(self, path: str | Path, tag: str) -> dict[str, Any]:
        p = str(Path(path).resolve())
        tags = self.db.add_tag(p, tag)
        return {"ok": True, "path": p, "tags": tags}

    def remove_tag(self, path: str | Path, tag: str) -> dict[str, Any]:
        p = str(Path(path).resolve())
        tags = self.db.remove_tag(p, tag)
        return {"ok": True, "path": p, "tags": tags}

    def get_tags(self, path: str | Path) -> dict[str, Any]:
        p = str(Path(path).resolve())
        tags = self.db.get_tags(p)
        return {"ok": True, "path": p, "tags": tags}

    # ---- 4. EXIF 회전 보정 썸네일 생성 ----

    def get_thumbnail(
        self,
        path: str | Path,
        *,
        size: int = DEFAULT_THUMB_SIZE,
        as_base64: bool = False,
        timeout: float = SLOT_TIMEOUT,
    ) -> dict[str, Any]:
        """EXIF 회전 방향(Orientation 1~8)을 보정한 정방향 썸네일을 반환한다."""
        def format_result(res: dict[str, Any]) -> dict[str, Any]:
            if as_base64 and res.get("data") is not None and isinstance(res["data"], bytes):
                res["data"] = base64.b64encode(res["data"]).decode("ascii")
            return res

        path_obj = Path(path).resolve()
        if not path_obj.is_file():
            return format_result({"ok": False, "code": "FILE_NOT_FOUND", "message": "파일을 찾을 수 없습니다."})

        try:
            stat_info = path_obj.stat(follow_symlinks=False)
        except OSError as exc:
            return format_result({"ok": False, "code": "STAT_FAILED", "message": str(exc)})

        attrs = getattr(stat_info, "st_file_attributes", 0)
        if cloud_only(attrs):
            return format_result({
                "ok": False,
                "code": "CLOUD_ONLY",
                "message": "클라우드 전용 파일은 내용을 읽지 않습니다.",
            })

        key = self.cache.key(path_obj, stat_info.st_mtime_ns, stat_info.st_size)

        # 1. 디스크 캐시 조회
        cached = self.cache.get(key)
        if cached is not None:
            return format_result({
                "ok": True,
                "type": "cached",
                "data": cached,
                "key": key,
            })

        # 2. 동시 생성 슬롯 확보 (동시 생성 2~3개 제한)
        acquired = self._slots.acquire(timeout=timeout)
        if not acquired:
            return format_result({
                "ok": False,
                "code": "BUSY",
                "message": "썸네일 생성 슬롯이 혼잡합니다. 잠시 후 다시 시도해 주세요.",
            })

        try:
            # 슬롯 획득 후 다른 스레드가 생성했는지 재확인
            cached = self.cache.get(key)
            if cached is not None:
                return format_result({
                    "ok": True,
                    "type": "cached",
                    "data": cached,
                    "key": key,
                })

            # 원본은 절대 수정하지 않고 읽기 전용으로 열기
            with Image.open(path_obj) as img:
                # EXIF 회전 방향 반영 (세로 사진이 눕지 않게 정방향 변환)
                transposed = ImageOps.exif_transpose(img)
                transposed.thumbnail((size, size), Image.Resampling.LANCZOS)

                # JPEG 저장 시 RGB 변환
                if transposed.mode not in ("RGB", "L"):
                    transposed = transposed.convert("RGB")

                output = io.BytesIO()
                transposed.save(output, format="JPEG", quality=thumbs.QUALITY)
                thumb_bytes = output.getvalue()

            self.cache.put(key, thumb_bytes)
            return format_result({
                "ok": True,
                "type": "image",
                "data": thumb_bytes,
                "key": key,
            })
        except Exception as exc:
            logger.warning("썸네일 생성 실패 (%s): %s", path_obj.name, exc)
            return format_result({"ok": False, "code": "THUMB_FAILED", "message": str(exc)})
        finally:
            self._slots.release()

    def clear_cache(self) -> int:
        """썸네일 디스크 캐시를 비운다."""
        return self.cache.clear()

    # ---- 5. 공문 첨부용 일괄 처리 ----

    def batch_export(
        self,
        paths: list[str | Path],
        output_dir: str | Path,
        *,
        max_width: int = 1280,
        max_height: int | None = None,
        quality: int = 85,
        fmt: str = "JPEG",
        suffix: str = "_docfit",
        naming: str = "suffix",
    ) -> dict[str, Any]:
        """고른 사진들을 최대 폭/품질 축소·EXIF 회전 정방향을 반영해 사본으로 저장한다.

        원본 사진 파일은 절대 수정하지 않는다.
        """
        out_dir = Path(output_dir).resolve()
        out_dir.mkdir(parents=True, exist_ok=True)

        target_fmt = fmt.upper()
        if target_fmt not in ("JPEG", "JPG", "PNG"):
            target_fmt = "JPEG"
        ext = ".jpg" if target_fmt in ("JPEG", "JPG") else ".png"

        results: list[dict[str, Any]] = []
        for idx, p in enumerate(paths):
            src_path = Path(p).resolve()
            if not src_path.is_file():
                results.append({"ok": False, "source_path": str(src_path), "message": "파일을 찾을 수 없습니다."})
                continue

            try:
                # 대상 파일명 결정
                if naming == "numbered":
                    filename = f"photo_{idx + 1:03d}{ext}"
                elif naming == "keep":
                    filename = f"{src_path.stem}{ext}"
                else:  # "suffix"
                    filename = f"{src_path.stem}{suffix}{ext}"

                dest_path = out_dir / filename

                # 원본 읽기 및 정방향 회전
                with Image.open(src_path) as img:
                    transposed = ImageOps.exif_transpose(img)
                    cur_w, cur_h = transposed.size

                    max_h = max_height if max_height is not None else max_width
                    if cur_w > max_width or cur_h > max_h:
                        transposed.thumbnail((max_width, max_h), Image.Resampling.LANCZOS)

                    final_w, final_h = transposed.size

                    # 저장
                    if target_fmt in ("JPEG", "JPG"):
                        if transposed.mode not in ("RGB", "L"):
                            transposed = transposed.convert("RGB")
                        transposed.save(dest_path, format="JPEG", quality=quality)
                    else:
                        transposed.save(dest_path, format="PNG", optimize=True)

                dest_size = dest_path.stat().st_size
                results.append({
                    "ok": True,
                    "source_path": str(src_path),
                    "output_path": str(dest_path),
                    "width": final_w,
                    "height": final_h,
                    "size": dest_size,
                })
            except Exception as exc:
                results.append({"ok": False, "source_path": str(src_path), "message": str(exc)})

        return {"ok": True, "output_dir": str(out_dir), "results": results}

    # ---- 6. 사진대지 HWPX 생성 연동 기반 ----

    def build_photo_sheet_hwpx(
        self,
        items: list[dict[str, Any]],
        target_path: str | Path,
        *,
        layout: str = "2x2",
        title: str = "사 진 대 지",
        base_hwpx: str | Path | None = None,
    ) -> dict[str, Any]:
        """2×2 또는 2×3 표에 사진과 촬영일/설명 캡션을 배치한 사진대지 HWPX 문서를 생성한다."""
        if not items:
            return {"ok": False, "message": "사진 목록이 비어 있습니다."}

        target = Path(target_path).resolve()
        if target.suffix.lower() != ".hwpx":
            return {"ok": False, "message": "출력 경로는 .hwpx 파일이어야 합니다."}

        base = Path(base_hwpx) if base_hwpx else default_base_hwpx()
        if not base or not base.is_file():
            return {"ok": False, "message": "기본 서식 HWPX 문서를 찾지 못했습니다."}

        cols_per_row = 2
        items_per_page = 4 if layout == "2x2" else 6
        target.parent.mkdir(parents=True, exist_ok=True)

        try:
            with open_hwpx_safely(base) as z:
                contents = {info.filename: z.read(info.filename) for info in z.infolist()}

            sections = sorted(n for n in contents if re.fullmatch(r"Contents/section\d+\.xml", n))
            if not sections or "Contents/header.xml" not in contents or "Contents/content.hpf" not in contents:
                return {"ok": False, "message": "바탕 HWPX 구조가 올바르지 않습니다."}

            header_bytes = contents["Contents/header.xml"]
            section_bytes = contents[sections[0]]
            hpf_bytes = contents["Contents/content.hpf"]

            header = ET.fromstring(header_bytes)
            section = ET.fromstring(section_bytes)
            hpf = ET.fromstring(hpf_bytes)

            pool = _Header(header)
            page = next(x for x in section.iter() if _tag(x) == "pagePr")
            margin = _child(page, "margin")
            body_width = int(page.get("width")) - int(margin.get("left")) - int(margin.get("right"))

            # 서식 스타일 준비
            title_char = pool.char({"font": "한컴돋움", "size": 16.0, "bold": True, "color": "#000000"})
            caption_char = pool.char({"font": "한컴돋움", "size": 10.0, "bold": False, "color": "#111111"})
            center_para = pool.center_para()

            line = ("SOLID", "0.12 mm", "#000000")
            cell_border = pool.border({"left": line, "right": line, "top": line, "bottom": line})

            # 본문 초기화: 첫 문단의 구역 설정만 남김
            paragraphs = [p for p in section if _tag(p) == "p"]
            first_p = paragraphs[0]
            for p in paragraphs[1:]:
                section.remove(p)
            for child in list(first_p):
                if _tag(child) == "run":
                    for item in list(child):
                        if _tag(item) not in ("secPr", "ctrl"):
                            child.remove(item)
                    if not len(child):
                        first_p.remove(child)
                elif _tag(child) == "linesegarray":
                    first_p.remove(child)

            # 제목 문단
            first_p.set("paraPrIDRef", center_para)
            t_run = _element(first_p, "run", charPrIDRef=title_char)
            _element(t_run, "t").text = title

            # 빈 줄 하나
            _element(section, "p", id=0, paraPrIDRef=center_para, styleIDRef=0, pageBreak=0, columnBreak=0, merged=0)

            # 사진 준비 & BinData 압축
            bin_images: dict[str, bytes] = {}
            photo_entries: list[dict[str, Any]] = []
            manifest = next((x for x in hpf.iter() if _tag(x) == "manifest"), None)

            for i, itm in enumerate(items):
                p_path = Path(itm.get("path") or "").resolve()
                caption_text = str(itm.get("caption") or "").strip()
                date_text = str(itm.get("date") or "").strip()

                bin_id = f"photo_{i + 1}"
                bin_href = f"BinData/{bin_id}.jpg"

                if p_path.is_file():
                    with Image.open(p_path) as raw_img:
                        trans = ImageOps.exif_transpose(raw_img)
                        # 사진대지용으로 최대 폭 800px 정도로 축소
                        if trans.width > 800 or trans.height > 800:
                            trans.thumbnail((800, 800), Image.Resampling.LANCZOS)
                        if trans.mode not in ("RGB", "L"):
                            trans = trans.convert("RGB")
                        buf = io.BytesIO()
                        trans.save(buf, format="JPEG", quality=85)
                        img_bytes = buf.getvalue()
                        bin_images[bin_href] = img_bytes
                        if not date_text:
                            dt, _ = read_exif_metadata(p_path)
                            if dt:
                                date_text = dt.strftime("%Y-%m-%d %H:%M:%S")

                if not date_text:
                    date_text = datetime.now().strftime("%Y-%m-%d %H:%M:%S")

                photo_entries.append({
                    "bin_id": bin_id,
                    "date": date_text,
                    "caption": caption_text,
                    "has_image": bin_href in bin_images,
                })

                # manifest 등록
                if manifest is not None and bin_href in bin_images:
                    _element(manifest, "item", _ns="http://www.idpf.org/2007/opf/", id=bin_id, href=bin_href, **{"media-type": "image/jpeg"})

            # 표 구성
            # 사진 행 + 캡션 행 = 블록당 2개 행
            num_photos = len(photo_entries)
            num_blocks = (num_photos + cols_per_row - 1) // cols_per_row
            total_rows = num_blocks * 2
            col_width = body_width // cols_per_row

            photo_row_height = 8000  # HwpUnit (약 28mm)
            caption_row_height = 2000  # HwpUnit (약 7mm)
            total_table_height = (photo_row_height + caption_row_height) * num_blocks

            holder_p = _element(section, "p", id=0, paraPrIDRef=center_para, styleIDRef=0, pageBreak=0, columnBreak=0, merged=0)
            holder_run = _element(holder_p, "run", charPrIDRef=caption_char)

            tbl = _table(holder_run, total_rows, cols_per_row, body_width, total_table_height, cell_border)

            for block_idx in range(num_blocks):
                # 1. 사진 행
                tr_photo = _element(tbl, "tr")
                for c in range(cols_per_row):
                    idx = block_idx * cols_per_row + c
                    tc = _element(tr_photo, "tc", name="", header=0, hasMargin=1, protect=0, editable=0, dirty=0, borderFillIDRef=cell_border)
                    sub = _element(tc, "subList", id="", textDirection="HORIZONTAL", lineWrap="BREAK", vertAlign="CENTER",
                                   linkListIDRef=0, linkListNextIDRef=0, textWidth=0, textHeight=0, hasTextRef=0, hasNumRef=0)
                    p = _element(sub, "p", id=0, paraPrIDRef=center_para, styleIDRef=0, pageBreak=0, columnBreak=0, merged=0)
                    run = _element(p, "run", charPrIDRef=caption_char)

                    if idx < num_photos and photo_entries[idx]["has_image"]:
                        # 이미지 요소 생성 (hp:pic)
                        entry = photo_entries[idx]
                        pic_w = col_width - 600
                        pic_h = photo_row_height - 600
                        _make_hp_pic(run, bin_item_id=entry["bin_id"], width=pic_w, height=pic_h, pic_id=idx + 1)
                    else:
                        _element(run, "t").text = "(사진 없음)" if idx < num_photos else ""

                    _element(tc, "cellAddr", colAddr=c, rowAddr=block_idx * 2)
                    _element(tc, "cellSpan", colSpan=1, rowSpan=1)
                    _element(tc, "cellSz", width=col_width, height=photo_row_height)
                    _element(tc, "cellMargin", left=141, right=141, top=141, bottom=141)

                # 2. 캡션 행
                tr_caption = _element(tbl, "tr")
                for c in range(cols_per_row):
                    idx = block_idx * cols_per_row + c
                    if idx < num_photos:
                        entry = photo_entries[idx]
                        cap_lines = [f"촬영일시: {entry['date']}"]
                        if entry["caption"]:
                            cap_lines.append(f"설명: {entry['caption']}")
                        caption_str = "\n".join(cap_lines)
                    else:
                        caption_str = ""

                    _cell(
                        tr_caption,
                        col=c,
                        row=block_idx * 2 + 1,
                        width=col_width,
                        height=caption_row_height,
                        border=cell_border,
                        para=center_para,
                        char=caption_char,
                        text=caption_str,
                    )

            _element(holder_run, "t")

            # HWPX 저장
            contents["Contents/header.xml"] = _serialize(header, header_bytes)
            contents[sections[0]] = _serialize(section, section_bytes)
            contents["Contents/content.hpf"] = _serialize(hpf, hpf_bytes)
            for href, img_data in bin_images.items():
                contents[href] = img_data

            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as out:
                if "mimetype" in contents:
                    out.writestr("mimetype", contents.pop("mimetype"), compress_type=zipfile.ZIP_STORED)
                for name, data in contents.items():
                    out.writestr(name, data)

            temp_target = None
            try:
                with tempfile.NamedTemporaryFile(dir=target.parent, prefix=".photo-", suffix=".hwpx.tmp", delete=False) as tmp:
                    temp_target = Path(tmp.name)
                    tmp.write(buffer.getvalue())

                # defusedxml 검증
                _verify_photo_sheet_hwpx(temp_target, num_photos)
                os.replace(temp_target, target)
                temp_target = None
            finally:
                if temp_target and temp_target.exists():
                    temp_target.unlink(missing_ok=True)

            return {"ok": True, "path": str(target), "photo_count": num_photos}

        except Exception as exc:
            logger.exception("사진대지 HWPX 생성 실패: %s", exc)
            return {"ok": False, "message": f"사진대지 생성 실패: {exc}"}

    # ---- 7. 디스패처 ----

    def command(self, action: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        """브리지 명령 디스패처."""
        payload = payload or {}
        if not isinstance(payload, dict):
            return {"ok": False, "code": "INVALID_INPUT", "message": "입력을 확인해 주세요."}

        try:
            if action == "register_folder":
                folder = payload.get("folder")
                if not folder:
                    return {"ok": False, "message": "folder 경로가 필요합니다."}
                return self.register_folder(folder)

            if action == "unregister_folder":
                folder = payload.get("folder")
                if not folder:
                    return {"ok": False, "message": "folder 경로가 필요합니다."}
                return self.unregister_folder(folder)

            if action == "list_folders":
                return self.list_registered_folders()

            if action == "scan":
                folder = payload.get("folder")
                if not folder:
                    return {"ok": False, "message": "folder 경로가 필요합니다."}
                return self.scan_folder(folder, recursive=bool(payload.get("recursive", False)))

            if action == "list_by_date" or action == "list":
                folder = payload.get("folder")
                items = self.list_by_date(folder)
                return {"ok": True, "groups": items, "folder": folder}

            if action == "search":
                res = self.search(
                    query=payload.get("query", ""),
                    folder=payload.get("folder"),
                    tag=payload.get("tag"),
                    start_date=payload.get("start_date"),
                    end_date=payload.get("end_date"),
                    favorite_only=bool(payload.get("favorite_only", False)),
                )
                return {"ok": True, "results": res}

            if action == "thumbnail":
                path = payload.get("path")
                if not path:
                    return {"ok": False, "message": "path가 필요합니다."}
                return self.get_thumbnail(
                    path,
                    size=int(payload.get("size", DEFAULT_THUMB_SIZE)),
                    as_base64=bool(payload.get("as_base64", True)),
                    timeout=float(payload.get("timeout", SLOT_TIMEOUT)),
                )

            if action == "toggle_favorite":
                path = payload.get("path")
                if not path:
                    return {"ok": False, "message": "path가 필요합니다."}
                return self.toggle_favorite(path)

            if action == "add_tag":
                path = payload.get("path")
                tag = payload.get("tag")
                if not path or not tag:
                    return {"ok": False, "message": "path와 tag가 필요합니다."}
                return self.add_tag(path, tag)

            if action == "remove_tag":
                path = payload.get("path")
                tag = payload.get("tag")
                if not path or not tag:
                    return {"ok": False, "message": "path와 tag가 필요합니다."}
                return self.remove_tag(path, tag)

            if action == "get_tags":
                path = payload.get("path")
                if not path:
                    return {"ok": False, "message": "path가 필요합니다."}
                return self.get_tags(path)

            if action == "batch_export":
                paths = payload.get("paths")
                output_dir = payload.get("output_dir")
                if not paths or not output_dir:
                    return {"ok": False, "message": "paths 목록과 output_dir이 필요합니다."}
                return self.batch_export(
                    paths,
                    output_dir,
                    max_width=int(payload.get("max_width", 1280)),
                    max_height=payload.get("max_height"),
                    quality=int(payload.get("quality", 85)),
                    fmt=str(payload.get("fmt", "JPEG")),
                    suffix=str(payload.get("suffix", "_docfit")),
                    naming=str(payload.get("naming", "suffix")),
                )

            if action in ("build_photo_sheet", "create_photo_sheet"):
                items = payload.get("items")
                output_path = payload.get("output_path")
                if not items or not output_path:
                    return {"ok": False, "message": "items 목록과 output_path가 필요합니다."}
                return self.build_photo_sheet_hwpx(
                    items,
                    output_path,
                    layout=str(payload.get("layout", "2x2")),
                    title=str(payload.get("title", "사 진 대 지")),
                    base_hwpx=payload.get("base_hwpx"),
                )

            if action == "clear_cache":
                cleared = self.clear_cache()
                return {"ok": True, "cleared": cleared}

            return {"ok": False, "code": "NOT_AVAILABLE", "message": f"지원하지 않는 명령입니다: {action}"}
        except Exception as exc:
            logger.exception("PhotoAlbumService command 실행 오류: %s", exc)
            return {"ok": False, "code": "COMMAND_FAILED", "message": str(exc)}


def _make_hp_pic(
    run: Any,
    *,
    bin_item_id: str,
    width: int,
    height: int,
    org_w: int | None = None,
    org_h: int | None = None,
    pic_id: int = 1,
) -> Any:
    """한/글 OWPML run 안에 hp:pic 개체를 생성한다."""
    ow = org_w or width
    oh = org_h or height
    pic = _element(
        run,
        "pic",
        id=str(pic_id),
        zOrder=str(pic_id),
        numberingType="PICTURE",
        textWrap="IN_FRONT_OF_TEXT",
        textFlow="BOTH_SIDES",
        lock="0",
        dropcapstyle="None",
        href="",
        groupLevel="0",
        instid=str(pic_id + 1000),
        reverse="0",
    )
    _element(pic, "offset", x="0", y="0")
    _element(pic, "orgSz", width=str(ow), height=str(oh))
    _element(pic, "curSz", width=str(width), height=str(height))
    _element(pic, "flip", horizontal="0", vertical="0")
    _element(pic, "rotationInfo", angle="0")
    rendering = _element(pic, "renderingInfo")
    _element(rendering, "transMatrix", _ns=HC, e1="1", e2="0", e3="0", e4="0", e5="1", e6="0")
    _element(
        rendering,
        "scaMatrix",
        _ns=HC,
        e1=f"{width / ow:.6f}",
        e2="0",
        e3="0",
        e4="0",
        e5=f"{height / oh:.6f}",
        e6="0",
    )
    _element(rendering, "rotMatrix", _ns=HC, e1="1", e2="0", e3="0", e4="0", e5="1", e6="0")
    rect = _element(pic, "imgRect")
    for index, (x, y) in enumerate(((0, 0), (ow, 0), (ow, oh), (0, oh))):
        _element(rect, f"pt{index}", _ns=HC, x=str(x), y=str(y))
    _element(pic, "imgClip", left="0", right=str(ow), top="0", bottom=str(oh))
    _element(pic, "inMargin", left="0", right="0", top="0", bottom="0")
    _element(pic, "imgDim", dimwidth=str(ow), dimheight=str(oh))
    _element(pic, "img", _ns=HC, binaryItemIDRef=bin_item_id, bright="0", contrast="0", effect="REAL_PIC", alpha="0")
    _element(pic, "effects")
    _element(pic, "sz", width=str(width), widthRelTo="ABSOLUTE", height=str(height), heightRelTo="ABSOLUTE", protect="0")
    _element(
        pic,
        "pos",
        treatAsChar="1",
        affectLSpacing="0",
        flowWithText="1",
        allowOverlap="1",
        holdAnchorAndSO="0",
        vertRelTo="PARA",
        horzRelTo="COLUMN",
        vertAlign="TOP",
        horzAlign="LEFT",
        vertOffset="0",
        horzOffset="0",
    )
    _element(pic, "outMargin", left="0", right="0", top="0", bottom="0")
    return pic


def _verify_photo_sheet_hwpx(path: Path, expected_photos: int) -> None:
    """defusedxml을 사용하여 생성된 사진대지 HWPX의 무결성을 검증한다."""
    with open_hwpx_safely(path) as archive:
        names = archive.namelist()
        if "mimetype" not in names or archive.read("mimetype") != b"application/hwp+zip":
            raise ValueError("HWPX mimetype이 올바르지 않습니다.")
        if "Contents/header.xml" not in names or "Contents/section0.xml" not in names:
            raise ValueError("HWPX 필수 XML 요소가 없습니다.")

        ET.fromstring(archive.read("Contents/header.xml"))
        root = ET.fromstring(archive.read("Contents/section0.xml"))

        # 표(tbl) 검증
        tables = [x for x in root.iter() if _tag(x) == "tbl"]
        if not tables:
            raise ValueError("사진대지 표(tbl)가 생성되지 않았습니다.")
