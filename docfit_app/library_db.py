"""문서뷰·사진뷰 목록 DB와 색인 비교·반영(DOCDB1).

파일 목록의 원천은 검색 색인(서비스, 없으면 klib)이고, 이 DB는 화면에 바로 보여 줄 정보만 담는 캐시다.
화면을 열면 DB 목록을 먼저 내보내고, 색인 결과와 비교해 추가·변경·삭제만 다시 내보낸다.

삭제는 원천이 완전할 때(`finish(complete=True)` 이고 행이 하나라도 왔을 때)만, 디스크에 실제로
없는 것이 확인된 파일에 한한다. 원천이 완전하지 않거나 비어 있으면 아무것도 지우지 않는다.

속도: sqlite3 와 `os.path.normcase` 는 행마다 파이썬 실행 잠금을 놓아, 화면 폴링 스레드와 함께 돌면
몇 배 느려진다. 그래서 DB 는 묶음을 SQL 문 하나(json_each)로 읽고 쓰며, 키는 문자열 연산으로 만든다.
"""
from __future__ import annotations

import json
import logging
import os
import sqlite3
import threading
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Callable, Iterable, Iterator

from docfit_core.log_mask import mask_log_text
from docfit_core.timemachine.rules import cloud_only

logger = logging.getLogger(__name__)

SCHEMA_VERSION = 1
# 색인 엔진마다 시각 정밀도가 달라, 이보다 작은 차이는 같은 파일로 본다.
MTIME_TOLERANCE = 1.0

_SCHEMA = """
CREATE TABLE IF NOT EXISTS items(
  path_key TEXT PRIMARY KEY, path TEXT NOT NULL, name TEXT NOT NULL, name_key TEXT NOT NULL,
  ext TEXT NOT NULL, folder TEXT NOT NULL, size INTEGER, mtime REAL, cloud INTEGER NOT NULL DEFAULT 0,
  thumb_key TEXT NOT NULL DEFAULT '', seen_at REAL NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS items_name ON items(name_key);
"""

# (path_key, path, name, ext, size, mtime, cloud, thumb_key)
Record = tuple


if os.name == "nt":
    def path_key(path: str) -> str:
        return path.replace("/", "\\").lower()
else:
    def path_key(path: str) -> str:
        return path


def _storable(text: str) -> bool:
    try:
        text.encode("utf-8")
        return True
    except UnicodeEncodeError:
        return False


def view_row(record: Record) -> dict[str, Any]:
    """화면에 넘기는 행."""
    _, path, name, ext, size, mtime, cloud, thumb_key = record
    return {"path": path, "name": name, "stem": os.path.splitext(name)[0], "ext": ext,
            "size": size or 0, "mtime": mtime or 0, "cloud": bool(cloud),
            "thumb_key": thumb_key, "has_thumb": False}


