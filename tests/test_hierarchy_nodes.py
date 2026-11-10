import copy
import unittest

from docfit_core.hierarchy_nodes import normalize_document
from docfit_core.hierarchy_search import build_index


class HierarchyNodesTests(unittest.TestCase):
    def test_rust_sqlite_rows_keep_authoritative_level_and_location(self):
        source = {"paragraphs": [
            {"body": "Ⅰ 추진 계획", "level": "chapter", "paragraph_index": 0},
            {"body": "□ 시설 정비", "level": "body", "paragraph_index": 1,
             "location_label": "3쪽", "section": "0", "table_order": 4,
             "style_name": "Heading", "indent": 2},
            {"body": "안전 점검한다. 예산 확인한다!", "level": "body", "paragraph_index": 2}]}
        original = copy.deepcopy(source)
        result = normalize_document(source, document_name="시험.hwpx")
        self.assertEqual(source, original)
        by_id = {n["id"]: n for n in result["nodes"]}
        p = by_id[result["source_ids"][1]]
        self.assertEqual(p["role"], "body")
        self.assertEqual(p["position"]["page"], 3)
        self.assertEqual(by_id[p["table_id"]]["kind"], "table")
        self.assertEqual(by_id[p["parent_id"]]["role"], "chapter")
        self.assertTrue(result["diagnostics"])
        sentences = [n for n in result["nodes"] if n["kind"] == "sentence" and n["parent_id"] == result["source_ids"][2]]
        self.assertEqual(len(sentences), 2)
        for sentence in sentences:
            self.assertEqual(sentence["text"], source["paragraphs"][2]["body"][sentence["start"]:sentence["end"]])

    def test_symbol_alone_never_promotes_and_context_can_promote(self):
        for value in ("□ 제목", "ㅇ 제목", "- 제목", "Ⅰ 제목", "1. 제목", "가. 제목"):
            r = normalize_document({"paragraphs": [{"text": value}]})
            p = next(n for n in r["nodes"] if n["id"] == r["source_ids"][0])
            self.assertEqual(p["role"], "body")
            r = normalize_document({"paragraphs": [{"text": value, "style_name": "Heading 1"}]})
            p = next(n for n in r["nodes"] if n["id"] == r["source_ids"][0])
            self.assertEqual(p["hierarchy_source"], "context")
            self.assertNotEqual(p["role"], "body")

    def test_legacy_parents_and_ids_are_preserved(self):
        index = build_index({"nodes": [{"id": 0, "text": "장", "role": "chapter", "level": 2, "parent_id": None},
                                       {"id": 1, "text": "내용", "role": "square", "level": 5, "parent_id": 0}]})
        self.assertEqual(index["records"][0]["parent_id"], None)
        self.assertEqual(index["records"][1]["parent_id"], 0)
        self.assertEqual(index["records"][1]["breadcrumb"], "장 > 내용")

    def test_invalid_tree_fails_without_hang(self):
        for nodes in ([{"id": 1, "parent_id": 1}], [{"id": 1, "parent_id": 99}], [{"id": 1}, {"id": 1}]):
            with self.assertRaises(ValueError):
                normalize_document({"nodes": nodes})

    def test_unknown_table_location_is_reported(self):
        r = normalize_document({"paragraphs": [{"text": "표 내용", "is_table_cell": True}]})
        self.assertEqual(r["diagnostics"][0]["code"], "table_identity_unavailable")

    def test_left_margin_context_and_distinct_sheets(self):
        r = normalize_document({"paragraphs": [
            {"text": "□ 계획", "paragraph_style": {"left_margin": 0}},
            {"text": "추진내용", "paragraph_style": {"left_margin": 100}}]})
        node = next(n for n in r["nodes"] if n["id"] == r["source_ids"][0])
        self.assertEqual(node["role"], "square")
        r = normalize_document({"paragraphs": [
            {"text": "표1", "table_order": 1, "sheet": "가"},
            {"text": "표2", "table_order": 1, "sheet": "나"}]})
        self.assertEqual(len([n for n in r["nodes"] if n["kind"] == "table"]), 2)


if __name__ == "__main__":
    unittest.main()
