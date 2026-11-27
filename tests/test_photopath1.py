"""실제 Rust Library와 Python PhotoAlbumDb의 합성 DB 왕복(Windows 경로 별칭)."""
from __future__ import annotations

import ctypes
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest
from contextlib import closing

from docfit_app.photo_album_service import PhotoAlbumDb
from docfit_app.photo_library import PhotoStore, PhotoViewSync
from docfit_app.library_db import file_record


@unittest.skipUnless(os.name == "nt", "Windows 경로 별칭")
@unittest.skipUnless(os.environ.get("PHOTOPATH1_RUST_PEER"), "빌드한 path_peer 예제 지정 필요")
class PhotoPathInteropTest(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="photopath1-")
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name)
        self.real = self.base / "Synthetic Long Photo Folder"
        self.real.mkdir()
        self.photo = self.real / "Synthetic Long Photo Name.JPG"
        self.photo.write_bytes(b"synthetic-photo-only")
        self.link = self.base / "junction"
        # PowerShell의 네이티브 파일 작업으로 생성. 기존 경로나 드라이브는 재사용하지 않는다.
        subprocess.run(["powershell", "-NoProfile", "-Command",
                        "New-Item -ItemType Junction -Path $env:PHOTOPATH1_LINK "
                        "-Target $env:PHOTOPATH1_REAL | Out-Null"], check=True, capture_output=True,
                       env={**os.environ, "PHOTOPATH1_LINK": str(self.link), "PHOTOPATH1_REAL": str(self.real)})
        self.addCleanup(self.link.rmdir)
        self.db_path = self.base / "synthetic.db"
        self.db = PhotoAlbumDb(self.db_path)

    def peer(self, op, path, tag=None):
        args = [os.environ["PHOTOPATH1_RUST_PEER"], str(self.db_path), op, str(path)]
        if tag is not None:
            args.append(tag)
        result = subprocess.run(args, check=True, capture_output=True, encoding="utf-8")
        return json.loads(result.stdout)

    def paths(self):
        yield "real", str(self.photo)
        yield "junction", str(self.link / self.photo.name)
        yield "case", str(self.photo).swapcase()
        yield "verbatim", "\\\\?\\" + str(self.photo.resolve())
        buf = ctypes.create_unicode_buffer(32768)
        get_short = ctypes.windll.kernel32.GetShortPathNameW
        get_short.argtypes = (ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint32)
        get_short.restype = ctypes.c_uint32
        count = get_short(str(self.photo), buf, len(buf))
        self.assertGreater(count, 0)
        self.assertLess(count, len(buf))
        if "~" in buf.value:
            yield "short", buf.value
        else:
            print("PHOTOPATH1: 8.3 alias unavailable on fixture volume", flush=True)
        drive = next((f"{c}:" for c in "ZYXWVUTSRQPONMLKJIHGF" if not os.path.exists(f"{c}:\\")), None)
        self.assertIsNotNone(drive, "합성 SUBST 시험용 빈 드라이브 필요")
        subprocess.run(["subst", drive, str(self.real.resolve())], check=True, capture_output=True)
        try:
            yield "subst", drive + "\\" + self.photo.name
        finally:
            subprocess.run(["subst", drive, "/D"], check=True, capture_output=True)

    def rows(self):
        with closing(sqlite3.connect(self.db_path)) as conn:
            return conn.execute("SELECT path, favorite FROM photos ORDER BY path").fetchall()

    def seed(self, path):
        self.db.upsert_photo(path=str(path), folder=str(Path(path).parent), filename=self.photo.name,
                             size=20, mtime=123, date_taken="2026-01-02", orientation=6)
        self.db.add_tag(str(path), "python")
        self.assertTrue(self.db.toggle_favorite(str(path)))

    def test_python_first_all_aliases_use_same_row(self):
        self.seed(self.photo)
        original = self.rows()[0][0]
        for kind, path in self.paths():
            with self.subTest(kind=kind):
                self.assertEqual(self.peer("read", path), {"favorite": True, "tags": ["python"]})
                self.peer("add", path, "rust")
                self.assertEqual(self.db.get_tags(path), ["python", "rust"])
                self.assertFalse(self.peer("favorite", path))
                self.assertFalse(self.db.get_photo(path)["favorite"])
                self.assertTrue(self.db.toggle_favorite(path))
                self.peer("remove", path, "rust")
                self.assertEqual(self.rows(), [(original, 1)])

    def test_rust_first_python_reads_and_writes_same_row(self):
        for kind, path in self.paths():
            with self.subTest(kind=kind):
                self.db_path = self.base / f"rust-first-{kind}.db"
                self.db = PhotoAlbumDb(self.db_path)
                self.peer("add", path, "rust")
                self.db.add_tag(str(self.photo), "python")
                self.assertEqual(self.db.get_tags(str(self.photo)), ["python", "rust"])
                self.assertEqual(len(self.rows()), 1)
                self.assertEqual(self.peer("read", path)["tags"], ["python", "rust"])

    def test_legacy_rows_under_each_alias_are_reused_without_relocation(self):
        for kind, path in self.paths():
            with self.subTest(kind=kind):
                self.db_path = self.base / f"legacy-{kind}.db"
                self.db = PhotoAlbumDb(self.db_path)
                self.db.upsert_many([(path, str(Path(path).parent), self.photo.name,
                                      20, 123, "2026-01-02", 6, 0)])
                with closing(sqlite3.connect(self.db_path)) as conn, conn:
                    conn.execute("INSERT INTO tags VALUES (?, 'legacy')", (path,))
                self.peer("add", self.photo, "rust")
                self.db.add_tag(str(self.photo), "python")
                self.assertEqual(self.db.get_tags(str(self.photo)), ["legacy", "python", "rust"])
                self.assertEqual(self.peer("read", self.photo)["tags"], ["legacy", "python", "rust"])
                self.assertEqual([r[0] for r in self.rows()], [path])

    def test_legacy_alias_row_keeps_path_metadata_and_marks(self):
        alias = str(self.link / self.photo.name)
        self.db.upsert_many([(alias, str(self.link), self.photo.name, 20, 123, "2026-01-02", 6, 0)])
        with closing(sqlite3.connect(self.db_path)) as conn, conn:
            conn.execute("UPDATE photos SET favorite=1, missing=1")
            conn.execute("INSERT INTO tags VALUES (?, 'legacy')", (alias,))
        self.assertEqual(self.peer("read", self.photo), {"favorite": True, "tags": ["legacy"]})
        self.peer("add", self.photo, "rust")
        self.assertEqual(self.db.get_tags(str(self.photo)), ["legacy", "rust"])
        self.db.add_tag(str(self.photo), "python")
        self.assertEqual(self.rows(), [(alias, 1)])
        with closing(sqlite3.connect(self.db_path)) as conn:
            self.assertEqual(conn.execute("SELECT size,mtime,date_taken,orientation,missing FROM photos").fetchall(),
                             [(20, 123, "2026-01-02", 6, 1)])

    def test_store_scan_reuses_legacy_alias_row(self):
        alias = str(self.link / self.photo.name)
        stat = self.photo.stat()
        self.db.upsert_many([(alias, str(self.link), self.photo.name, stat.st_size, stat.st_mtime, "2026-01-02", 6, 0)])
        store = PhotoStore(self.db)
        store.load()
        store.write(iter([file_record(str(self.photo))]))
        self.assertEqual([r[0] for r in self.rows()], [alias])
        self.assertEqual((self.db.get_photo(str(self.photo))["date_taken"],
                          self.db.get_photo(str(self.photo))["orientation"]), ("2026-01-02", 6))

    def test_reading_legacy_alias_is_read_only_and_missing_file_is_found(self):
        alias = str(self.link / self.photo.name)
        self.db.upsert_many([(alias, str(self.link), self.photo.name, 20, 123, "2026-01-02", 6, 0)])
        with closing(sqlite3.connect(self.db_path)) as conn, conn:
            conn.execute("INSERT INTO tags VALUES (?, 'legacy')", (alias,))
        self.photo.unlink()
        before = self.db_path.read_bytes()
        self.assertEqual(self.db.get_photo(str(self.photo))["path"], alias)
        self.assertEqual(self.db.get_tags(str(self.photo)), ["legacy"])
        self.assertEqual(self.peer("read", self.photo), {"favorite": False, "tags": ["legacy"]})
        self.assertEqual(self.db_path.read_bytes(), before)

    def test_bulk_scan_and_single_upsert_do_not_move_legacy_path(self):
        alias = str(self.link / self.photo.name)
        self.db.upsert_many([(alias, str(self.link), self.photo.name, 20, 123, "2026-01-02", 6, 0)])
        self.db.upsert_many([(str(self.photo), str(self.real), self.photo.name, 21, 124, "2026-01-03", 3, 0)])
        self.db.upsert_photo(path=str(self.photo), folder=str(self.real), filename=self.photo.name,
                             size=22, mtime=125, date_taken="2026-01-04", orientation=8)
        self.assertEqual([r[0] for r in self.rows()], [alias])

    def test_legacy_alias_set_remove_and_toggle_reuse_row(self):
        alias = str(self.link / self.photo.name)
        self.db.upsert_many([(alias, str(self.link), self.photo.name, 20, 123, "2026-01-02", 6, 0)])
        self.db.set_tags(str(self.photo), ["keep", "remove"])
        self.assertEqual(self.peer("read", self.photo)["tags"], ["keep", "remove"])
        self.db.remove_tag(str(self.photo), "remove")
        self.assertTrue(self.db.toggle_favorite(str(self.photo)))
        self.assertFalse(self.peer("favorite", self.photo))
        self.assertEqual(self.rows(), [(alias, 0)])
        self.assertEqual(self.db.get_photo(str(self.photo))["tags"], ["keep"])

    def test_scan_reuses_the_cached_row_and_its_marks(self):
        alias = str(self.link / self.photo.name)
        stat = self.photo.stat()
        self.db.upsert_many([(alias, str(self.link), self.photo.name, stat.st_size, stat.st_mtime,
                              "2026-01-02", 6, 0)])
        self.db.add_tag(alias, "legacy")
        self.db.toggle_favorite(alias)
        sync = PhotoViewSync(PhotoStore(self.db), lambda row: None, roots=[self.base])
        sync.publish_snapshot()
        sync.feed({"path": str(self.photo), "name": self.photo.name,
                   "size": stat.st_size, "mtime": stat.st_mtime, "cloud": False})
        sync.finish(True)
        self.assertEqual(len(sync.items()), 1)
        self.assertEqual((sync.items()[0]["favorite"], sync.items()[0]["tags"]), (True, ["legacy"]))
        self.assertEqual([r[0] for r in self.rows()], [alias])


if __name__ == "__main__":
    unittest.main()