class LibraryDB:
    """목록 DB 파일 하나. 손상되었거나 다른 버전이면 지우고 새로 만든다(원본 파일과 무관한 캐시)."""

    def __init__(self, path: Path | str) -> None:
        self.path = Path(path)
        self._lock = threading.RLock()

    @contextmanager
    def _connection(self) -> Iterator[sqlite3.Connection]:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        connection = sqlite3.connect(self.path, timeout=5.0)
        try:
            version = connection.execute("PRAGMA user_version").fetchone()[0]
            if version != SCHEMA_VERSION:
                connection.execute("DROP TABLE IF EXISTS items")
                connection.executescript(_SCHEMA)
                connection.execute(f"PRAGMA user_version={SCHEMA_VERSION}")
                connection.execute("PRAGMA journal_mode=WAL")
            connection.execute("PRAGMA synchronous=NORMAL")
            with connection:
                yield connection
        finally:
            connection.close()

    def _run(self, work: Callable[[sqlite3.Connection], Any], default: Any = None) -> Any:
        """DB 작업 하나. 손상이면 새로 만들어 한 번 더 하고, 그 밖의 실패는 `default`."""
        with self._lock:
            for attempt in (0, 1):
                try:
                    with self._connection() as connection:
                        return work(connection)
                except sqlite3.OperationalError as exc:
                    # 잠금·권한·디스크 문제는 파일을 지울 이유가 아니다.
                    logger.warning("목록 DB 사용 실패: %s", mask_log_text(exc))
                    return default
                except sqlite3.DatabaseError as exc:
                    # 손상은 DatabaseError 그대로 온다. 하위 종류(제약·사용 오류)로는 지우지 않는다.
                    if type(exc) is not sqlite3.DatabaseError:
                        logger.warning("목록 DB 사용 실패: %s", mask_log_text(exc))
                        return default
                    logger.warning("목록 DB 손상, 새로 만듭니다: %s", mask_log_text(exc))
                    if attempt or not self._remove():
                        return default
                except (OSError, OverflowError) as exc:
                    logger.warning("목록 DB 사용 실패: %s", mask_log_text(exc))
                    return default
        return default

    def _remove(self) -> bool:
        try:
            for suffix in ("", "-wal", "-shm", "-journal"):
                Path(f"{self.path}{suffix}").unlink(missing_ok=True)
            return True
        except OSError as exc:
            logger.warning("손상된 목록 DB 를 지우지 못했습니다: %s", mask_log_text(exc))
            return False

    def has_rows(self) -> bool:
        return bool(self._run(lambda c: c.execute("SELECT 1 FROM items LIMIT 1").fetchone(), None))

    def load(self) -> list[Record]:
        """저장된 전체 목록(이름순)."""
        packed = self._run(lambda c: c.execute(
            "SELECT json_group_array(json_array(path_key,path,name,ext,size,mtime,cloud,thumb_key)) FROM"
            " (SELECT * FROM items ORDER BY name_key)").fetchone()[0], "[]")
        return [tuple(record) for record in json.loads(packed)]

    def write(self, upserts: Iterable[Record] = (), removes: Iterable[str] = (),
              seen: Iterable[str] = ()) -> None:
        # 저장할 수 없는 이름(짝 없는 대리 문자) 하나가 묶음 전체를 막지 않게 뺀다.
        now = time.time()
        rows = [(key, path, name, name.casefold(), ext, os.path.dirname(path), size, mtime, int(bool(cloud)),
                 thumb_key, now)
                for key, path, name, ext, size, mtime, cloud, thumb_key in upserts if _storable(path)]
        removes, seen = [k for k in removes if _storable(k)], [k for k in seen if _storable(k)]
        if not (rows or removes or seen):
            return
        columns = ",".join(f"json_extract(value,'$[{index}]')" for index in range(11))

        def pack(values: list) -> str:
            return json.dumps(values, ensure_ascii=False, separators=(",", ":"))

        def work(c: sqlite3.Connection) -> None:
            if removes:
                c.execute("DELETE FROM items WHERE path_key IN (SELECT value FROM json_each(?))", (pack(removes),))
            if rows:
                c.execute("INSERT OR REPLACE INTO items(path_key,path,name,name_key,ext,folder,size,mtime,cloud,"
                          f"thumb_key,seen_at) SELECT {columns} FROM json_each(?)", (pack(rows),))
            if seen:
                c.execute("UPDATE items SET seen_at=? WHERE path_key IN (SELECT value FROM json_each(?))",
                          (now, pack(seen)))

        self._run(work)

    def set_thumb_key(self, path: str, thumb_key: str) -> None:
        self._run(lambda c: c.execute("UPDATE items SET thumb_key=? WHERE path_key=? AND thumb_key<>?",
                                      (thumb_key, path_key(path), thumb_key)))

    def import_rows(self, rows: Iterable[dict[str, Any]]) -> None:
        """이전 버전의 저장 목록(JSON)을 한 번 옮긴다."""
        records = []
        for row in rows:
            path, name = row.get("path"), row.get("name")
            if not isinstance(path, str) or not isinstance(name, str):
                continue
            size, mtime = row.get("size"), row.get("mtime")
            records.append((path_key(path), path, name, os.path.splitext(name)[1].lstrip(".").lower(),
                            int(size) if isinstance(size, (int, float)) else None,
                            mtime if isinstance(mtime, (int, float)) else None,
                            bool(row.get("cloud")), ""))
        self.write(records)


_MISSING = "missing"
_UNKNOWN = "unknown"


