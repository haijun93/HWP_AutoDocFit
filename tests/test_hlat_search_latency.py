"""HLAT1 위계검색 지연 개선: 바꾼 내부 단계가 예전 방식과 같은 값을 내는지 함수 단위로 고정한다."""
import tempfile
import unittest
from pathlib import Path

from docfit_core.timemachine import hierarchy_path
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


class BreadcrumbRowsReuseTests(unittest.TestCase):
    def test_known_rows_give_same_breadcrumb_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            index = _synthetic_index(tmp, documents=8, paragraphs=6)
            try:
                paths = sorted({r[0] for r in index.db.execute("SELECT path FROM content_paragraph_fts")})
                index.db.execute("INSERT INTO content_table_cells(path,paragraph_index,cell_id) VALUES(?,?,?)",
                                 (paths[1], 0, 7))
                expected = index._breadcrumb_rows(paths)
                known = {}
                for row in index._select_paragraphs_for_paths(
                        "rowid AS paragraph_rowid, path, paragraph_index, body, level, level_rank", paths[:5]):
                    known.setdefault(row["path"], []).append(row)

                def by_path(rows):
                    grouped = {}
                    for row in rows:
                        grouped.setdefault(row["path"], []).append(
                            {k: row[k] for k in ("paragraph_index", "body", "level", "level_rank", "table_cell_id")})
                    return grouped

                self.assertEqual(by_path(index._breadcrumb_rows(paths, known=known)), by_path(expected))
                self.assertEqual(by_path(expected)[paths[1]][0]["table_cell_id"], 7)
            finally:
                index.close()


class BreadcrumbPrefixTests(unittest.TestCase):
    # (level, level_rank, body) 문단 목록. 표 칸 제목·뒤쪽 문서 제목·제목 없는 문서를 섞는다.
    DOCUMENTS = {
        "plain": [("document_title", 0, "# 시설 사업"), ("chapter", 1, "## 관광안내판"),
                  ("section", 2, "### 안전관리"), ("body", 8, "해체공사 본문"), ("body", 8, "다른 본문")],
        "late_title": [("body", 8, "머리 본문"), ("chapter", 1, "제1장 총칙"), ("body", 8, "해체공사"),
                       ("document_title", 0, "늦은 문서 제목"), ("body", 8, "끝")],
        "no_title": [("body", 8, "첫 본문"), ("chapter", 1, "제2장 계획"), ("body", 8, "해체공사 계획")],
        "cell_title": [("table_cell", 8, "짧"), ("table_cell", 8, "관광안내판 정비 계획"),
                       ("chapter", 1, "1. 개요"), ("body", 8, "해체공사 개요")],
        "grouped_cells": [("table_cell", 8, "부제 문구"), ("body", 8, "본문 하나"),
                          ("table_cell", 8, "본제 문구"), ("chapter", 1, "1. 추진"), ("body", 8, "해체공사")],
    }

    def _index(self, tmp):
        index = ContentSearchIndex(Path(tmp) / "index.sqlite")
        rows, cells = [], []
        for name, paragraphs in self.DOCUMENTS.items():
            path = str(Path(tmp) / f"{name}.md")
            for number, (level, rank, body) in enumerate(paragraphs):
                rows.append((path, f"{name}.md", number, level, rank, body))
            if name == "grouped_cells":
                cells += [(path, 0, 3), (path, 2, 3)]
        index.db.executemany("INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body)"
                             " VALUES(?,?,?,?,?,?)", rows)
        index.db.executemany("INSERT INTO content_table_cells(path,paragraph_index,cell_id) VALUES(?,?,?)", cells)
        index.db.commit()
        index._ensure_paragraph_map()
        return index

    def _trails(self, index, chosen_by_path, **kwargs):
        grouped = {}
        for row in index._breadcrumb_rows(list(chosen_by_path), **kwargs):
            grouped.setdefault(row["path"], []).append(row)
        return {path: hierarchy_path.breadcrumb(grouped.get(path, ()), chosen) for path, chosen in chosen_by_path.items()}

    def test_prefix_rows_give_same_breadcrumb_for_every_paragraph(self):
        with tempfile.TemporaryDirectory() as tmp:
            index = self._index(tmp)
            index._PARAGRAPH_SCAN_RATIO = 1  # 작은 시험 색인에서도 앞부분 읽기 경로를 탄다.
            try:
                for number in range(6):
                    until = {str(Path(tmp) / f"{name}.md"): number for name in self.DOCUMENTS}
                    self.assertEqual(self._trails(index, until, until=until), self._trails(index, until), number)
                late = str(Path(tmp) / "late_title.md")
                self.assertEqual(index._paragraph_prefixes({late: 2}).__len__(), 3)
            finally:
                index.close()

    def test_prefix_reading_falls_back_when_too_many_paragraphs(self):
        with tempfile.TemporaryDirectory() as tmp:
            index = self._index(tmp)
            try:
                plain = str(Path(tmp) / "plain.md")
                late = str(Path(tmp) / "late_title.md")
                self.assertIsNone(index._paragraph_prefixes({plain: 4, late: 4}))  # 10/22문단 > 22//4
                self.assertEqual(self._trails(index, {plain: 4, late: 4}, until={plain: 4, late: 4}),
                                 self._trails(index, {plain: 4, late: 4}))
                until = {plain: 3}
                self.assertEqual(self._trails(index, until, until=until), self._trails(index, until))
                self.assertEqual([step["text"] for step in self._trails(index, until)[plain]],
                                 ["시설 사업", "관광안내판", "안전관리"])
            finally:
                index.close()


if __name__ == "__main__":
    unittest.main()
