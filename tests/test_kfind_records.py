import os
import unittest
from pathlib import Path
from types import SimpleNamespace

from docfit_core.timemachine.content_search import (
    CURRENT_HIERARCHY_SCHEMA_VERSION, LEVEL_ORDER, ParagraphRecord,
)
from docfit_core.timemachine.kfind_records import doc_key, doc_from_records


class KfindRecordsTests(unittest.TestCase):
    def test_file_key(self):
        path = Path("fixture") / ".." / "문서.txt"
        self.assertEqual(doc_key(path), os.path.normcase(os.path.abspath(path)))

    def test_memo_keys(self):
        for key in ("memo:box:item", "nbmemo:nb:id"):
            self.assertEqual(doc_key(memo=key), key)
        for kwargs in ({}, {"path": ""}, {"path": "a", "memo": "memo:b:i"},
                       {"memo": "memo::i"}, {"memo": "other:b:i"}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                doc_key(**kwargs)

    def test_levels_cells_empty_and_original_text(self):
        records = [ParagraphRecord(i * 2, f"  {level}\n다음 문장.  ", level, i, False)
                   for i, level in enumerate(LEVEL_ORDER)]
        records += [ParagraphRecord(20, "표 칸", "table_cell", 8, True),
                    ParagraphRecord(21, "", "body", 8, False)]
        doc = doc_from_records("문서.txt", records,
                               SimpleNamespace(st_ctime=12.345, st_mtime=10), None, ["tm"])
        self.assertEqual(doc["paras"], [dict(p=r.paragraph_index, lv=r.level_rank, t=r.text)
                                       for r in records])
        self.assertEqual(doc["date"], 12345)
        self.assertEqual(doc["level_rule"], CURRENT_HIERARCHY_SCHEMA_VERSION)
        self.assertEqual(doc["level_src"], "py")
        self.assertEqual(doc["scopes"], ["tm"])

    def test_dates_and_no_records(self):
        for created, modified, expected in ((1, 2.125, 2125), (None, None, 0), (-1, 0, 0)):
            doc = doc_from_records("a.txt", [], SimpleNamespace(st_ctime=created, st_mtime=modified),
                                   "2026-01-01", ("box:1",))
            self.assertEqual(doc["date"], expected)
            self.assertEqual(doc["paras"], [])