class FolderStats:
    """파일마다 stat 하지 않고 폴더를 한 번씩만 훑어 크기·수정 시각·클라우드 여부를 얻는다."""

    def __init__(self) -> None:
        self._folders: dict[str, Any] = {}

    def stat(self, path: str) -> Any:
        """`(size, mtime, cloud)`, 없으면 `_MISSING`, 확인할 수 없으면 `_UNKNOWN`."""
        parent, name = os.path.split(path)
        if not parent or not name:
            return _UNKNOWN
        entries = self._folders.get(parent)
        if entries is None:
            entries = self._folders[parent] = self._scan(parent)
        if isinstance(entries, str):
            return entries
        return entries.get(path_key(name), _MISSING)

    @staticmethod
    def _scan(parent: str) -> Any:
        found: dict[str, Any] = {}
        try:
            with os.scandir(parent) as iterator:
                for entry in iterator:
                    key = path_key(entry.name)
                    try:
                        if not entry.is_file():
                            continue
                        st = entry.stat(follow_symlinks=False)
                        found[key] = (st.st_size, st.st_mtime,
                                      cloud_only(getattr(st, "st_file_attributes", 0)))
                    except OSError:
                        found[key] = _UNKNOWN
        except (FileNotFoundError, NotADirectoryError):
            return _MISSING
        except OSError:
            return _UNKNOWN
        return found


def _prefixes(folders: Iterable[Any]) -> tuple[str, ...]:
    return tuple(path_key(os.path.normpath(str(f))).rstrip("\\/") + os.sep for f in folders)


