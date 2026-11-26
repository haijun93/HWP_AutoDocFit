"""Rust 사진뷰 창의 즐겨찾기·태그 DB 표가 앱 화면(PhotoAlbumDb)과 같은지 대조한다."""
import re
import sqlite3
import tempfile
import unittest
from contextlib import closing
from pathlib import Path

from docfit_app.photo_album_service import PhotoAlbumDb

ROOT = Path(__file__).resolve().parent.parent


def _shape(conn):
    names = [r[0] for r in conn.execute(
        "SELECT name FROM sqlite_master WHERE type IN ('table','index') AND name NOT LIKE 'sqlite_%'")]
    shape = {}
    for name in names:
        shape[name] = (
            conn.execute(f"PRAGMA table_info({name})").fetchall(),
            conn.execute(f"PRAGMA index_info({name})").fetchall(),
            conn.execute(f"PRAGMA foreign_key_list({name})").fetchall(),
        )
    return shape


class PhotoViewLibrarySchemaTest(unittest.TestCase):
    def test_rust_schema_matches_python_schema(self):
        source = (ROOT / "photoview_win" / "src" / "library.rs").read_text(encoding="utf-8")
        match = re.search(r'const SCHEMA: &str = "(.*?)";\s*\n\s*\n', source, re.S)
        self.assertIsNotNone(match, "library.rs 의 SCHEMA 를 찾지 못함")
        with tempfile.TemporaryDirectory() as folder:
            python_db = Path(folder) / "python.db"
            PhotoAlbumDb(python_db)
            with closing(sqlite3.connect(python_db)) as conn:
                expected = _shape(conn)
        with closing(sqlite3.connect(":memory:")) as conn:
            conn.executescript(match.group(1))
            self.assertEqual(_shape(conn), expected)


if __name__ == "__main__":
    unittest.main()
