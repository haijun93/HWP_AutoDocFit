"""사진뷰 목록 비교·반영과 촬영 정보 배경 정리(PHOTOSYNC1).

저장소는 새 DB 가 아니라 기존 `photo_album.db`(태그·즐겨찾기 포함)다. 원천 비교는 문서뷰와 같은
`library_db.ViewSync` 를 쓰고, 여기서는 그 저장소 자리(`load`·`write`)와 사진 화면 행만 바꾼다.

- 새 파일·크기나 수정 시각이 바뀐 파일만 `date_taken` 을 비워 둔다. 빈 값이 "촬영 정보를 아직 안 읽음"
  표시라서, 정리를 중단하거나 앱을 껐다 켜도 남은 것부터 이어서 한다.
- 디스크에서 사라진 사진은 행을 지운다. 태그나 즐겨찾기가 있으면 지우지 않고 `missing` 으로 감춘다.
- DB 오류가 나도 DB 파일은 지우지 않는다(사용자 태그·즐겨찾기가 들어 있다).
"""
from __future__ import annotations

import json
import logging
import os
import threading
import time
import uuid
from contextlib import closing
from datetime import datetime
from typing import Any, Callable, Iterable

from docfit_core.log_mask import mask_log_text
from docfit_core.timemachine.rules import cloud_only

from .library_db import MTIME_TOLERANCE, Record, ViewSync, path_key

logger = logging.getLogger(__name__)

# 촬영 정보는 이만큼 또는 이 시간마다 저장한다(중단해도 여기까지는 남는다).
EXIF_BATCH = 200
EXIF_FLUSH_SECONDS = 2.0
# 진행 응답 하나에 싣는 갱신 행의 상한.
EXIF_PAGE_ROWS = 4000


def date_text(mtime: float | None) -> str:
    if not mtime:
        return ""
    try:
        return datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M:%S")
    except (OverflowError, OSError, ValueError):
        return ""


def _pack(values: list) -> str:
    return json.dumps(values, ensure_ascii=False, separators=(",", ":"))


def _storable(text: str) -> bool:
    try:
        text.encode("utf-8")
        return True
    except UnicodeEncodeError:
        return False


def photo_row(path: str, name: str, date: str, size: int | None, mtime: float | None, cloud: bool,
              orientation: int, favorite: bool, tags: Iterable[str]) -> dict[str, Any]:
    """화면에 넘기는 사진 행. 촬영일을 아직 못 읽었으면 수정 시각을 보여 준다."""
    shown = date or date_text(mtime)
    return {"path": path, "name": name, "filename": name, "date": shown, "date_taken": shown,
            "size": size or 0, "favorite": bool(favorite), "tags": list(tags),
            "orientation": orientation or 1, "cloud": bool(cloud),
            "metadata_pending": size is None and not shown}


class _Saved:
    """DB 에 저장된 사진 하나의 현재 값."""

    __slots__ = ("path", "size", "mtime", "cloud", "date", "orientation", "favorite", "tags", "missing")

    def __init__(self, path: str, size: Any, mtime: Any, cloud: Any, date: Any, orientation: Any,
                 favorite: Any, tags: list[str], missing: Any) -> None:
        self.path, self.size, self.mtime = path, size, mtime
        self.cloud, self.date, self.orientation = bool(cloud), date or "", orientation or 1
        self.favorite, self.tags, self.missing = bool(favorite), tags, bool(missing)


def _carried(old: _Saved | None, size: int | None, mtime: float | None, cloud: bool) -> tuple[str, int]:
    """비교 결과에 대해 저장할 (촬영일, 회전). 빈 촬영일은 다시 읽어야 한다는 뜻이다."""
    if cloud:  # 클라우드 전용은 내용을 읽지 않는다. 전에 읽은 값은 남긴다.
        return (old.date if old and old.date else date_text(mtime)), (old.orientation if old else 1)
    if old is None:
        return "", 1
    same = (old.size == size and old.mtime is not None and mtime is not None
            and abs(old.mtime - mtime) < MTIME_TOLERANCE)
    if same and not old.cloud:
        return old.date, old.orientation
    return "", old.orientation


