"""사진뷰 DB 즉시 표시·증분 반영·촬영 정보 배경 정리(PHOTOSYNC1) 시험. 합성 파일·DB 만 쓴다."""
import os
import sqlite3
import tempfile
import threading
import time
import unittest
from contextlib import closing
from datetime import datetime
from pathlib import Path
from unittest.mock import patch

from docfit_app.photo_album_service import PhotoAlbumService

TAKEN = (datetime(2020, 5, 6, 7, 8, 9), 6)
READER = "docfit_app.photo_album_service.read_exif_metadata"

OLD_SCHEMA = """
CREATE TABLE registered_folders (path TEXT PRIMARY KEY, registered_at TEXT, last_scanned_at TEXT);
CREATE TABLE photos (path TEXT PRIMARY KEY, folder TEXT, filename TEXT, size INTEGER, mtime REAL,
  date_taken TEXT, orientation INTEGER DEFAULT 1, favorite INTEGER DEFAULT 0, cloud INTEGER DEFAULT 0);
CREATE TABLE tags (photo_path TEXT, tag TEXT, PRIMARY KEY (photo_path, tag),
  FOREIGN KEY (photo_path) REFERENCES photos(path) ON DELETE CASCADE);
"""


class Base(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name).resolve()
        self.root = self.base / "drive"
        self.root.mkdir()
        self.db_path = self.base / "photo_album.db"
        env = patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE": "python"})
        env.start()
        self.addCleanup(env.stop)
        self.services: list[PhotoAlbumService] = []
        self.addCleanup(self._stop)

    def _stop(self):
        for service in self.services:
            service.exif.cancel()
            service.exif.wait(10)

    def service(self) -> PhotoAlbumService:
        service = PhotoAlbumService(db_path=self.db_path, thumb_dir=self.base / "thumbs", drive_roots=[self.root])
        self.services.append(service)
        return service

    def file(self, name: str, data: bytes = b"synthetic") -> Path:
        path = self.root / name
        path.write_bytes(data)
        return path

    def scan(self, service):
        """검색 한 번(화면과 같은 since 방식)."""
        started = service.command("scan_default")
        self.assertTrue(started["ok"], started)
        rows, since = [], 0
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            state = service.command("scan_progress", {"since": since})
            rows.extend(state["rows"])
            since = state["next"]
            if state["state"] not in ("running", "cancelling") and since >= state["total"]:
                self.assertEqual(state["state"], "complete", state)
                return started, rows, state["result"]
            time.sleep(0.005)
        self.fail("검색이 끝나지 않았습니다.")

    def stored(self, sql: str):
        with closing(sqlite3.connect(self.db_path)) as connection:
            return connection.execute(sql).fetchall()


class UpgradeTests(Base):
    def old_database(self, rows, tags=()):
        with closing(sqlite3.connect(self.db_path)) as connection, connection:
            connection.executescript(OLD_SCHEMA)
            connection.executemany("INSERT INTO photos VALUES (?,?,?,?,?,?,?,?,0)", rows)
            connection.executemany("INSERT INTO tags VALUES (?,?)", tags)

    def old_row(self, path: Path, date="2019-01-02 03:04:05", favorite=0, shown: str | None = None):
        st = path.stat()
        text = shown or str(path)
        return (text, os.path.dirname(text), path.name, st.st_size, st.st_mtime, date, 3, favorite)

    def test_old_database_is_shown_and_kept_without_rereading_unchanged_photos(self):
        path = self.file("a.jpg")
        self.old_database([self.old_row(path, favorite=1)], [(str(path), "행사")])
        service = self.service()
        with patch(READER, side_effect=AssertionError("바뀌지 않은 사진을 다시 읽음")):
            started, rows, result = self.scan(service)
            service.exif.wait(10)
        self.assertTrue(started["from_db"])
        self.assertEqual([(r["name"], r["date_taken"], r["favorite"], r["tags"], r.get("op")) for r in rows],
                         [("a.jpg", "2019-01-02 03:04:05", True, ["행사"], None)])
        self.assertEqual((result["added"], result["updated"], result["removed"], result["exif"]["total"]),
                         (0, 0, 0, 0))
        self.assertEqual(self.stored("SELECT favorite, orientation, missing FROM photos"), [(1, 3, 0)])

    @unittest.skipUnless(os.name == "nt", "Windows 경로 표기")
    def test_rows_differing_only_in_case_are_merged_with_their_marks(self):
        path = self.file("a.jpg")
        lowered = str(path).lower()
        self.old_database([self.old_row(path), self.old_row(path, date="", favorite=1, shown=lowered)],
                          [(str(path), "하나"), (lowered, "둘")])
        service = self.service()
        _, rows, _ = self.scan(service)
        self.assertEqual(len(rows), 1)
        self.assertEqual((rows[0]["path"], rows[0]["favorite"], sorted(rows[0]["tags"])),
                         (str(path), True, ["둘", "하나"]))
        self.assertEqual(len(self.stored("SELECT path FROM photos")), 1)


