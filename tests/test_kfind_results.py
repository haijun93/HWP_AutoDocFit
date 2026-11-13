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

    def test_hierarchy_uses_sentence_level_and_sqlite_ancestors(self):
        with self.index._lock:
            self.index.db.execute("UPDATE content_paragraph_fts SET level='document_title', level_rank=0 WHERE CAST(paragraph_index AS INTEGER)=0")
        row = results.to_hierarchy_rows(self.response, self.index)[0]
        self.assertEqual(row["context_hits"], 0)
        self.assertEqual(row["breadcrumb_text"], "예산 계획")
        self.assertEqual(row["breadcrumb"], ["예산 계획"])

    def test_library_pages_scopes_and_escaped_highlight(self):
        doc = dict(name="메모", title="제목", scopes=["tm", "box:2", "box:10"], paragraphs=[
            dict(paragraph_index=1, body="<예산ㆍ집행>", location_label="7쪽", level_rank=8)])
        response = copy.deepcopy(self.response)
        response["hits"][0]["sentences"][0].update(start=1, len=5)
        row = results.to_library_rows(response, documents={self.path: doc})[0]
        self.assertEqual((row["page"], row["hit_pages"], row["box_ids"]), (7, [7], ["2", "10"]))
        self.assertEqual(row["highlighted"], ["&lt;<b>예산</b>ㆍ<b>집행</b>&gt;"])

    def test_notebook_source_ordinal_original_and_location(self):
        doc = dict(name="메모", scopes=["nb:2:src:17"], paragraphs=[
            dict(paragraph_index=1, body="예산ㆍ집행", location_label="시트 4행", level_rank=8)])
        response = copy.deepcopy(self.response)
        response["hits"][0]["sentences"][0].update(start=0, len=5)
        response["hits"][0]["sentences"] *= 2
        rows = results.to_notebook_rows(response, documents={self.path: doc})
        self.assertEqual(len(rows), 1)
        self.assertEqual((rows[0]["source_id"], rows[0]["ordinal"], rows[0]["body"], rows[0]["location"]),
                         (17, 1, "예산ㆍ집행", "시트 4행"))

    def test_fold_highlights_unicode_and_multiple_sentences(self):
        body = "😀 Straße Cafe\u0301 예산. Straße"
        words = [dict(text=t) for t in ("STRASSE", "Café", "예산")]
        row = results._location(dict(body=body, paragraph_index=0),
            dict(start=0, len=body.index(".") + 1), words, [])
        self.assertEqual([row["snippet"][a:b] for a, b in row["highlights"]],
                         ["Straße", "Cafe\u0301", "예산"])