_UPSERT = """
INSERT INTO photos(path, folder, filename, size, mtime, date_taken, orientation, cloud, missing)
SELECT json_extract(value,'$[0]'), json_extract(value,'$[1]'), json_extract(value,'$[2]'),
       json_extract(value,'$[3]'), json_extract(value,'$[4]'), json_extract(value,'$[5]'),
       json_extract(value,'$[6]'), json_extract(value,'$[7]'), 0
FROM json_each(?) WHERE true
ON CONFLICT(path) DO UPDATE SET
    folder=excluded.folder, filename=excluded.filename, size=excluded.size, mtime=excluded.mtime,
    date_taken=CASE WHEN excluded.date_taken='' AND photos.cloud=0 AND photos.size=excluded.size
                         AND abs(IFNULL(photos.mtime,0)-excluded.mtime) < 1.0
                    THEN photos.date_taken ELSE excluded.date_taken END,
    orientation=excluded.orientation, cloud=excluded.cloud, missing=0
"""

_MARKED = "(favorite=1 OR EXISTS(SELECT 1 FROM tags t WHERE t.photo_path = photos.path))"


class PhotoStore:
    """`ViewSync` 가 쓰는 저장소 자리. 실제 저장은 `PhotoAlbumDb` 의 연결과 잠금을 쓴다."""

    def __init__(self, db: Any) -> None:
        self.db = db
        self.saved: dict[str, _Saved] = {}

    def _read(self, connection: Any, where: str = "", params: tuple = ()) -> list[tuple[str, str, _Saved]]:
        packed = connection.execute(
            "SELECT json_group_array(json_array(path,filename,size,mtime,cloud,date_taken,orientation,favorite,"
            f"missing)) FROM (SELECT * FROM photos {where} ORDER BY date_taken DESC, filename)", params).fetchone()[0]
        tags: dict[str, list[str]] = {}
        for path, tag in connection.execute("SELECT photo_path, tag FROM tags ORDER BY tag"):
            tags.setdefault(path, []).append(tag)
        return [(path, name, _Saved(path, size, mtime, cloud, date, orientation, favorite, tags.get(path, []), missing))
                for path, name, size, mtime, cloud, date, orientation, favorite, missing in json.loads(packed)]

    def load(self) -> list[Record]:
        """저장된 전체 목록(최신 촬영일 먼저). 대소문자만 다른 중복 행은 하나로 합친다."""
        with self.db._lock, closing(self.db._connect()) as connection:
            entries = self._read(connection)
            keys = [path_key(path) for path, _, _ in entries]
            if len(set(keys)) != len(keys):
                with connection:
                    self._merge_duplicates(connection, entries)
                entries = self._read(connection)
        self.saved = {}
        records: list[Record] = []
        for path, name, saved in entries:
            key = path_key(path)
            self.saved[key] = saved
            records.append((key, path, name, os.path.splitext(name)[1].lstrip(".").lower(),
                            saved.size, saved.mtime, saved.cloud, ""))
        return records

    @staticmethod
    def _merge_duplicates(connection: Any, entries: list[tuple[str, str, _Saved]]) -> None:
        """이전 버전이 같은 파일을 표기만 다르게 두 번 저장한 행을 합친다(태그·즐겨찾기 보존)."""
        groups: dict[str, list[_Saved]] = {}
        for path, _, saved in entries:
            groups.setdefault(path_key(path), []).append(saved)
        for group in groups.values():
            if len(group) < 2:
                continue
            keep = max(group, key=lambda s: (bool(s.date), bool(s.size), s.favorite or bool(s.tags)))
            for other in group:
                if other is keep:
                    continue
                if other.favorite:
                    connection.execute("UPDATE photos SET favorite=1 WHERE path=?", (keep.path,))
                connection.executemany("INSERT OR IGNORE INTO tags(photo_path, tag) VALUES (?, ?)",
                                       [(keep.path, tag) for tag in other.tags])
                connection.execute("DELETE FROM tags WHERE photo_path=?", (other.path,))
                connection.execute("DELETE FROM photos WHERE path=?", (other.path,))

    def refresh(self, path: str) -> None:
        """검색 중 파일 작업·태그 변경을 한 행의 보관 값에 반영한다."""
        with self.db._lock, closing(self.db._connect()) as connection:
            row = connection.execute("SELECT * FROM photos WHERE path=?", (path,)).fetchone()
            if row is None:
                return
            tags = [r[0] for r in connection.execute("SELECT tag FROM tags WHERE photo_path=? ORDER BY tag", (path,))]
        self.saved[path_key(path)] = _Saved(row["path"], row["size"], row["mtime"], row["cloud"], row["date_taken"],
                                            row["orientation"], row["favorite"], tags, row["missing"])

    def write(self, upserts: Iterable[Record] = (), removes: Iterable[str] = (),
              seen: Iterable[str] = ()) -> None:
        """바뀐 행만 저장한다. 확인한 파일(`seen`)은 사진 DB 에 따로 남기지 않는다."""
        rows: list[list[Any]] = []
        renames: list[tuple[str, str]] = []
        for key, path, name, _, size, mtime, cloud, _ in upserts:
            if not _storable(path):
                continue
            old = self.saved.get(key)
            date, orientation = _carried(old, size, mtime, cloud)
            if old is not None and old.path != path:
                renames.append((old.path, path))
            rows.append([path, os.path.dirname(path), name, size or 0, mtime or 0, date, orientation, int(cloud)])
        gone = [self.saved[key].path for key in removes if key in self.saved]
        if not (rows or gone):
            return
        with self.db._lock, closing(self.db._connect()) as connection, connection:
            for old_path, new_path in renames:  # 파일 이름의 대소문자만 바뀐 경우
                connection.execute(
                    "INSERT OR IGNORE INTO photos(path,folder,filename,size,mtime,date_taken,orientation,favorite,cloud)"
                    " SELECT ?,folder,filename,size,mtime,date_taken,orientation,favorite,cloud FROM photos"
                    " WHERE path=?", (new_path, old_path))
                connection.execute("UPDATE OR IGNORE tags SET photo_path=? WHERE photo_path=?", (new_path, old_path))
                connection.execute("DELETE FROM tags WHERE photo_path=?", (old_path,))
                connection.execute("DELETE FROM photos WHERE path=?", (old_path,))
            if rows:
                connection.execute(_UPSERT, (_pack(rows),))
            if gone:
                listed = "path IN (SELECT value FROM json_each(?))"
                connection.execute(f"UPDATE photos SET missing=1 WHERE {listed} AND {_MARKED}", (_pack(gone),))
                connection.execute(f"DELETE FROM photos WHERE {listed} AND NOT {_MARKED}", (_pack(gone),))