class IncrementalTests(Base):
    def test_only_new_and_changed_photos_are_read_again(self):
        keep, change, gone = self.file("keep.jpg"), self.file("change.jpg"), self.file("gone.jpg")
        service = self.service()
        with patch(READER, return_value=TAKEN):
            self.scan(service)
            service.exif.wait(10)
        change.write_bytes(b"changed content")
        gone.unlink()
        added = self.file("new.jpg")
        read: list[str] = []
        with patch(READER, side_effect=lambda p: read.append(os.path.basename(p)) or TAKEN):
            _, rows, result = self.scan(self.service())
            self.services[-1].exif.wait(10)
        self.assertEqual(sorted(read), sorted([change.name, added.name]))
        self.assertEqual((result["added"], result["updated"], result["removed"]), (1, 1, 1))
        events = {(r.get("op", "add"), os.path.basename(r["path"])) for r in rows}
        self.assertTrue({("update", change.name), ("add", added.name), ("remove", gone.name)} <= events)
        self.assertEqual(sorted(r[0] for r in self.stored("SELECT filename FROM photos")),
                         sorted([keep.name, change.name, added.name]))

    def test_missing_photo_with_marks_is_hidden_not_deleted_and_comes_back(self):
        marked, other = self.file("marked.jpg"), self.file("other.jpg")
        service = self.service()
        with patch(READER, return_value=TAKEN):
            self.scan(service)
            service.exif.wait(10)
            service.add_tag(marked, "보존")
            st = marked.stat()
            data = marked.read_bytes()
            marked.unlink()
            self.scan(service)
            self.assertEqual([p["name"] for g in service.list_by_date() for p in g["items"]], [other.name])
            self.assertEqual(service.search("marked"), [])
            self.assertEqual(service.list_tags()["tags"], [])
            _, rows, _ = self.scan(service)  # 감춘 상태에서 다시 열어도 목록에 없다.
            self.assertEqual([r["name"] for r in rows], [other.name])
            marked.write_bytes(data)
            os.utime(marked, ns=(st.st_atime_ns, st.st_mtime_ns))
        with patch(READER, side_effect=AssertionError("내용이 같은데 다시 읽음")):
            _, rows, _ = self.scan(service)
        back = [r for r in rows if r["name"] == marked.name]
        self.assertEqual([r["tags"] for r in back], [["보존"]])
        self.assertEqual(self.stored("SELECT missing FROM photos WHERE filename='marked.jpg'"), [(0,)])

    def test_cloud_photo_is_listed_but_never_read(self):
        self.file("cloud.jpg")
        service = self.service()
        with patch("docfit_app.photo_album_service.cloud_only", return_value=True), \
             patch(READER, side_effect=AssertionError("클라우드 파일 읽기")):
            _, rows, result = self.scan(service)
        self.assertEqual([(r["name"], r["cloud"]) for r in rows], [("cloud.jpg", True)])
        self.assertEqual(result["exif"]["total"], 0)

    def test_name_and_tag_search_reads_only_the_database(self):
        path = self.file("find-me.jpg")
        service = self.service()
        with patch(READER, return_value=TAKEN):
            self.scan(service)
            service.exif.wait(10)
        service.add_tag(path, "현장")
        with patch("os.stat", side_effect=AssertionError("검색 중 stat")), \
             patch("os.scandir", side_effect=AssertionError("검색 중 폴더 훑기")):
            by_name = service.search("find")
            by_tag = service.search(tag="현장")
        self.assertEqual([r["name"] for r in by_name], [path.name])
        self.assertEqual([r["name"] for r in by_tag], [path.name])


