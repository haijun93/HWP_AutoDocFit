import copy
import os
import tempfile
import unittest

from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine import kfind_results as results


class ResultTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = os.path.join(self.tmp.name, "sample.txt")
        with open(self.path, "w", encoding="utf-8") as stream:
            stream.write("예산 계획\n" + "앞" * 90 + "😀 예산ㆍ집행. 다음 예산 문장.")
        self.index = ContentSearchIndex(os.path.join(self.tmp.name, "content.db"))
        self.addCleanup(self.index.close)
        self.index.index_file(self.path)
        self.response = dict(generation=7, words=[dict(text="예산집행", pieces=["예산", "집행"])], dropped=["및"],
            hits=[dict(path=self.path, doc_key=self.path, count=3, level=8, adjacent=False,
                sentences=[dict(para=1, start=92, len=6, ord=1)])])

    def test_keyword_fields_and_split_highlights(self):
        row = results.to_keyword_rows(self.response, self.index, offset=5)[0]
        self.assertEqual(row["relevance_rank"], 6)
        self.assertEqual(row["snippet"][:70], "앞" * 68 + "😀 ")
        self.assertEqual([row["snippet"][a:b] for a, b in row["highlights"]], ["예산", "집행"])
        self.assertEqual(row["total_matches"], 3)
        self.assertEqual(row["match_counts"], {"예산집행": 3})
        self.assertEqual((row["engine"], row["gen"], row["best_level"]), ("kfind", 7, "본문"))
        self.assertEqual(row["locations"][0]["position"]["label"], "2행")
        self.assertEqual(row["dropped"], ["및"])

    def test_missing_or_invalid_original_rejected(self):
        response = copy.deepcopy(self.response)
        response["hits"][0]["sentences"][0]["len"] = 9999
        with self.assertRaises(ValueError):
            results.to_keyword_rows(response, self.index)
        with self.assertRaises(ValueError):
            results.to_keyword_rows(self.response)

    def test_empty(self):
        self.assertEqual(results.to_keyword_rows(dict(hits=[])), [])
