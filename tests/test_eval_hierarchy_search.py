from __future__ import annotations

import unittest

from scripts.eval_hierarchy_search import canonical_path, score_ranked_results


class HierarchySearchScoringTests(unittest.TestCase):
    def test_regression_expectations_include_title_and_shorten_long_breadcrumbs(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory

        from scripts.eval_hierarchy_search import generate_corpus
        from docfit_core.timemachine.hierarchy_path import shorten

        with TemporaryDirectory() as temp:
            documents, _, _, groups = generate_corpus(Path(temp), document_count=100)
            numbered = documents[groups["번호 체계 오류"][0]]
            title = Path(numbered["path"]).read_text(encoding="utf-8").splitlines()[0][2:]
            self.assertEqual(numbered["breadcrumb"], [title, "제3장 관리", "7.4.9 계획", "나) 세부 기준"])

            long_title = documents[groups["아주 긴 제목"][0]]
            self.assertEqual(long_title["breadcrumb"], [
                shorten(part) for part in long_title["breadcrumb"]
            ])
            self.assertTrue(all(len(part) <= 60 for part in long_title["breadcrumb"]))

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

    def test_corpus_uses_markdown_hierarchy_and_unique_body_words(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory

        from scripts.eval_hierarchy_search import generate_corpus

        with TemporaryDirectory() as temp:
            documents, _, _, _ = generate_corpus(Path(temp), document_count=100)
            self.assertEqual(len(documents), 100)
            for item in documents.values():
                path = Path(item["path"])
                self.assertEqual(path.suffix, ".md")
                lines = path.read_text(encoding="utf-8").splitlines()
                self.assertTrue(lines[0].startswith("# "))
                self.assertTrue(any(line.startswith("## ") for line in lines))
                self.assertTrue(any(line.startswith("### ") for line in lines))
                self.assertIn(item["unique_word"], lines[-1])
                self.assertNotIn(item["unique_word"], "\n".join(lines[:-1]))

    def test_empty_relevance_and_missing_context_have_defined_values(self):
        score = score_ranked_results({"q": []}, {"q": []})
        self.assertEqual(score["q"]["recall_at_5"], 0.0)
        self.assertEqual(score["q"]["mrr"], 0.0)
        self.assertIsNone(score["q"]["context_accuracy"])
        self.assertIsNone(score["mean_context_accuracy"])


if __name__ == "__main__":
    unittest.main()
