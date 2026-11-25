"""사진뷰(PhotoAlbumService): 사진 라이브러리 백엔드 서비스 (MPLB1).

사진 폴더 등록·증분 스캔, EXIF 촬영일 및 회전 방향(Orientation 1~8) 보정 썸네일,
연·월 묶음 격자 목록, 즐겨찾기·태그·검색(SQLite DB),
공문 첨부용 일괄 처리(축소 & EXIF 회전 정방향 반영 사본 저장),
사진대지 HWPX 생성 연동 기반(2×2·2×3 표에 촬영일/설명 캡션 넣기)을 제공한다.

동시 썸네일 생성 제한(기본 2개)을 지키며, 클라우드 전용 파일(OneDrive 등)은 내용을 절대 읽지 않는다.
원본 사진 파일의 내용은 어떠한 경우에도 수정하지 않는다.
사용자가 지시한 복사·이동·이름 변경·휴지통 삭제만 file_ops 를 거쳐 한다(FILEOPS1).
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
from PIL import Image, ImageEnhance, ImageOps

from docfit_core.abbreviation_doc import default_base_hwpx
from docfit_core.batch_format import _serialize
from docfit_core.hwpx import open_hwpx_safely
from docfit_core.table_builder import _Header, _cell, _child, _element, _table, _tag
from docfit_core.timemachine import thumbs
from docfit_core.path_utils import path_is_within
from docfit_core.timemachine.rules import cloud_only, excluded_name
from .library_scope import current_default_roots, is_excluded_path, is_listed_location
from .file_ops import FileOps, handle_command as handle_file_ops
from .background_scan import BackgroundScan
from .thumb_size_setting import handle_thumb_size
from .library_exclusions import read_exclusions, update_exclusion
from .rust_scan import scan as _rust_scan, scan_default_roots as _scan_default_roots
from . import library_scan_cache
from .open_listed_file import open_listed_file
from .doc_grid_service import find_klib_executable

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
EXIF_GPS_IFD: int = 0x8825
EXIF_MAKE: int = 271
EXIF_MODEL: int = 272

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


def compute_dhash(path: Path | str) -> int | None:
    """사진 파일에서 64비트 dHash(Difference Hash)를 계산한다.

    EXIF 방향(Orientation)을 정방향으로 보정한 후, 9x8 그레이스케일로 축소하여
    인접 픽셀(p[x, y] > p[x+1, y]) 64비트를 생성한다.
    오류 발생 시 None을 반환하며, 원본 파일은 절대 수정하지 않는다.
    """
    try:
        with Image.open(path) as img:
            img = ImageOps.exif_transpose(img)
            gray = img.convert("L").resize((9, 8), Image.Resampling.LANCZOS)
            pixels = list(gray.tobytes())
            h = 0
            for y in range(8):
                row_offset = y * 9
                for x in range(8):
                    p1 = pixels[row_offset + x]
                    p2 = pixels[row_offset + x + 1]
                    h = (h << 1) | (1 if p1 > p2 else 0)
            return h
    except Exception:
        return None


_compute_dhash = compute_dhash


def _read_photo_group_meta(path: Path | str) -> dict[str, Any]:
    """사진 파일의 EXIF에서 카메라 모델 및 오프라인 GPS 위치 정보를 안전하게 읽는다.

    외부 네트워크 호출 없이 100% 오프라인으로 동작하며, 클라우드 전용 파일은
    호출 전 배제되어야 한다. 오류 시에도 안전한 기본값을 반환한다.
    원본 파일은 절대 수정하지 않는다.
    """
    p = Path(path)
    res: dict[str, Any] = {
        "camera": "카메라 정보 없음",
        "gps_bucket": "",
        "location": p.parent.name or "사진뷰",
    }
    try:
        with Image.open(p) as img:
            exif = img.getexif()
            if not exif:
                return res

            make_raw = str(exif.get(EXIF_MAKE) or "").replace("\x00", "").strip()
            model_raw = str(exif.get(EXIF_MODEL) or "").replace("\x00", "").strip()
            camera_str = ""
            if make_raw and model_raw:
                if model_raw.lower().startswith(make_raw.lower()):
                    camera_str = model_raw
                else:
                    camera_str = f"{make_raw} {model_raw}"
            elif make_raw or model_raw:
                camera_str = make_raw or model_raw

            if camera_str:
                res["camera"] = camera_str

            try:
                gps_ifd = exif.get_ifd(EXIF_GPS_IFD)
                if gps_ifd:
                    lat_ref = gps_ifd.get(1)
                    lat_val = gps_ifd.get(2)
                    lon_ref = gps_ifd.get(3)
                    lon_val = gps_ifd.get(4)

                    def _to_float(v: Any) -> float:
                        if isinstance(v, (int, float)):
                            return float(v)
                        if hasattr(v, "numerator") and hasattr(v, "denominator"):
                            return float(v.numerator) / float(v.denominator) if v.denominator != 0 else 0.0
                        if isinstance(v, (tuple, list)) and len(v) == 2:
                            return float(v[0]) / float(v[1]) if v[1] != 0 else 0.0
                        return float(v)

                    def _calc_deg(coords: Any) -> float | None:
                        if not coords or not isinstance(coords, (tuple, list)) or len(coords) < 3:
                            return None
                        d = _to_float(coords[0])
                        m = _to_float(coords[1])
                        s = _to_float(coords[2])
                        return d + (m / 60.0) + (s / 3600.0)

                    lat = _calc_deg(lat_val)
                    lon = _calc_deg(lon_val)
                    if lat is not None and lon is not None:
                        if str(lat_ref).strip().upper() == "S":
                            lat = -lat
                        if str(lon_ref).strip().upper() == "W":
                            lon = -lon
                        bucket = f"{lat:.2f},{lon:.2f}"
                        res["gps_bucket"] = bucket
                        res["location"] = f"GPS ({lat:.2f}, {lon:.2f})"
            except Exception:
                pass
    except Exception:
        pass
    return res


def _path_within_root(path, root):
    """path 가 root 아래인지(드라이브가 다르면 아님). 공용 도우미 path_is_within 으로 위임한다."""
    return path_is_within(path, root)


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

    def delete_photo(self, path: str) -> None:
        """없어진 사진의 행과 태그를 지운다."""
        norm = _norm_path(path)
        with self._lock, closing(self._connect()) as conn:
            with conn:
                conn.execute("DELETE FROM tags WHERE photo_path = ?", (norm,))
                conn.execute("DELETE FROM photos WHERE path = ?", (norm,))

    def transfer_photo(self, source: str, target: str, *, keep_source: bool) -> None:
        """사진 정보를 새 경로에 옮긴다(태그·즐겨찾기 보존).

        `keep_source`(복사)면 원본 행은 두고, 촬영일 같은 파일 정보만 새 행에 쓴다.
        `source`·`target`은 작업 전후의 경로 문자열 그대로 정규화한다.
        """
        # 파일 이름의 대소문자는 받은 그대로 둔다(resolve 는 대소문자만 바꾼 이름 변경을 구분하지 못한다).
        old = str(Path(source).parent.resolve() / Path(source).name)
        new = str(Path(target).parent.resolve() / Path(target).name)
        folder, name = os.path.dirname(new), os.path.basename(new)
        with self._lock, closing(self._connect()) as conn:
            with conn:
                row = conn.execute("SELECT * FROM photos WHERE path = ?", (old,)).fetchone()
                if row is None:
                    return
                tags = [r["tag"] for r in conn.execute("SELECT tag FROM tags WHERE photo_path = ?", (old,))]
                removed = [new] if keep_source else [old, new]
                for path in removed:
                    # 새 경로에 남아 있던 옛 행은 이미 없는 파일의 것이다.
                    conn.execute("DELETE FROM tags WHERE photo_path = ?", (path,))
                    conn.execute("DELETE FROM photos WHERE path = ?", (path,))
                conn.execute(
                    """
                    INSERT INTO photos
                        (path, folder, filename, size, mtime, date_taken, orientation, favorite, cloud)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (new, folder, name, row["size"], row["mtime"], row["date_taken"], row["orientation"],
                     0 if keep_source else row["favorite"], row["cloud"]),
                )
                if not keep_source:
                    conn.executemany("INSERT INTO tags (photo_path, tag) VALUES (?, ?)",
                                     [(new, tag) for tag in tags])

    def list_photos(self, folder: str | None = None, *, recursive: bool = True) -> list[dict[str, Any]]:
        with self._lock, closing(self._connect()) as conn:
            if folder:
                folder_norm = _norm_path(folder)
                if recursive:
                    prefix_slash = folder_norm.rstrip("/\\") + "/"
                    prefix_backslash = folder_norm.rstrip("/\\") + "\\"
                    rows = conn.execute(
                        """
                        SELECT * FROM photos
                        WHERE folder = ? OR folder LIKE ? OR folder LIKE ?
                        ORDER BY date_taken DESC, filename ASC
                        """,
                        (folder_norm, prefix_slash + "%", prefix_backslash + "%"),
                    ).fetchall()
                else:
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
        drive_roots: Any | None = None,
        settings_store: Any | None = None,
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
        self._drive_roots = tuple(drive_roots) if drive_roots is not None else None
        self._background_scan = BackgroundScan()
        self.settings_store = settings_store
        self._default_name_paths: set[str] | None = None
        self.file_ops = FileOps(PHOTO_EXTENSIONS, after_item=self._after_file_op, after_all=self._after_file_ops)

    def _after_file_op(self, op: str, source: Path, target: Path | None, before: os.stat_result) -> dict[str, Any]:
        """성공한 파일 하나의 DB·썸네일 캐시를 맞추고, 새 위치가 목록 범위인지 알려 준다."""
        old_key = self.cache.key(source, before.st_mtime_ns, before.st_size)
        names = self._default_name_paths
        old_name = os.path.normcase(os.path.abspath(source))
        if target is None:
            self.db.delete_photo(str(source))
            self.cache.discard(old_key)
            if names is not None:
                names.discard(old_name)
            return {"listed": False}
        listed = op == "rename" or is_listed_location(
            target, self._default_roots(), read_exclusions(self.settings_store))
        if op != "copy":
            self.cache.rekey(old_key, self.cache.key(target, before.st_mtime_ns, before.st_size))
            # 목록 범위 밖으로 옮겨도 태그·즐겨찾기는 지우지 않는다.
            self.db.transfer_photo(str(source), str(target), keep_source=False)
            if names is not None:
                names.discard(old_name)
        elif listed:
            self.db.transfer_photo(str(source), str(target), keep_source=True)
        if names is not None and listed:
            names.add(os.path.normcase(os.path.abspath(target)))
        return {"listed": listed}

    def _after_file_ops(self, op: str, results: list[dict[str, Any]]) -> None:
        """다음에 화면을 열 때 보이는 저장된 목록에서도 옛 경로를 걷어낸다."""
        done = {os.path.normcase(item["path"]): item for item in results if item["ok"]}
        if not done:
            return

        def change(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
            changed: list[dict[str, Any]] = []
            for row in rows:
                item = done.get(os.path.normcase(str(row.get("path", ""))))
                if item is None or op == "copy":
                    changed.append(row)
                if item is None or not item.get("listed") or "new_path" not in item:
                    continue
                moved = {**row, "path": item["new_path"], "folder": os.path.dirname(item["new_path"]),
                         "filename": item["new_name"]}
                if op == "copy":
                    moved.update(tags=[], favorite=False)
                changed.append(moved)
            return changed

        library_scan_cache.rewrite("photos", change)

    def _get_klib_exe(self) -> Path | None:
        return find_klib_executable()

    def _default_roots(self) -> tuple[Path, ...]:
        return tuple(Path(p) for p in (self._drive_roots if self._drive_roots is not None else current_default_roots()))

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
        cancel_event: threading.Event | None = None,
        on_progress: Any | None = None,
        excluded_folders: Any = (),
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

        rust_rows = None
        if recursive and os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() != "python":
            rust_rows = _rust_scan(folder_path, PHOTO_EXTENSIONS, excluded_folders, executable=self._get_klib_exe(), cancel_event=cancel_event,
                                   on_progress=(lambda n, p: on_progress(p)) if on_progress else None)
        if rust_rows is not None:
            for row in rust_rows:
                if cancel_event is not None and cancel_event.is_set():
                    break
                path = Path(row["path"])
                name = row["name"]
                if "size" not in row or "mtime" not in row or "cloud" not in row:
                    try:
                        st = path.stat()
                        st_size, mtime = st.st_size, st.st_mtime
                        is_cloud = cloud_only(getattr(st, "st_file_attributes", 0))
                    except OSError:
                        continue
                else:
                    st_size, mtime, is_cloud = row["size"], row["mtime"], row["cloud"]
                path_str = str(path)
                scanned_count += 1
                if is_cloud:
                    date_taken, orientation = None, 1
                else:
                    date_taken, orientation = read_exif_metadata(path)
                    if date_taken is None:
                        date_taken = datetime.fromtimestamp(mtime)
                old = existing_photos.get(path_str)
                if old is not None and old["size"] == st_size and old["cloud"] == is_cloud and abs(old["mtime"] - mtime) < 0.001:
                    continue
                self.db.upsert_photo(path=path_str, folder=str(path.parent), filename=name,
                    size=st_size, mtime=mtime, date_taken=date_taken.strftime("%Y-%m-%d %H:%M:%S") if date_taken else None,
                    orientation=orientation, cloud=is_cloud)
                if old is None:
                    added_count += 1
                else:
                    updated_count += 1
            return {"ok": True, "folder": str(folder_path), "scanned": scanned_count,
                    "added": added_count, "updated": updated_count}

        def scan_dir(target_dir: Path) -> None:
            nonlocal scanned_count, added_count, updated_count
            try:
                with os.scandir(target_dir) as iterator:
                    for entry in iterator:
                        if cancel_event is not None and cancel_event.is_set():
                            return
                        if on_progress is not None:
                            on_progress(str(target_dir))
                        try:
                            name = entry.name
                            if name.startswith(".") or excluded_name(name):
                                continue

                            if entry.is_dir(follow_symlinks=False):
                                if recursive and not is_excluded_path(entry.path, excluded_folders, base=folder_path):
                                    scan_dir(Path(entry.path))
                                continue

                            if not entry.is_file(follow_symlinks=False):
                                continue
                            if is_excluded_path(entry.path, excluded_folders, base=folder_path):
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

    def _without_excluded(self, photos: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """제외 폴더의 사진은 목록에서만 감춘다. DB의 태그·즐겨찾기는 남겨 제외를 풀면 다시 보인다."""
        excluded = read_exclusions(self.settings_store)
        if not excluded:
            return photos
        return [p for p in photos if not any(path_is_within(p["path"], folder) for folder in excluded)]

    def scan_default_scope(self, *, cancel_event: threading.Event | None = None, on_progress: Any | None = None,
                           excluded_folders: Any = (), return_engine: bool = False) -> dict[str, Any]:
        """Desktop 및 고정 드라이브 사진을 스캔한다. 이동식·네트워크는 제외한다."""
        scanned = added = updated = 0
        engine = "python"
        rows = None
        if os.environ.get("DOCFIT_SCAN_ENGINE", "").strip().casefold() == "python":
            engine = "python"
        else:
            from .doc_grid_service import query_names_service
            rows = query_names_service(PHOTO_EXTENSIONS, excluded_folders)
            if rows is not None: engine = "service"
            else:
                rows = _scan_default_roots(PHOTO_EXTENSIONS, excluded_folders,
                    executable=self._get_klib_exe(), cancel_event=cancel_event,
                    on_progress=(lambda n, p: on_progress(p)) if on_progress else None,
                    roots=self._default_roots())
                if rows is not None: engine = "klib"
        if rows is not None:
            self._default_name_paths = {os.path.normcase(os.path.abspath(row["path"])) for row in rows} if engine == "service" else None
            # Name-index rows lack metadata; only seed unseen paths, and defer stat/EXIF until a visible item needs it.
            for root in self._default_roots():
                if cancel_event is not None and cancel_event.is_set(): break
                if not root.is_dir() or is_excluded_path(root, excluded_folders, base=root): continue
                self.db.register_folder(str(root))
                matching = []
                root_norm = os.path.normcase(os.path.abspath(str(root)))
                for row in rows:
                    if path_is_within(os.path.normcase(os.path.abspath(row["path"])), root_norm): matching.append(row)
                if engine == "service":
                    existing = {row["path"] for row in self.db.list_photos(str(root))}
                    added_rows = 0
                    for row in matching:
                        path = Path(row["path"])
                        if str(path) in existing or (cancel_event is not None and cancel_event.is_set()): continue
                        self.db.upsert_photo(path=str(path), folder=str(path.parent), filename=row["name"],
                            size=0, mtime=0, date_taken="", orientation=1, cloud=0)
                        added_rows += 1
                    result = {"scanned": len(matching), "added": added_rows, "updated": 0}
                else:
                    result = self._apply_default_rows(root, matching, cancel_event)
                scanned += result["scanned"]; added += result["added"]; updated += result["updated"]
            response = {"ok": True, "roots": [str(p) for p in self._default_roots()], "scanned": scanned,
                        "added": added, "updated": updated, "engine": engine}
            return (response, engine) if return_engine else response
        roots = []
        self._default_name_paths = None
        for root in self._default_roots():
            if cancel_event is not None and cancel_event.is_set():
                break
            if not root.is_dir() or is_excluded_path(root, excluded_folders, base=root):
                continue
            roots.append(str(root))
            self.db.register_folder(str(root))
            result = self.scan_folder(root, recursive=True, cancel_event=cancel_event,
                                      on_progress=on_progress, excluded_folders=excluded_folders)
            if result.get("ok"):
                scanned += result.get("scanned", 0)
                added += result.get("added", 0)
                updated += result.get("updated", 0)
        response = {"ok": True, "roots": roots, "scanned": scanned, "added": added, "updated": updated, "engine": "python"}
        return (response, engine) if return_engine else response

    def _apply_default_rows(self, root: Path, rows: list[dict[str, Any]], cancel_event: threading.Event | None) -> dict[str, int]:
        scanned = added = updated = 0
        existing = {p["path"]: p for p in self.db.list_photos(str(root))}
        for row in rows:
            if cancel_event is not None and cancel_event.is_set(): break
            path = Path(row["path"])
            try:
                st = path.stat()
            except OSError:
                continue
            cloud = bool(row.get("cloud", cloud_only(getattr(st, "st_file_attributes", 0))))
            old = existing.get(str(path))
            scanned += 1
            if old and old["size"] == st.st_size and old["cloud"] == cloud and abs(old["mtime"] - st.st_mtime) < .001: continue
            date_taken, orientation = (None, 1) if cloud else read_exif_metadata(path)
            if date_taken is None: date_taken = datetime.fromtimestamp(st.st_mtime)
            self.db.upsert_photo(path=str(path), folder=str(path.parent), filename=path.name, size=st.st_size,
                mtime=st.st_mtime, date_taken=date_taken.strftime("%Y-%m-%d %H:%M:%S") if date_taken else None,
                orientation=orientation, cloud=cloud)
            if old is None: added += 1
            else: updated += 1
        return {"scanned": scanned, "added": added, "updated": updated}

    def start_default_scan(self) -> dict[str, Any]:
        excluded = read_exclusions(self.settings_store)
        roots = [str(p) for p in self._default_roots()]
        cached = library_scan_cache.load("photos", PHOTO_EXTENSIONS, excluded, roots=roots)
        def worker(report: Any, cancel: threading.Event) -> dict[str, Any]:
            visited = 0
            def tick(current_folder: str = "") -> None:
                nonlocal visited
                visited += 1
                report(visited, current_folder)
            result, engine = self.scan_default_scope(cancel_event=cancel, on_progress=tick,
                excluded_folders=excluded, return_engine=True)
            rows = []
            roots = [os.path.normcase(os.path.abspath(str(root))) for root in self._default_roots()]
            for root in self._default_roots(): rows.extend(self.db.list_photos(str(root)))
            existing = []
            for row in rows:
                try: path = os.path.normcase(os.path.abspath(row["path"]))
                except OSError: continue
                valid = path in self._default_name_paths if self._default_name_paths is not None else Path(row["path"]).is_file()
                if valid and any(path_is_within(path, root) for root in roots): existing.append(row)
            rows = self._without_excluded(existing)
            if not cancel.is_set(): library_scan_cache.save("photos", PHOTO_EXTENSIONS, excluded, rows, roots=roots)
            result["engine"] = engine
            return result
        initial = {"photos": cached, "engine": "cache"} if cached is not None else None
        started = self._background_scan.start(worker, initial_result=initial,
                                              engine="cache" if cached is not None else None)
        if cached is not None and started.get("ok"):
            started["result"] = {"photos": [{**item, "state": "cached"} for item in cached], "engine": "cache"}
        return started

    def cached_default_photos(self) -> dict[str, Any]:
        excluded = read_exclusions(self.settings_store)
        rows = library_scan_cache.load("photos", PHOTO_EXTENSIONS, excluded, roots=[str(p) for p in self._default_roots()])
        photos = [] if rows is None else [dict(row, name=row.get("filename", row.get("name", "")))
            for row in rows if Path(row.get("path", "")).is_file()]
        return {"ok": True, "photos": photos, "state": "cached" if rows is not None else "empty",
                "engine": "cache" if rows is not None else None}

    # ---- 2. 연·월 묶음 목록 및 검색 ----

    def list_by_date(self, folder: str | Path | None = None) -> list[dict[str, Any]]:
        """촬영일(또는 수정일) 기준 연·월 묶음 목록을 반환한다."""
        folder_str = str(Path(folder).resolve()) if folder else None
        photos = self._without_excluded(self.db.list_photos(folder_str))

        # 연·월별로 그룹화
        groups: dict[tuple[int, int], list[dict[str, Any]]] = {}
        for p in photos:
            path_key = os.path.normcase(os.path.abspath(p["path"]))
            service_row = self._default_name_paths is not None and path_key in self._default_name_paths
            if self._default_name_paths is not None and not service_row:
                continue
            try:
                st = None if service_row else os.stat(p["path"])
            except OSError:
                continue
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
                "metadata_pending": bool(service_row and not p["size"] and not p["mtime"] and not p["date_taken"]),
            }
            if st is not None:
                thumb_key = self.cache.key(p["path"], st.st_mtime_ns, st.st_size)
                item["thumb_key"] = thumb_key
                item["has_thumb"] = bool(not p["cloud"] and self.cache.get(thumb_key) is not None)
            else:
                item["thumb_key"] = ""

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
        photos = self._without_excluded(self.db.search(
            query=query,
            folder=folder_str,
            tag=tag,
            start_date=start_date,
            end_date=end_date,
            favorite_only=favorite_only,
        ))
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

    def list_tags(self, *, folder: str | Path | None = None) -> dict[str, Any]:
        """사진뷰 DB에서 사용 중인 태그와 사진 수를 반환한다."""
        photos = self._without_excluded(self.db.list_photos(
            str(Path(folder).resolve()) if folder and str(folder).strip() else None
        ))
        counts: dict[str, int] = {}
        for photo in photos:
            for tag in photo.get("tags", []):
                counts[tag] = counts.get(tag, 0) + 1
        return {"ok": True, "tags": [
            {"tag": tag, "count": counts[tag]} for tag in sorted(counts, key=str.casefold)
        ]}

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

    # ---- 7. 유사·중복 사진 탐지 및 편집·오프라인 묶음 ----

    def find_duplicates(
        self,
        folder: str | Path | None = None,
        *,
        threshold: int = 6,
    ) -> dict[str, Any]:
        """64비트 dHash 기반 유사·중복 사진을 탐지하여 그룹별로 반환한다.

        클라우드 전용 파일은 내용을 읽지 않고 건너뛰며, 원본 파일은 절대 수정하지 않는다.
        """
        try:
            threshold_val = int(threshold)
        except (ValueError, TypeError):
            return {"ok": False, "code": "INVALID_INPUT", "message": "threshold는 정수여야 합니다."}

        if threshold_val < 0:
            return {"ok": False, "code": "INVALID_INPUT", "message": "threshold는 0 이상이어야 합니다."}

        threshold_val = min(64, threshold_val)

        folder_str = str(Path(folder).resolve()) if folder else None
        photos = self._without_excluded(self.db.list_photos(folder_str))

        scanned = 0
        skipped_cloud = 0
        skipped_missing = 0
        valid_items: list[tuple[dict[str, Any], int]] = []

        for p in photos:
            if p.get("cloud"):
                skipped_cloud += 1
                continue
            path_obj = Path(p["path"])
            if not path_obj.is_file():
                skipped_missing += 1
                continue

            try:
                st = path_obj.stat()
                if cloud_only(getattr(st, "st_file_attributes", 0)):
                    skipped_cloud += 1
                    continue
            except OSError:
                skipped_missing += 1
                continue

            h = compute_dhash(path_obj)
            if h is None:
                skipped_missing += 1
                continue

            scanned += 1
            valid_items.append((p, h))

        n = len(valid_items)
        if n < 2:
            return {
                "ok": True,
                "threshold": threshold_val,
                "scanned": scanned,
                "skipped_cloud": skipped_cloud,
                "skipped_missing": skipped_missing,
                "groups": [],
            }

        # Union-Find 클러스터링
        parent = list(range(n))

        def find(i: int) -> int:
            path_nodes = []
            while parent[i] != i:
                path_nodes.append(i)
                i = parent[i]
            for node in path_nodes:
                parent[node] = i
            return i

        def union(i: int, j: int) -> None:
            root_i = find(i)
            root_j = find(j)
            if root_i != root_j:
                parent[root_i] = root_j

        for i in range(n):
            hi = valid_items[i][1]
            for j in range(i + 1, n):
                hj = valid_items[j][1]
                if (hi ^ hj).bit_count() <= threshold_val:
                    union(i, j)

        clusters: dict[int, list[int]] = {}
        for idx in range(n):
            clusters.setdefault(find(idx), []).append(idx)

        group_id_counter = 1
        raw_groups = []
        for cluster_indices in clusters.values():
            if len(cluster_indices) < 2:
                continue

            # 대표 항목: 파일 크기(size)가 가장 큰 항목 (동률이면 첫 항목)
            rep_idx = max(cluster_indices, key=lambda idx: (valid_items[idx][0].get("size", 0), -idx))
            rep_photo, rep_hash = valid_items[rep_idx]
            rep_path = rep_photo["path"]

            items = []
            for idx in cluster_indices:
                photo, h = valid_items[idx]
                dist = (h ^ rep_hash).bit_count()
                items.append({
                    "path": photo["path"],
                    "name": photo["filename"],
                    "size": photo["size"],
                    "date": photo["date_taken"],
                    "dhash": f"{h:016x}",
                    "distance": dist,
                })

            items.sort(key=lambda it: (it["distance"], it["name"]))

            raw_groups.append({
                "count": len(items),
                "representative": rep_path,
                "items": items,
            })

        raw_groups.sort(key=lambda g: (-g["count"], g["representative"]))
        for g in raw_groups:
            g["group_id"] = group_id_counter
            group_id_counter += 1

        return {
            "ok": True,
            "threshold": threshold_val,
            "scanned": scanned,
            "skipped_cloud": skipped_cloud,
            "skipped_missing": skipped_missing,
            "groups": raw_groups,
        }

    def edit_copy(
        self,
        path: str | Path,
        *,
        output_path: str | Path | None = None,
        output_dir: str | Path | None = None,
        crop: tuple[int, int, int, int] | list[int] | None = None,
        rotate_deg: float | int = 0,
        brightness: float = 1.0,
        contrast: float = 1.0,
    ) -> dict[str, Any]:
        """원본을 보존하며 비파괴 사본을 생성하여 편집한다.

        클라우드 전용 파일은 내용을 읽지 않고 CLOUD_ONLY로 거부한다.
        원본 덮어쓰기는 절대 차단(OVERWRITE_FORBIDDEN)한다.
        """
        src_path = Path(path).resolve()
        src_norm = _norm_path(src_path)

        # 1. 클라우드 전용 파일 확인 (DB 플래그 & OS 속성)
        db_photo = self.db.get_photo(src_norm)
        if db_photo and db_photo.get("cloud"):
            return {
                "ok": False,
                "code": "CLOUD_ONLY",
                "message": "클라우드 전용 파일은 오프라인에서 편집할 수 없습니다.",
            }

        if not src_path.is_file():
            return {
                "ok": False,
                "code": "FILE_NOT_FOUND",
                "message": f"원본 파일이 존재하지 않습니다: {path}",
            }

        try:
            st = src_path.stat()
            if cloud_only(getattr(st, "st_file_attributes", 0)):
                return {
                    "ok": False,
                    "code": "CLOUD_ONLY",
                    "message": "클라우드 전용 파일은 오프라인에서 편집할 수 없습니다.",
                }
        except OSError:
            return {
                "ok": False,
                "code": "FILE_NOT_FOUND",
                "message": f"파일 상태를 확인할 수 없습니다: {path}",
            }

        # 2. 파라미터 유효성 검증
        try:
            rotate_val = float(rotate_deg)
        except (ValueError, TypeError):
            return {"ok": False, "code": "INVALID_INPUT", "message": "rotate_deg는 숫자여야 합니다."}

        try:
            brightness_val = float(brightness)
        except (ValueError, TypeError):
            return {"ok": False, "code": "INVALID_INPUT", "message": "brightness는 숫자여야 합니다."}
        if not (0.0 < brightness_val <= 5.0):
            return {"ok": False, "code": "INVALID_INPUT", "message": "brightness는 0.0 초과 5.0 이하여야 합니다."}

        try:
            contrast_val = float(contrast)
        except (ValueError, TypeError):
            return {"ok": False, "code": "INVALID_INPUT", "message": "contrast는 숫자여야 합니다."}
        if not (0.0 < contrast_val <= 5.0):
            return {"ok": False, "code": "INVALID_INPUT", "message": "contrast는 0.0 초과 5.0 이하여야 합니다."}

        if crop is not None:
            if not isinstance(crop, (list, tuple)) or len(crop) != 4:
                return {"ok": False, "code": "INVALID_CROP", "message": "crop은 4개의 정수 (left, top, right, bottom)여야 합니다."}
            try:
                _ = [int(v) for v in crop]
            except (ValueError, TypeError):
                return {"ok": False, "code": "INVALID_CROP", "message": "crop 좌표는 정수여야 합니다."}

        # 3. 출력 경로 결정 및 덮어쓰기 절대 차단
        if output_path:
            out_p = Path(output_path).resolve()
            if _norm_path(out_p) == src_norm:
                return {
                    "ok": False,
                    "code": "OVERWRITE_FORBIDDEN",
                    "message": "원본 파일은 덮어쓸 수 없습니다.",
                }
        else:
            base_dir = Path(output_dir).resolve() if output_dir else src_path.parent
            base_dir.mkdir(parents=True, exist_ok=True)
            stem = src_path.stem
            ext = src_path.suffix
            cand = base_dir / f"{stem}_edited{ext}"
            idx = 1
            while _norm_path(cand) == src_norm or cand.exists():
                cand = base_dir / f"{stem}_edited_{idx}{ext}"
                idx += 1
            out_p = cand

        if _norm_path(out_p) == src_norm:
            return {
                "ok": False,
                "code": "OVERWRITE_FORBIDDEN",
                "message": "원본 파일은 덮어쓸 수 없습니다.",
            }

        out_p.parent.mkdir(parents=True, exist_ok=True)

        # 4. 이미지 편집 파이프라인
        try:
            with Image.open(src_path) as orig_img:
                img = ImageOps.exif_transpose(orig_img)

                # 시계 방향 회전 (PIL rotate는 반시계 방향 기준이므로 -rotate_val)
                if rotate_val % 360 != 0:
                    img = img.rotate(-rotate_val, expand=True)

                # 자르기 (crop)
                if crop is not None:
                    w, h = img.size
                    c_left = max(0, min(w, int(crop[0])))
                    c_top = max(0, min(h, int(crop[1])))
                    c_right = max(0, min(w, int(crop[2])))
                    c_bottom = max(0, min(h, int(crop[3])))
                    if c_right <= c_left or c_bottom <= c_top:
                        return {"ok": False, "code": "INVALID_CROP", "message": "유효하지 않은 자르기 영역입니다."}
                    img = img.crop((c_left, c_top, c_right, c_bottom))

                # 밝기 조정
                if abs(brightness_val - 1.0) > 1e-4:
                    enhancer = ImageEnhance.Brightness(img)
                    img = enhancer.enhance(brightness_val)

                # 대비 조정
                if abs(contrast_val - 1.0) > 1e-4:
                    enhancer = ImageEnhance.Contrast(img)
                    img = enhancer.enhance(contrast_val)

                # 저장 포맷 결정 및 색상 공간 변환
                ext_upper = out_p.suffix.lstrip(".").upper()
                fmt = ext_upper if ext_upper in ("PNG", "WEBP", "BMP", "TIFF") else "JPEG"
                if fmt == "JPEG" and img.mode in ("RGBA", "LA", "P"):
                    img = img.convert("RGB")

                # 임시 파일 저장 후 rename (원자적 파일 저장)
                tmp_out = out_p.with_name(f".tmp_{out_p.name}_{os.getpid()}")
                try:
                    if fmt == "JPEG":
                        img.save(tmp_out, format="JPEG", quality=95)
                    else:
                        img.save(tmp_out, format=fmt)
                    tmp_out.replace(out_p)
                finally:
                    if tmp_out.exists():
                        try:
                            tmp_out.unlink()
                        except OSError:
                            pass

                return {
                    "ok": True,
                    "source": str(src_path),
                    "output_path": str(out_p),
                    "width": img.width,
                    "height": img.height,
                }
        except Exception as exc:
            logger.exception("사진 사본 편집 실패: %s", exc)
            return {"ok": False, "code": "EDIT_FAILED", "message": f"편집 실패: {exc}"}

    def place_groups(
        self,
        folder: str | Path | None = None,
        *,
        group_by: str = "folder",
    ) -> dict[str, Any]:
        """외부 네트워크 없이 EXIF 및 폴더명 기반으로 사진을 오프라인 그룹화한다.

        group_by: 'folder'(기본: 폴더별), 'camera'(카메라 기종별), 'location'(GPS 버킷 또는 폴더명 fallback)
        """
        group_by_norm = (group_by or "").strip().lower()
        if group_by_norm not in ("folder", "camera", "location"):
            return {
                "ok": False,
                "code": "INVALID_INPUT",
                "message": f"지원하지 않는 group_by 모드입니다: {group_by}",
            }

        folder_str = str(Path(folder).resolve()) if folder else None
        photos = self._without_excluded(self.db.list_photos(folder_str))

        groups_map: dict[str, dict[str, Any]] = {}

        for p in photos:
            is_cloud = bool(p.get("cloud", False))
            path_obj = Path(p["path"])

            if is_cloud or not path_obj.is_file():
                meta = {
                    "camera": "카메라 정보 없음",
                    "gps_bucket": "",
                    "location": path_obj.parent.name or "사진뷰",
                }
            else:
                try:
                    st = path_obj.stat()
                    if cloud_only(getattr(st, "st_file_attributes", 0)):
                        is_cloud = True
                        meta = {
                            "camera": "카메라 정보 없음",
                            "gps_bucket": "",
                            "location": path_obj.parent.name or "사진뷰",
                        }
                    else:
                        meta = _read_photo_group_meta(path_obj)
                except OSError:
                    meta = {
                        "camera": "카메라 정보 없음",
                        "gps_bucket": "",
                        "location": path_obj.parent.name or "사진뷰",
                    }

            # 그룹 키와 라벨 결정
            if group_by_norm == "folder":
                folder_norm = _norm_path(p.get("folder") or path_obj.parent)
                key = folder_norm
                label = Path(folder_norm).name or "사진뷰"
            elif group_by_norm == "camera":
                key = meta["camera"]
                label = key
            elif group_by_norm == "location":
                if meta["gps_bucket"]:
                    key = meta["gps_bucket"]
                    label = meta["location"]
                else:
                    folder_norm = _norm_path(p.get("folder") or path_obj.parent)
                    key = f"folder:{folder_norm}"
                    label = Path(folder_norm).name or "사진뷰"

            if key not in groups_map:
                groups_map[key] = {
                    "key": key,
                    "label": label,
                    "items": [],
                    "cameras_set": set(),
                }

            group_entry = groups_map[key]
            cam = meta["camera"]
            if cam and cam != "카메라 정보 없음":
                group_entry["cameras_set"].add(cam)

            group_entry["items"].append({
                "path": p["path"],
                "name": p["filename"],
                "date": p["date_taken"],
                "camera": meta["camera"],
                "location": meta["location"],
                "cloud": is_cloud,
            })

        groups = []
        for g in groups_map.values():
            cams = sorted(g.pop("cameras_set"))
            g["count"] = len(g["items"])
            g["cameras"] = cams
            groups.append(g)

        # count 내림차순 -> label 오름차순 정렬
        groups.sort(key=lambda g: (-g["count"], g["label"]))

        return {
            "ok": True,
            "group_by": group_by_norm,
            "total_photos": len(photos),
            "groups": groups,
        }

    # ---- 8. 디스패처 ----

    def open_file(self, path: str | Path) -> dict[str, Any]:
        """지정된 사진을 시스템 기본 연결 프로그램으로 연다(사진 확장자만)."""
        return open_listed_file(path, PHOTO_EXTENSIONS, self.opener, "사진")

    def command(self, action: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        """브리지 명령 디스패처."""
        payload = payload or {}
        if not isinstance(payload, dict):
            return {"ok": False, "code": "INVALID_INPUT", "message": "입력을 확인해 주세요."}

        try:
            if action == "scan_default":
                return self.start_default_scan()

            if action == "cached_default_photos":
                return self.cached_default_photos()

            if action == "scan_progress":
                return self._background_scan.progress()

            if action == "scan_cancel":
                return self._background_scan.cancel()

            if action in ("thumb_size_get", "thumb_size_set"):
                return handle_thumb_size(self.settings_store, "photo_album", action, payload)

            if action == "excluded_folders_get":
                return {"ok": True, "folders": read_exclusions(self.settings_store)}

            if action in ("excluded_folder_add", "excluded_folder_remove"):
                folder = payload.get("folder")
                if not isinstance(folder, str) or not folder.strip():
                    return {"ok": False, "code": "INVALID_FOLDER", "message": "폴더 경로를 입력해 주세요."}
                folders = update_exclusion(self.settings_store, folder,
                                           add=action == "excluded_folder_add")
                return {"ok": True, "folders": folders}

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

            if action == "list_tags":
                return self.list_tags(folder=payload.get("folder"))

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

            if action == "open":
                path = payload.get("path")
                if not path or not isinstance(path, str):
                    return {"ok": False, "code": "INVALID_PATH", "message": "파일 경로를 지정해 주세요."}
                return self.open_file(path)

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

            if action == "find_duplicates":
                return self.find_duplicates(
                    folder=payload.get("folder"),
                    threshold=payload.get("threshold", 6),
                )

            if action == "edit_copy":
                path = payload.get("path")
                if not path:
                    return {"ok": False, "message": "path가 필요합니다."}
                return self.edit_copy(
                    path,
                    output_path=payload.get("output_path"),
                    output_dir=payload.get("output_dir"),
                    crop=payload.get("crop"),
                    rotate_deg=payload.get("rotate_deg", 0),
                    brightness=float(payload.get("brightness", 1.0)),
                    contrast=float(payload.get("contrast", 1.0)),
                )

            if action == "place_groups":
                return self.place_groups(
                    folder=payload.get("folder"),
                    group_by=str(payload.get("group_by", "folder")),
                )

            handled = handle_file_ops(self.file_ops, action, payload)
            if handled is not None:
                return handled

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
