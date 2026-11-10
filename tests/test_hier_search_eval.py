import copy
import json
import unittest
from defusedxml import ElementTree as ET
from docfit_core.document_structure import text as paragraph_text
from docfit_core.hierarchy_search import build_index, search

from tools.hier_search_eval import (DEFAULT_CORPUS, _combined_structure, _legacy_index,
                                    _rank, evaluate, metrics, plain_search)


class MetricsTests(unittest.TestCase):
    def test_known_ranks_and_context(self):
        results = [{"id": "wrong", "breadcrumb": "관련 없음"},
                   {"id": "a", "breadcrumb": "문서 > 장 > 정답"},
                   {"id": "b", "breadcrumb": "다른 장 > 정답"}]
        value = metrics(results, ["a", "b", "c"], ["문서 > 장"], 3)
        self.assertEqual(value["recall_at_k"], 2 / 3)
        self.assertEqual(value["mrr_at_k"], .5)
        self.assertEqual(value["context_accuracy"], .5)
        missed = metrics(results, ["c"], ["문서"], 2)
        self.assertEqual(missed, {"recall_at_k": 0, "mrr_at_k": 0, "context_accuracy": 0})

    def test_k_boundary_and_no_context_annotation(self):
        results = [{"id": "wrong"}, {"id": "right"}]
        value = metrics(results, ["right"], [], 1)
        self.assertEqual(value["recall_at_k"], 0)
        self.assertEqual(value["mrr_at_k"], 0)
        self.assertIsNone(value["context_accuracy"])
        with self.assertRaises(ValueError):
            metrics([], [], [], 5)

    def test_plain_search_uses_only_body_tokens(self):
        index = {"records": [
            {"id": "body", "text": "철거", "tokens": ["철거"], "breadcrumb": "예산 > 철거"},
            {"id": "heading", "text": "예산", "tokens": ["예산"]}]}
        self.assertEqual([r["id"] for r in plain_search(index, "예산")], ["heading"])


class RegressionEvaluationTests(unittest.TestCase):
    def setUp(self):
        self.corpus = json.loads(DEFAULT_CORPUS.read_text(encoding="utf-8"))

    def test_same_corpus_reproducible_quality_and_visible_limits(self):
        original = copy.deepcopy(self.corpus)
        report = evaluate(self.corpus, k=5, repeats=1)
        second = evaluate(self.corpus, k=5, repeats=1)
        self.assertEqual(self.corpus, original)
        self.assertEqual(report["corpus_sha256"], second["corpus_sha256"])
        self.assertEqual(report["document_count"], 5)
        self.assertEqual({q["group"] for q in report["queries"]}, {
            "no_toc_pdf", "malformed_numbering", "title_in_table", "kordoc_hp_tab",
            "incomplete_hanging_indent", "cross_document_context"})
        self.assertTrue(report["unsupported"])
        self.assertTrue(report["unverified"])
        for query in report["queries"][:5]:
            self.assertEqual(query["plain"]["recall_at_k"], 1)
            self.assertEqual(query["hierarchy"]["recall_at_k"], 1)
            self.assertEqual(query["hierarchy"]["context_accuracy"], 1)
        for name in ("plain", "hierarchy"):
            for key in ("recall_at_k", "mrr_at_k", "context_accuracy"):
                self.assertEqual(report["summary"][name][key], second["summary"][name][key])
                self.assertGreaterEqual(report["summary"][name][key], 0)
                self.assertLessEqual(report["summary"][name][key], 1)
            self.assertGreaterEqual(report["summary"][name]["index_time_ms"], 0)
            self.assertGreater(report["summary"][name]["memory"]["peak_python_bytes"], 0)
            self.assertEqual(report["summary"][name]["search_latency_ms"]["samples"], 6)

    def test_invalid_annotations_fail_instead_of_dropping_queries(self):
        self.corpus["queries"][0]["relevant"] = ["missing"]
        with self.assertRaises(ValueError):
            evaluate(self.corpus)
        with self.assertRaises(ValueError):
            evaluate(self.corpus, k=0)

    def test_legacy_comparison_matches_original_commit_golden(self):
        golden = json.loads(DEFAULT_CORPUS.with_name("legacy_2b7ec241_top5.json").read_text(encoding="utf-8"))
        index = _legacy_index(_combined_structure(self.corpus))
        for query in self.corpus["queries"]:
            actual = _rank(index, query["query"], limit=5, legacy=True)
            self.assertEqual([{"id": r["id"], "score": r["score"]} for r in actual],
                             golden["queries"][query["id"]])

    def test_real_xml_tab_reader_and_incomplete_indent_preserve_search(self):
        root = ET.fromstring(DEFAULT_CORPUS.with_name("kordoc_tab.xml").read_bytes())
        paragraphs = [{"text": paragraph_text(p), "level": level,
                       "paragraph_style": {"indent": None}}
                      for p, level in zip(root, ["square", "circle"])]
        original = copy.deepcopy(paragraphs)
        index = build_index({"paragraphs": paragraphs})
        result = search(index, "출입통제")[0]
        self.assertIn("출입통제", result["text"])
        self.assertEqual(paragraphs, original)
        self.assertEqual(result["level"], "square")


if __name__ == "__main__":
    unittest.main()
