import unittest

from scripts.search_quality_eval import metrics


class SearchQualityMetricsTests(unittest.TestCase):
    def test_recall_at_10_and_mrr_use_known_relevant_documents(self):
        result = metrics(["body.txt", "gold-b.txt", "gold-a.txt"], {"gold-a.txt", "gold-b.txt"})
        self.assertEqual(result["recall_at_10"], 1.0)
        self.assertEqual(result["mrr"], 0.5)
        self.assertEqual(result["first_relevant_rank"], 2)

    def test_empty_retrieval_has_zero_metrics(self):
        result = metrics([], {"gold.txt"})
        self.assertEqual(result["recall_at_10"], 0.0)
        self.assertEqual(result["mrr"], 0.0)


if __name__ == "__main__":
    unittest.main()
