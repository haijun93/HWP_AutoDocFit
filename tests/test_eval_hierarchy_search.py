from __future__ import annotations

import unittest

from scripts.eval_hierarchy_search import canonical_path, score_ranked_results


class HierarchySearchScoringTests(unittest.TestCase):
    def test_recall_and_mrr_use_ranked_document_paths(self):
        q = "예산"
        relevant = ["a.txt", "c.txt", "z.txt"]
        rows = {q: [{"path": "x.txt"}, {"path": "a.txt"}, {"path": "c.txt"}]}
        score = score_ranked_results(rows, {q: relevant})[q]
        self.assertAlmostEqual(score["recall_at_5"], 2 / 3)
        self.assertAlmostEqual(score["recall_at_10"], 2 / 3)
        self.assertAlmostEqual(score["mrr"], 0.5)

    def test_context_accuracy_requires_exact_breadcrumb_match(self):
        path_a, path_b = canonical_path("a.md"), canonical_path("b.md")
        query = "안전"
        rows = {query: [
            {"path": path_a, "breadcrumb": ["제1장", "제1절"]},
            {"path": path_b, "breadcrumb": ["제2장"]},
        ]}
        expected = {query: {path_a: ["제1장", "제1절"], path_b: ["제2장", "제1절"]}}
        score = score_ranked_results(rows, {query: [path_a, path_b]}, expected)[query]
        self.assertEqual(score["context_accuracy"], 0.5)
        self.assertEqual(score["context_evaluated"], 2)

    def test_empty_relevance_and_missing_context_have_defined_values(self):
        score = score_ranked_results({"q": []}, {"q": []})
        self.assertEqual(score["q"]["recall_at_5"], 0.0)
        self.assertEqual(score["q"]["mrr"], 0.0)
        self.assertIsNone(score["q"]["context_accuracy"])
        self.assertIsNone(score["mean_context_accuracy"])


if __name__ == "__main__":
    unittest.main()