class ViewSync:
    """한 번의 검색: DB 목록 내보내기 → 원천 행 비교(`feed`) → 마무리(`finish`).

    `publish` 로 나가는 항목은 추가(행), 변경(행 + `op: update`), 삭제(`op: remove`, `path`)다.
    """

    def __init__(self, db: LibraryDB, publish: Callable[[dict[str, Any]], None], *,
                 roots: Iterable[Any], excluded: Iterable[Any] = ()) -> None:
        self.db = db
        self._publish = publish
        self._roots = tuple(str(r) for r in roots)
        self._root_prefixes = _prefixes(self._roots)
        self._excluded_prefixes = _prefixes(excluded)
        self._lock = threading.RLock()
        self._known: dict[str, Record] = {}
        self._shown: dict[str, dict[str, Any]] = {}
        self._seen: set[str] = set()
        self._gone: set[str] = set()
        self._pending: dict[str, Record] = {}
        self._stats = FolderStats()
        self.counts = {"added": 0, "updated": 0, "removed": 0}

    # 사진뷰(`photo_library`)가 바꾸는 부분: 화면 행의 모양과 목록에서 감출 항목.
    def _row(self, record: Record) -> dict[str, Any]:
        return view_row(record)

    def _hidden(self, record: Record) -> bool:
        return bool(record[6])  # 클라우드 전용 문서는 목록에 두지 않는다.

    def _in_scope(self, key: str) -> bool:
        return key.startswith(self._root_prefixes) and not (
            self._excluded_prefixes and key.startswith(self._excluded_prefixes))

    def publish_snapshot(self) -> int:
        """저장된 목록 가운데 지금 범위의 것을 내보낸다."""
        records = self.db.load()
        with self._lock:
            for record in records:
                key = record[0]
                self._known[key] = record
                if self._hidden(record) or key in self._gone or not self._in_scope(key):
                    continue
                row = self._shown[key] = self._row(record)
                self._publish(row)
            return len(self._shown)

    def feed(self, row: dict[str, Any]) -> None:
        """원천 행 하나. 메타데이터가 없거나 시각이 미상이면 폴더를 훑어 채운다."""
        path, name = row["path"], row["name"]
        key = path_key(path)
        with self._lock:
            if key in self._seen or key in self._gone:
                return
            old = self._known.get(key)
            # 신형 서비스의 미상 시각은 0이다. 빈 파일(size=0)의 유효한 시각은 그대로 쓴다.
            if row.get("size") is not None and row.get("mtime") not in (None, 0):
                size, mtime, cloud = int(row["size"]), row["mtime"], bool(row.get("cloud"))
            else:
                found = self._stats.stat(path)
                if found == _MISSING:
                    return  # 색인에 남은 지워진 파일
                if found == _UNKNOWN:
                    if old is not None:
                        self._seen.add(key)
                        return
                    size, mtime, cloud = None, None, bool(row.get("cloud"))
                else:
                    size, mtime, cloud = found
            self._seen.add(key)
            changed = old is None or self._changed(old, name, size, mtime, cloud)
            if changed:
                ext = os.path.splitext(name)[1].lstrip(".").lower()
                # 대소문자만 다른 경로 표기(엔진 차이)는 저장된 표기를 유지한다.
                stored = old[1] if old is not None and old[2] == name else path
                record = (key, stored, name, ext, size, mtime, cloud, "")
                self._known[key] = self._pending[key] = record
            else:
                record = old
            self._show(key, record, old, changed)

    def _changed(self, old: Record, name: str, size: int | None, mtime: float | None, cloud: bool) -> bool:
        if old[2] != name or old[4] != size or bool(old[6]) != cloud:
            return True
        if old[5] is None or mtime is None:
            return old[5] is not mtime
        return abs(old[5] - mtime) >= MTIME_TOLERANCE

    def _show(self, key: str, record: Record, old: Record | None, changed: bool) -> None:
        shown = self._shown.get(key)
        if self._hidden(record):
            if shown is not None:
                del self._shown[key]
                self._remove_event(shown["path"])
            return
        if shown is None:
            row = self._shown[key] = self._row(record)
            self.counts["added"] += 1
            self._publish(row)
        elif changed:
            row = self._shown[key] = self._row(record)
            self.counts["updated"] += 1
            event = {**row, "op": "update"}
            if old is not None and old[1] != row["path"]:
                event["old_path"] = old[1]
            self._publish(event)

    def _remove_event(self, path: str) -> None:
        self.counts["removed"] += 1
        self._publish({"op": "remove", "path": path})

    def finish(self, complete: bool) -> None:
        """변경을 한 번에 저장한다. 원천이 완전하면 디스크에서 사라진 파일을 지운다.

        검색 중에 나눠 저장하면 연결을 닫을 때마다 WAL 을 합쳐 몇 배 느려진다. 취소·실패 때도 부른다.
        """
        with self._lock:
            removes: list[str] = []
            if complete and self._seen:
                # 지금 연결된 루트 아래만 본다. 빠진 드라이브의 항목은 남겨 둔다.
                available = _prefixes(root for root in self._roots if os.path.isdir(root))
                stats = FolderStats()
                for key, record in self._known.items():
                    if key in self._seen or key in self._gone or not key.startswith(available):
                        continue
                    if stats.stat(record[1]) == _MISSING:
                        removes.append(key)
                for key in removes:
                    del self._known[key]
                    shown = self._shown.pop(key, None)
                    if shown is not None:
                        self._remove_event(shown["path"])
            pending, self._pending = list(self._pending.values()), {}
            # 삭제 판단은 위의 _seen만 사용한다. 읽는 곳이 없는 seen_at을 확인한 모든 행에
            # 다시 쓰면 변경 없는 열기에도 전체 DB를 갱신한다. 추가·변경·삭제만 저장한다.
            self.db.write(pending, removes=removes)

    def items(self) -> list[dict[str, Any]]:
        with self._lock:
            return sorted(self._shown.values(), key=lambda item: item["name"].casefold())

    def forget(self, paths: Iterable[str]) -> None:
        """검색 중에 옮기거나 지운 파일을 비교 결과가 되살리지 않게 한다."""
        with self._lock:
            for path in paths:
                key = path_key(path)
                self._gone.add(key)
                self._known.pop(key, None)
                self._shown.pop(key, None)
                self._pending.pop(key, None)

    def adopt(self, record: Record) -> None:
        """검색 중에 파일 작업으로 생긴 항목."""
        with self._lock:
            key = record[0]
            self._gone.discard(key)
            self._known[key] = record
            self._seen.add(key)
            if not self._hidden(record):
                self._shown[key] = self._row(record)


def file_record(path: str) -> Record | None:
    """파일 작업으로 생긴 파일의 DB 행."""
    try:
        st = os.lstat(path)
    except OSError:
        return None
    name = os.path.basename(path)
    return (path_key(path), path, name, os.path.splitext(name)[1].lstrip(".").lower(), st.st_size,
            st.st_mtime, cloud_only(getattr(st, "st_file_attributes", 0)), "")


def apply_file_ops(db: LibraryDB, sync: ViewSync | None, op: str, results: Iterable[dict[str, Any]]) -> None:
    """성공한 파일 작업(FILEOPS1)을 DB 와 진행 중인 비교에 반영한다."""
    gone: list[str] = []
    added: list[Record] = []
    for item in results:
        if not item.get("ok"):
            continue
        if op != "copy":
            gone.append(str(item["path"]))
        if item.get("listed") and "new_path" in item:
            record = file_record(item["new_path"])
            if record is not None:
                added.append(record)
    if sync is not None:
        sync.forget(gone)
        for record in added:
            sync.adopt(record)
    db.write(added, removes=[path_key(path) for path in gone])