class PhotoViewSync(ViewSync):
    """사진뷰용 비교: 화면 행에 촬영일·태그·즐겨찾기를 싣고, 클라우드 사진도 목록에 둔다."""

    def __init__(self, store: PhotoStore, publish: Callable[[dict[str, Any]], None], *,
                 roots: Iterable[Any], excluded: Iterable[Any] = ()) -> None:
        super().__init__(store, publish, roots=roots, excluded=excluded)
        self.store = store

    def in_scope(self, path: str) -> bool:
        return self._in_scope(path_key(path))

    @property
    def seen_count(self) -> int:
        return len(self._seen)

    def _row(self, record: Record) -> dict[str, Any]:
        key, path, name, _, size, mtime, cloud, _ = record
        saved = self.store.saved.get(key)
        date, orientation = _carried(saved, size, mtime, cloud)
        return photo_row(path, name, date, size, mtime, cloud, orientation,
                         bool(saved and saved.favorite), saved.tags if saved else ())

    def _hidden(self, record: Record) -> bool:
        saved = self.store.saved.get(record[0])
        return bool(saved and saved.missing)

    def _changed(self, old: Record, name: str, size: int | None, mtime: float | None, cloud: bool) -> bool:
        saved = self.store.saved.get(old[0])
        if saved is not None and saved.missing:  # 감춰 둔 사진이 다시 나타났다.
            saved.missing = False
            return True
        return super()._changed(old, name, size, mtime, cloud)


def pending_photos(db: Any) -> list[str]:
    """촬영 정보를 아직 읽지 않은 사진(최근에 바뀐 것 먼저). 클라우드 전용·사라진 사진은 뺀다."""
    with db._lock, closing(db._connect()) as connection:
        packed = connection.execute(
            "SELECT json_group_array(path) FROM (SELECT path FROM photos WHERE cloud=0 AND missing=0"
            " AND IFNULL(date_taken,'')='' ORDER BY mtime DESC)").fetchone()[0]
    return json.loads(packed)


class _Run:
    def __init__(self, total: int) -> None:
        self.id = uuid.uuid4().hex
        self.cancel = threading.Event()
        self.total = total
        self.done = 0
        self.updated: list[str] = []
        self.again = False
        self.finished = False


