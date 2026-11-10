import unittest
import copy
import sqlite3
import tempfile
from pathlib import Path
from unittest.mock import patch

from docfit_core.hierarchy_search import build_index, search, build_index_from_content_db


class HierarchySearchTest(unittest.TestCase):
    def setUp(self):
        self.index = build_index({"nodes": [
            {"id": 1, "role": "chapter", "level": 2, "text": "시설물 정비", "parent_id": None},
            {"id": 2, "role": "section", "level": 3, "text": "관광안내판 철거", "parent_id": 1},
            {"id": 3, "role": "detail", "level": 4, "text": "전기시설 분리발주", "parent_id": 2},
            {"id": 4, "role": "square", "level": 5, "text": "전기설비 해체공사는 별도 발주를 원칙으로 한다", "parent_id": 3},
        ]}, document_name="2026년 관광시설 정비계획.hwpx")

    def test_breadcrumb_restores_parent_context(self):
        result = search(self.index, "전기공사 분리발주")
        self.assertTrue(result)
        self.assertEqual(result[0]["breadcrumb"], "시설물 정비 > 관광안내판 철거 > 전기시설 분리발주")

    def test_empty_query_and_limit(self):
        self.assertEqual(search(self.index, "", limit=5), [])
        self.assertEqual(search(self.index, "분리발주", limit=0), [])

    def test_additive_context_fields_and_no_io(self):
        original = copy.deepcopy(self.index)
        with patch("builtins.open", side_effect=AssertionError("원본 열기 금지")), \
                patch("socket.socket", side_effect=AssertionError("외부 연결 금지")):
            result = search(self.index, "분리발주")[0]
        self.assertEqual(result["full_breadcrumb"], self.index["document_name"] + " > " + result["breadcrumb"])
        self.assertEqual(result["matched_terms"], ["분리발주"])
        self.assertEqual(result["matched_body_terms"], ["분리발주"])
        self.assertEqual(result["parent_nodes"][-1]["id"], 2)
        self.assertEqual(result["child_nodes"][0]["id"], 4)
        self.assertIsNone(result["page"])
        self.assertEqual(len(result["pipeline"]), 3)
        result["position"]["page"] = 999
        self.assertEqual(self.index, original)

    def test_external_candidates_keep_original_scores(self):
        all_results = search(self.index, "분리발주")
        one = search(self.index, "분리발주", candidate_ids=[4])
        self.assertEqual([r["id"] for r in one], [4])
        self.assertEqual(one[0]["score"], next(r["score"] for r in all_results if r["id"] == 4))
        self.assertEqual(one[0]["matched_context_terms"], ["분리발주"])
        self.assertEqual(search(self.index, "분리발주", candidate_ids=[]), [])

    def test_table_cell_position_is_returned(self):
        index = build_index({"paragraphs": [{"text": "철거비", "level": "table_cell",
                                            "table_order": 2, "sheet": "예산", "cell": "B2"}]})
        result = search(index, "철거비")[0]
        self.assertEqual(result["position"]["cell"], "B2")
        self.assertEqual(result["position"]["sheet"], "예산")
        self.assertIsNotNone(result["table_id"])

    def test_version_one_index_and_mixed_ids(self):
        legacy = {"document_name": "옛 인덱스", "records": [
            {"id": 1, "parent_id": None, "role": "body", "level": 2,
             "text": "검색", "tokens": ["검색"], "path": ["검색"]}]}
        self.assertEqual(search(legacy, "검색")[0]["id"], 1)
        index = build_index({"paragraphs": [{"id": 1, "text": "검색", "level": "body"},
                                            {"id": "a", "text": "검색", "level": "body"}]})
        self.assertEqual(len(search(index, "검색")), 2)

    def test_shared_rust_sqlite_schema_read_only_adapter(self):
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "색인.sqlite"
            with sqlite3.connect(database) as db:
                db.execute("CREATE TABLE content_meta(path TEXT, name TEXT)")
                db.execute("CREATE TABLE content_paragraph_fts(path TEXT, paragraph_index, level, body, location_label, section)")
                db.execute("INSERT INTO content_meta VALUES (?, ?)", ("missing.hwpx", "시설.hwpx"))
                db.executemany("INSERT INTO content_paragraph_fts VALUES (?,?,?,?,?,?)", [
                    ("missing.hwpx", 0, "chapter", "시설 정비", "1쪽", "0"),
                    ("missing.hwpx", 1, "body", "안전 점검", "2쪽", "0")])
            db.close()
            before = database.read_bytes()
            index = build_index_from_content_db(database, "missing.hwpx")
            result = search(index, "점검")[0]
            self.assertEqual(result["page"], 2)
            self.assertIn("시설 정비", result["full_breadcrumb"])
            self.assertEqual(result["level"], "body")
            self.assertEqual(database.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