class OrganizerTests(Base):
    def test_screen_and_search_are_usable_while_reading_and_stop_resumes_later(self):
        names = [f"{index}.jpg" for index in range(5)]
        for name in names:
            self.file(name)
        entered, release = threading.Event(), threading.Event()
        read: list[str] = []

        def slow(path):
            entered.set()
            release.wait(5)
            read.append(os.path.basename(path))
            return TAKEN

        first = self.service()
        with patch(READER, side_effect=slow):
            _, rows, result = self.scan(first)  # 비교는 촬영 정보를 기다리지 않고 끝난다.
            self.assertEqual((len(rows), result["exif"]["total"]), (5, 5))
            self.assertTrue(entered.wait(5))
            status = first.command("exif_status", {"since": 0})
            self.assertEqual((status["running"], status["total"], status["done"]), (True, 5, 0))
            self.assertEqual(len(first.search("")), 5)
            first.command("exif_cancel")
            release.set()
            first.exif.wait(10)
        self.assertEqual(len(read), 1)  # 중단 전에 읽던 한 장은 저장된다.
        self.assertEqual(len(self.stored("SELECT 1 FROM photos WHERE date_taken=''")), 4)

        second = self.service()  # 앱을 다시 켠 것과 같다.
        resumed: list[str] = []
        with patch(READER, side_effect=lambda p: resumed.append(os.path.basename(p)) or TAKEN):
            started, rows, result = self.scan(second)
            second.exif.wait(10)
        self.assertTrue(started["from_db"])
        self.assertEqual(sorted(read + resumed), names)
        self.assertEqual(result["exif"]["total"], 4)
        status = second.command("exif_status", {"since": 0})
        self.assertEqual((status["running"], status["done"], status["next"]), (False, 4, 4))
        self.assertEqual({(r["op"], r["date_taken"], r["orientation"]) for r in status["rows"]},
                         {("info", "2020-05-06 07:08:09", 6)})

    def test_file_operation_during_compare_is_not_undone(self):
        path = self.file("a.jpg")
        other = self.file("b.jpg")
        service = self.service()
        with patch(READER, return_value=TAKEN):
            self.scan(service)
            service.exif.wait(10)
        other.write_bytes(b"changed so the compare has something to store")
        reached, release = threading.Event(), threading.Event()
        compare = service._compare_default

        def slow_compare(sync, **kwargs):
            engine = compare(sync, **kwargs)
            reached.set()
            release.wait(5)
            return engine

        service.file_ops.recycler = lambda target: target.unlink()
        with patch.object(service, "_compare_default", side_effect=slow_compare), patch(READER, return_value=TAKEN):
            service.command("scan_default")
            self.assertTrue(reached.wait(5))
            started = service.command("file_op_start", {"op": "delete", "paths": [str(path)]})
            self.assertTrue(started["ok"], started)
            service.file_ops.wait(10)
            release.set()
            deadline = time.monotonic() + 10
            while service.command("scan_progress")["state"] != "complete" and time.monotonic() < deadline:
                time.sleep(0.005)
            service.exif.wait(10)
        self.assertEqual([r[0] for r in self.stored("SELECT filename FROM photos")], [other.name])


if __name__ == "__main__":
    unittest.main()
