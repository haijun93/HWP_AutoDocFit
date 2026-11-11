"""제목 표 승격과 규칙 버전 갱신 회귀 시험."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from docfit_core.timemachine.content_search import (
    ParagraphRecord, classify_paragraph_levels, ContentSearchIndex,
    CURRENT_HIERARCHY_SCHEMA_VERSION,
)
from tests.test_timemachine_content_search import write_hwpx_paragraphs


class TitlePromotionTests(unittest.TestCase):
    def classify(self, prefix=0, explicit=False):
        rows = [ParagraphRecord(i, "삼체 문명", "body", 8, False) for i in range(prefix)]
        rows += [ParagraphRecord(prefix, "삼체 문명", "body", 8, True, table_cell_id=1),
                 ParagraphRecord(prefix+1, "삼체 문명", "body", 8, True, table_cell_id=1),
                 ParagraphRecord(prefix+2, "담당자", "body", 8, True, table_cell_id=2)]
        if explicit:
            rows.append(ParagraphRecord(prefix+3, "삼체 문명", "body", 8, False, style_name="제목"))
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
            return classify_paragraph_levels("sample.hwpx", rows)

    def test_cell_fragments_and_leading_body_boundary(self):
        for prefix in (0, 1, 2):
            records = self.classify(prefix)
            self.assertEqual(["document_title", "document_title", "table_cell"],
                             [p.level for p in records[prefix:]])
        self.assertEqual("table_cell", self.classify(3)[3].level)
        self.assertEqual("table_cell", self.classify(explicit=True)[0].level)

    def test_only_first_table_is_considered(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "tables.hwpx"
            write_hwpx_paragraphs(path, [{"text": "1234", "is_table_cell": True},
                                        {"text": "삼체 문명", "is_table_cell": True}])
            from docfit_core.timemachine.content_search import extract_paragraphs
            self.assertEqual(["table_cell", "table_cell"], [p.level for p in extract_paragraphs(path)])

    def test_engine_path_and_breadcrumb_preserve_whole_title(self):
        from docfit_core.timemachine import hierarchy_path
        records = [ParagraphRecord(0, "삼체 문명", "body", 8, True, table_cell_id=1),
                   ParagraphRecord(1, "이주 계획", "body", 8, True, table_cell_id=1),
                   ParagraphRecord(2, "행성 조사", "body", 8, False)]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value={"levels": [8, 8, 8]}):
            records = classify_paragraph_levels("sample.hwpx", records)
        self.assertEqual(["document_title", "document_title", "body"], [p.level for p in records])
        rows = [dict(paragraph_index=p.paragraph_index, body=p.text, level=p.level,
                     level_rank=p.level_rank, table_cell_id=p.table_cell_id) for p in records]
        self.assertIsNone(hierarchy_path.document_title_text(rows))
        self.assertEqual("삼체 문명 이주 계획", hierarchy_path.breadcrumb_text(hierarchy_path.breadcrumb(rows, 2)))


class RuleReindexTests(unittest.TestCase):
    def test_old_rule_search_survives_reopen_and_reindex(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root / "plan.hwpx"
            write_hwpx_paragraphs(path, [{"text": "삼체 문명 계획", "is_table_cell": True},
                                        {"text": "삼체 행성 조사"}])
            db = root / "content.sqlite"
            index = ContentSearchIndex(db)
            self.assertTrue(index.index_file(path)["indexed"])
            index.db.execute("UPDATE content_paragraph_fts SET level='table_cell',level_rank=8 WHERE paragraph_index=0")
            index.db.execute("UPDATE content_meta SET hierarchy_schema_version=?", (CURRENT_HIERARCHY_SCHEMA_VERSION-1,))
            index.db.commit()
            index.close()
            index = ContentSearchIndex(db)
            try:
                self.assertEqual(1, index.needs_reindex_count())
                self.assertEqual("표 칸", index.search("삼체")[0]["best_level"])
                self.assertEqual(1, len(index.keyword_search("삼체")))
                self.assertTrue(index.index_file(path)["indexed"])
                self.assertEqual(0, index.needs_reindex_count())
                self.assertEqual("문서 제목", index.search("삼체")[0]["best_level"])
                self.assertFalse(index.index_file(path)["indexed"])
            finally:
                index.close()

    def test_whole_body_legacy_index_remains_searchable(self):
        import sqlite3
        with tempfile.TemporaryDirectory() as tmp:
            db = Path(tmp) / "content.sqlite"
            conn = sqlite3.connect(db)
            conn.executescript("CREATE VIRTUAL TABLE content_fts USING fts5(path UNINDEXED,name UNINDEXED,body);"
                               "CREATE TABLE content_meta(path TEXT PRIMARY KEY,size INTEGER,mtime REAL);")
            path = str(Path(tmp) / "old.txt")
            conn.execute("INSERT INTO content_fts VALUES(?,?,?)", (path,"old.txt","삼체 검색 본문"))
            conn.execute("INSERT INTO content_meta VALUES(?,?,?)", (path,1,1))
            conn.commit()
            conn.close()
            for _ in range(2):
                index = ContentSearchIndex(db)
                try:
                    self.assertEqual(1, index.needs_reindex_count())
                    self.assertEqual(1, len(index.search("검색")))
                    self.assertEqual(1, len(index.keyword_search("삼체")))
                    self.assertEqual(1, index.db.execute("SELECT COUNT(*) FROM content_paragraph_fts").fetchone()[0])
                finally:
                    index.close()

    def test_service_open_reindexes_in_existing_background_worker(self):
        import json
        import threading
        from docfit_app.timemachine_service import TimemachineService
        from tests.test_timemachine_service import FakeAgent
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            docs = root / "docs"
            docs.mkdir()
            path = docs / "plan.hwpx"
            write_hwpx_paragraphs(path, [{"text": "삼체 계획", "is_table_cell": True}])
            db = root / "timeline.content.sqlite"
            index = ContentSearchIndex(db)
            self.assertTrue(index.index_file(path)["indexed"])
            index.db.execute("UPDATE content_meta SET hierarchy_schema_version=1")
            index.db.commit()
            index.close()
            settings = root / "settings"
            settings.mkdir()
            (settings / "timemachine.json").write_text(json.dumps({"roots":[str(docs)],"extensions":["hwpx"],"enabled":False,"time_mode":"max"}),encoding="utf-8")
            entered, release = threading.Event(), threading.Event()
            original = ContentSearchIndex.index_file
            def paused_index(index, file, **kwargs):
                entered.set()
                release.wait(5)
                return original(index, file, **kwargs)
            with patch.object(ContentSearchIndex, "index_file", paused_index):
                service = TimemachineService(settings,db_path=root/"timeline.db",agent=FakeAgent(),thumb_dir=root/"thumbs")
                try:
                    self.assertTrue(entered.wait(3))
                    self.assertTrue(service._content_state["running"])
                    self.assertEqual(1,service.content_index.needs_reindex_count())
                    self.assertEqual(1,len(service.content_index.search("삼체")))
                    release.set()
                    service._content_thread.join(5)
                    self.assertFalse(service._content_thread.is_alive())
                    self.assertEqual(0,service.content_index.needs_reindex_count())
                    self.assertEqual("문서 제목",service.content_index.search("삼체")[0]["best_level"])
                    self.assertEqual(1,service._content_state["indexed"])
                finally:
                    release.set()
                    service.close()
