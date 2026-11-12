import os
import sqlite3
import tempfile
import unittest
from pathlib import Path

from docfit_core.timemachine.content_search import ContentSearchIndex


class KfindContentSearchTests(unittest.TestCase):
    def test_records_are_ordered_and_preserve_cells_and_empty_text(self):
        index = ContentSearchIndex(":memory:")
        self.addCleanup(index.close)
        path = os.path.abspath("문서.txt")
        for number, level, rank, body in ((9, "table_cell", 8, "표 칸"),
                                           (4, "body", 8, ""), (0, "document_title", 0, "  제목\n")):
            index.db.execute(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body)"
                " VALUES(?,?,?,?,?,?)", (path, "문서", number, level, rank, body))
        index.db.execute("INSERT INTO content_table_cells VALUES(?,?,?)", (path, 9, 42))
        index.db.commit()
        records = index.paragraphs_for_kfind(Path(path))
        self.assertEqual([r.paragraph_index for r in records], [0, 4, 9])
        self.assertEqual([r.text for r in records], ["  제목\n", "", "표 칸"])
        self.assertEqual([r.level_rank for r in records], [0, 8, 8])
        self.assertTrue(records[-1].is_table_cell)
        self.assertEqual(records[-1].table_cell_id, 42)
        self.assertEqual(index.paragraphs_for_kfind("missing.txt"), [])

    def test_migration_keeps_rows_and_sync_state_on_reopen(self):
        with tempfile.TemporaryDirectory() as folder:
            db_path = Path(folder) / "index.sqlite"
            with sqlite3.connect(db_path) as db:
                db.execute("CREATE TABLE content_meta(path TEXT PRIMARY KEY,size INTEGER,mtime REAL)")
                db.execute("INSERT INTO content_meta VALUES('document.txt',12,34)")
            db.close()
            index = ContentSearchIndex(db_path)
            row = index.db.execute("SELECT size,mtime,kfind_level_rule,kfind_gen FROM content_meta").fetchone()
            self.assertEqual(tuple(row), (12, 34, 0, 0))
            index.db.execute("UPDATE content_meta SET kfind_level_rule=5,kfind_gen=123")
            index.db.commit()
            index.close()
            index = ContentSearchIndex(db_path)
            try:
                row = index.db.execute("SELECT kfind_level_rule,kfind_gen FROM content_meta").fetchone()
                self.assertEqual(tuple(row), (5, 123))
            finally:
                index.close()
