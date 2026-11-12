"""HLAT1 위계검색 지연 개선: 바꾼 내부 단계가 예전 방식과 같은 값을 내는지 함수 단위로 고정한다."""
import tempfile
import unittest
from pathlib import Path

from docfit_core.timemachine.content_search import ContentSearchIndex


def _synthetic_index(tmp, documents=30, paragraphs=12):
    index = ContentSearchIndex(Path(tmp) / "index.sqlite")
    rows = []
    for doc in range(documents):
        path = str(Path(tmp) / f"doc{doc:03}.md")
        texts = ["# 시설 사업", f"## 관광안내판 구역{doc % 5:03}"]
        texts += [f"해체공사 공정{p % 4:03} 시행 계획 {p}" for p in range(paragraphs - 2)]
        rows.extend((path, Path(path).name, p,
                     "document_title" if p == 0 else "chapter" if p == 1 else "body",
                     0 if p == 0 else 1 if p == 1 else 8, text) for p, text in enumerate(texts))
    index.db.executemany(
        "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) VALUES(?,?,?,?,?,?)",
        rows)
    index.db.commit()
    index._ensure_paragraph_map()
    return index


class MatchingPathsTests(unittest.TestCase):
    def test_matching_paths_equals_fts_distinct_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            index = _synthetic_index(tmp)
            try:
                for term in ("해체공사", "구역003", "공정002", "관광안내판", "없는말", "시행 계획 1"):
                    expression = '"' + term.replace('"', '""') + '"'
                    expected = {r[0] for r in index.db.execute(
                        "SELECT DISTINCT path FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?",
                        (expression,))}
                    self.assertEqual(index._matching_paths(expression), expected, term)
            finally:
                index.close()

    def test_matching_paths_uses_rowid_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            index = _synthetic_index(tmp, documents=2, paragraphs=3)
            try:
                plan = " ".join(str(row[-1]) for row in index.db.execute(
                    "EXPLAIN QUERY PLAN SELECT DISTINCT path FROM content_paragraph_map WHERE para_rowid IN"
                    " (SELECT rowid FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?)",
                    ('"해체공사"',)))
                self.assertIn("content_paragraph_map_rowid", plan)
            finally:
                index.close()


if __name__ == "__main__":
    unittest.main()
