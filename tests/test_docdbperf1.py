"""DOCDBPERF1: 변경 없는 확인의 DB 쓰기 증폭과 삭제 안전 조건을 합성 자료로 검증한다."""
from contextlib import closing
import os
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.library_db import LibraryDB, ViewSync


class DocumentComparisonPersistenceTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.files = self.root / "files"
        self.files.mkdir()
        self.db = LibraryDB(self.root / "view.db")

    def file(self, name, data=b"synthetic"):
        path = self.files / name
        path.write_bytes(data)
        os.utime(path, (1700000000, 1700000000))
        return path

    def compare(self, paths, *, complete=True, stamp=100):
        events = []
        sync = ViewSync(self.db, events.append, roots=[self.files])
        sync.publish_snapshot()
        for path in paths:
            stat = path.stat()
            sync.feed(dict(path=str(path), name=path.name, size=stat.st_size,
                           mtime=stat.st_mtime, cloud=False))
        with patch("docfit_app.library_db.time.time", return_value=stamp):
            sync.finish(complete)
        return events, sync

    def stamps(self):
        with closing(sqlite3.connect(self.db.path)) as connection:
            return dict(connection.execute("SELECT name, seen_at FROM items"))

    def test_unchanged_confirmation_does_not_rewrite_database(self):
        paths = [self.file(f"item{index}.pdf") for index in range(20)]
        _, original = self.compare(paths)
        before = self.db.path.read_bytes()
        for complete in (True, False):
            events, current = self.compare(paths, complete=complete, stamp=200)
            self.assertEqual(current.items(), original.items())
            self.assertEqual(events, original.items())
            self.assertEqual(current.counts, dict(added=0, updated=0, removed=0))
            self.assertEqual(self.db.path.read_bytes(), before)
            self.assertEqual(set(self.stamps().values()), {100})

    def test_only_changes_are_persisted_without_weakening_deletion_guards(self):
        kept, changed, gone, unindexed = [self.file(name + ".pdf") for name in
                                          ("kept", "changed", "gone", "unindexed")]
        self.compare([kept, changed, gone, unindexed])
        changed.write_bytes(b"synthetic changed")
        gone.unlink()
        added = self.file("added.pdf")
        for paths, complete in (([kept, changed, added], False), ([], True)):
            _, current = self.compare(paths, complete=complete, stamp=200)
            self.assertIn("gone.pdf", self.stamps())
            self.assertEqual(current.counts["removed"], 0)
        events, current = self.compare([kept, changed, added], stamp=300)
        self.assertEqual([event for event in events if event.get("op") == "remove"],
                         [dict(op="remove", path=str(gone))])
        self.assertEqual(self.stamps(), {"kept.pdf": 100, "unindexed.pdf": 100,
                                         "changed.pdf": 200, "added.pdf": 200})
        self.assertEqual({row["name"] for row in current.items()},
                         {"kept.pdf", "changed.pdf", "added.pdf", "unindexed.pdf"})


if __name__ == "__main__":
    unittest.main()
