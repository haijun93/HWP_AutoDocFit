"""문서뷰 목록 DB: 즉시 표시와 색인 비교·반영(DOCDB1) 시험."""
import os
import sqlite3
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app import library_scan_cache
from docfit_app.doc_grid_service import DocGridService
from docfit_app.library_db import LibraryDB, ViewSync


class Base(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name).resolve()
        self.root = self.base / "drive"
        self.root.mkdir()
        self.db = LibraryDB(self.base / "data" / "view.db")

    def file(self, name: str, data: bytes = b"x") -> Path:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def sync(self, paths, *, complete=True, excluded=(), roots=None):
        """색인 서비스처럼 경로·이름만 주는 원천으로 한 번 맞춘다."""
        events: list[dict] = []
        sync = ViewSync(self.db, events.append, roots=roots or [self.root], excluded=excluded)
        sync.publish_snapshot()
        snapshot = len(events)
        for path in paths:
            sync.feed({"path": str(path), "name": Path(path).name})
        sync.finish(complete)
        return events[:snapshot], events[snapshot:], sync

    def stored(self) -> list[str]:
        return [row[2] for row in self.db.load()]


class ViewSyncTests(Base):
    def test_second_open_shows_saved_rows_then_only_changes(self):
        kept, changed, gone = self.file("a.pdf"), self.file("b.pdf"), self.file("c.pdf")
        _, first, _ = self.sync([kept, changed, gone])
        self.assertEqual([e["name"] for e in first], ["a.pdf", "b.pdf", "c.pdf"])
        self.assertEqual((first[0]["size"], first[0]["cloud"]), (1, False))
        changed.write_bytes(b"longer")
        gone.unlink()
        added = self.file("sub/d.pdf")
        snapshot, delta, sync = self.sync([kept, changed, added])
        self.assertEqual([e["name"] for e in snapshot], ["a.pdf", "b.pdf", "c.pdf"])
        self.assertEqual([(e.get("op", "add"), Path(e["path"]).name) for e in delta],
                         [("update", "b.pdf"), ("add", "d.pdf"), ("remove", "c.pdf")])
        self.assertEqual(delta[0]["size"], 6)
        self.assertEqual(sync.counts, {"added": 1, "updated": 1, "removed": 1})
        self.assertEqual([item["name"] for item in sync.items()], ["a.pdf", "b.pdf", "d.pdf"])
        self.assertEqual(self.stored(), ["a.pdf", "b.pdf", "d.pdf"])

    def test_incomplete_or_empty_source_never_deletes(self):
        first, second = self.file("a.pdf"), self.file("b.pdf")
        self.sync([first, second])
        second.unlink()
        for paths, complete in (([first], False), ([], True), ([], False)):
            _, delta, _ = self.sync(paths, complete=complete)
            self.assertEqual(delta, [])
            self.assertEqual(self.stored(), ["a.pdf", "b.pdf"])
        _, delta, _ = self.sync([first])
        self.assertEqual([e["op"] for e in delta], ["remove"])
        self.assertEqual(self.stored(), ["a.pdf"])

    def test_file_missing_from_index_but_on_disk_is_kept(self):
        first, second = self.file("a.pdf"), self.file("b.pdf")
        self.sync([first, second])
        _, delta, sync = self.sync([first])
        self.assertEqual(delta, [])
        self.assertEqual(len(sync.items()), 2)

    def test_stale_index_row_of_a_deleted_file_is_removed(self):
        first, second = self.file("a.pdf"), self.file("b.pdf")
        self.sync([first, second])
        second.unlink()
        _, delta, _ = self.sync([first, second])  # 색인이 아직 지운 파일을 갖고 있다.
        self.assertEqual([e["op"] for e in delta], ["remove"])

    def test_disconnected_root_keeps_its_rows(self):
        other = self.base / "usb"
        other.mkdir()
        (other / "u.pdf").write_bytes(b"x")
        first = self.file("a.pdf")
        self.sync([first, other / "u.pdf"], roots=[self.root, other])
        (other / "u.pdf").unlink()
        other.rmdir()
        self.sync([first], roots=[self.root, other])
        self.assertEqual(self.stored(), ["a.pdf", "u.pdf"])

    def test_excluded_folder_is_hidden_and_returns_without_a_new_scan(self):
        first, hidden = self.file("a.pdf"), self.file("private/h.pdf")
        self.sync([first, hidden])
        snapshot, delta, _ = self.sync([first], excluded=[self.root / "private"])
        self.assertEqual([e["name"] for e in snapshot], ["a.pdf"])
        self.assertEqual(delta, [])
        snapshot, _, _ = self.sync([first, hidden])
        self.assertEqual([e["name"] for e in snapshot], ["a.pdf", "h.pdf"])

    def test_cloud_only_file_leaves_the_list(self):
        path = self.file("a.pdf")
        self.sync([path])
        events: list[dict] = []
        sync = ViewSync(self.db, events.append, roots=[self.root])
        sync.publish_snapshot()
        sync.feed({"path": str(path), "name": "a.pdf", "size": 1, "mtime": path.stat().st_mtime, "cloud": True})
        sync.finish(True)
        self.assertEqual(events[-1], {"op": "remove", "path": str(path)})
        self.assertEqual(self.sync([], complete=False)[0], [])

    def test_engine_difference_in_time_precision_is_not_a_change(self):
        path = self.file("a.pdf")
        self.sync([path])
        events: list[dict] = []
        sync = ViewSync(self.db, events.append, roots=[self.root])
        sync.publish_snapshot()
        sync.feed({"path": str(path), "name": "a.pdf", "size": 1, "mtime": int(path.stat().st_mtime), "cloud": False})
        sync.finish(True)
        self.assertEqual(len(events), 1)

    def test_forgotten_file_is_not_restored_by_a_running_comparison(self):
        first, second = self.file("a.pdf"), self.file("b.pdf")
        events: list[dict] = []
        sync = ViewSync(self.db, events.append, roots=[self.root])
        sync.feed({"path": str(first), "name": "a.pdf"})
        sync.forget([str(second)])
        sync.feed({"path": str(second), "name": "b.pdf"})
        sync.finish(True)
        self.assertEqual([item["name"] for item in sync.items()], ["a.pdf"])
        self.assertEqual(self.stored(), ["a.pdf"])