class ExifOrganizer:
    """촬영 정보를 뒤에서 읽는 작업 하나. 화면·검색과 따로 돌고, 중단하면 남은 것부터 다시 한다."""

    def __init__(self, db: Any, reader: Callable[[str], tuple[datetime | None, int]],
                 pending: Callable[[], list[str]]) -> None:
        self.db = db
        self._reader = reader
        self._pending = pending
        self._lock = threading.RLock()
        self._run: _Run | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> int:
        """남은 사진 수를 돌려준다. 이미 돌고 있으면 끝날 때 새로 생긴 것까지 마저 한다."""
        todo = self._pending()
        with self._lock:
            run = self._run
            if run is not None and not run.finished and not run.cancel.is_set():
                run.again = True
                return max(0, run.total - run.done)
            if not todo:
                return 0
            run = self._run = _Run(len(todo))
            self._thread = threading.Thread(target=self._work, args=(run, todo),
                                            name="docfit-photo-exif", daemon=True)
            self._thread.start()
        return len(todo)

    def cancel(self) -> None:
        with self._lock:
            if self._run is not None:
                self._run.cancel.set()

    def wait(self, timeout: float | None = None) -> None:
        thread = self._thread
        if thread is not None:
            thread.join(timeout)

    def status(self, since: int = 0) -> dict[str, Any]:
        """진행 상태와, `since` 뒤에 촬영 정보를 채운 사진의 지금 값."""
        with self._lock:
            run = self._run
            if run is None:
                return {"ok": True, "running": False, "run": "", "total": 0, "done": 0,
                        "rows": [], "next": 0, "more": False}
            start = max(0, min(since, len(run.updated)))
            paths = run.updated[start:start + EXIF_PAGE_ROWS]
            response = {"ok": True, "running": not run.finished, "run": run.id, "total": run.total,
                        "done": run.done, "next": start + len(paths),
                        "more": start + len(paths) < len(run.updated)}
        response["rows"] = self._rows(paths)
        return response

    def _rows(self, paths: list[str]) -> list[dict[str, Any]]:
        if not paths:
            return []
        store = PhotoStore(self.db)
        with self.db._lock, closing(self.db._connect()) as connection:
            entries = store._read(connection, "WHERE path IN (SELECT value FROM json_each(?)) AND missing=0",
                                  (_pack(paths),))
        return [{**photo_row(path, name, s.date, s.size, s.mtime, s.cloud, s.orientation, s.favorite, s.tags),
                 "op": "info"} for path, name, s in entries]

    def _work(self, run: _Run, todo: list[str]) -> None:
        tried: set[str] = set()
        batch: list[tuple[Any, ...]] = []
        flushed = time.monotonic()
        try:
            while True:
                for path in todo:
                    if run.cancel.is_set():
                        break
                    tried.add(path)
                    found = self._read(path)
                    if found is not None:
                        batch.append(found)
                    run.done += 1
                    if len(batch) >= EXIF_BATCH or time.monotonic() - flushed >= EXIF_FLUSH_SECONDS:
                        self._save(run, batch)
                        batch, flushed = [], time.monotonic()
                self._save(run, batch)
                batch = []
                with self._lock:
                    again, run.again = run.again, False
                    if run.cancel.is_set() or not again:
                        run.finished = True
                        return
                todo = [path for path in self._pending() if path not in tried]
                run.total += len(todo)
        except Exception as exc:  # 저장 실패 등. 남은 것은 다음에 열 때 다시 한다.
            logger.warning("촬영 정보 정리를 멈췄습니다: %s", mask_log_text(exc))
        finally:
            run.finished = True

    def _read(self, path: str) -> tuple[Any, ...] | None:
        """(촬영일, 회전, 크기, 수정 시각, 클라우드, 경로). 지금 읽을 수 없으면 None."""
        try:
            st = os.stat(path)
        except (OSError, ValueError):
            return None
        # 읽기 직전의 속성으로 확인한다. 클라우드 전용 파일은 내용을 읽지 않는다.
        cloud = cloud_only(getattr(st, "st_file_attributes", 0))
        taken, orientation = (None, 1) if cloud else self._reader(path)
        date = taken.strftime("%Y-%m-%d %H:%M:%S") if taken else date_text(st.st_mtime)
        # 수정 시각조차 쓸 수 없는 파일도 다시 읽지 않게 자리 값을 남긴다.
        return (date or "1970-01-01 00:00:00", orientation, st.st_size, st.st_mtime, int(cloud), path)

    def _save(self, run: _Run, batch: list[tuple[Any, ...]]) -> None:
        if not batch:
            return
        with self.db._lock, closing(self.db._connect()) as connection, connection:
            connection.executemany(
                "UPDATE photos SET date_taken=?, orientation=?, size=?, mtime=?, cloud=?"
                " WHERE path=? AND IFNULL(date_taken,'')=''", batch)
        with self._lock:
            run.updated.extend(item[-1] for item in batch)