class LibraryDBTests(Base):
    def test_damaged_database_is_rebuilt(self):
        self.db.path.parent.mkdir(parents=True)
        self.db.path.write_bytes(b"this is not a database" * 100)
        self.assertEqual(self.db.load(), [])
        self.sync([self.file("a.pdf")])
        self.assertEqual(self.stored(), ["a.pdf"])

    def test_other_schema_version_is_rebuilt(self):
        self.sync([self.file("a.pdf")])
        connection = sqlite3.connect(self.db.path)
        connection.execute("PRAGMA user_version=99")
        connection.commit()
        connection.close()
        self.assertEqual(self.db.load(), [])
        self.sync([self.root / "a.pdf"])
        self.assertEqual(self.stored(), ["a.pdf"])

    def test_locked_database_is_not_deleted(self):
        self.sync([self.file("a.pdf")])
        with patch.object(LibraryDB, "_connection", side_effect=sqlite3.OperationalError("database is locked")), \
             patch.object(LibraryDB, "_remove", side_effect=AssertionError("잠긴 DB 삭제")):
            self.assertEqual(self.db.load(), [])
        self.assertEqual(self.stored(), ["a.pdf"])


class ServiceTests(Base):
    def setUp(self):
        super().setUp()
        patcher = patch.object(library_scan_cache, "_data_root", lambda: self.base / "data")
        patcher.start()
        self.addCleanup(patcher.stop)
        env = patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE": ""})
        env.start()
        self.addCleanup(env.stop)
        self.service = DocGridService(thumb_dir=self.base / "data" / "thumbs", drive_roots=[self.root])

    def run_scan(self, rows):
        """색인 서비스가 `rows` 를 주는 검색 한 번."""
        def names(extensions, excluded, *, on_row=None):
            for row in rows:
                on_row(row)
            return rows

        with patch("docfit_app.doc_grid_service.query_names_service", side_effect=names), \
             patch("docfit_app.doc_grid_service._scan_default_roots", return_value=None):
            started = self.service.command("list_default")
            self.assertTrue(started["ok"])
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                state = self.service.command("scan_progress", {"since": 0})
                if state["state"] == "complete" and state["next"] >= state["total"]:
                    return started, state
                time.sleep(0.005)
        self.fail("검색이 끝나지 않았습니다.")

    @staticmethod
    def names(*paths):
        return [{"path": str(p), "name": p.name, "ext": p.suffix[1:]} for p in paths]

    def test_reopen_starts_from_database_and_reports_changes(self):
        first, second = self.file("a.pdf"), self.file("b.pdf")
        started, state = self.run_scan(self.names(first, second))
        self.assertFalse(started["from_db"])
        self.assertNotIn("documents", state["result"])
        second.unlink()
        started, state = self.run_scan(self.names(first))
        self.assertTrue(started["from_db"])
        self.assertEqual([(r.get("op", "add"), Path(r["path"]).name) for r in state["rows"]],
                         [("add", "a.pdf"), ("add", "b.pdf"), ("remove", "b.pdf")])
        self.assertEqual(state["result"]["changes"], {"added": 0, "updated": 0, "removed": 1})
        full = self.service.command("scan_progress")["result"]["documents"]
        self.assertEqual([item["name"] for item in full], ["a.pdf"])

    def test_empty_index_answer_keeps_the_database(self):
        first = self.file("a.pdf")
        self.run_scan(self.names(first))
        first.unlink()
        self.run_scan([])
        self.assertEqual([row[2] for row in self.service.db.load()], ["a.pdf"])

    def test_old_saved_list_is_imported_once(self):
        path = self.file("a.pdf")
        library_scan_cache.save("documents", {".hwp", ".hwpx", ".xls", ".xlsx", ".ppt", ".pptx", ".pdf"}, [],
                                [{"path": str(path), "name": "a.pdf"}], roots=[str(self.root)])
        started, state = self.run_scan(self.names(path))
        self.assertTrue(started["from_db"])
        self.assertEqual(state["rows"][0]["name"], "a.pdf")
        self.assertEqual([row[4] for row in self.service.db.load()], [1])

    def test_thumbnail_request_records_its_key(self):
        path = self.file("a.pdf", b"%PDF-1.4\n%%EOF")
        self.run_scan(self.names(path))
        result = self.service.get_thumbnail(path)
        self.assertEqual(self.service.db.load()[0][7], result["key"])


if __name__ == "__main__":
    unittest.main()
