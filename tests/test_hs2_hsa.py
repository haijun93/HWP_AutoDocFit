"""HS2-H 제목/본문 분산 검색 재현 및 회귀."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core.timemachine.content_search import ContentSearchIndex


class AncestorSearchTests(unittest.TestCase):
    def test_split_heading_and_body_selects_body_after_direct_match(self):
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            root = Path(tmp)
            index = ContentSearchIndex(root / "index.sqlite")
            try:
                documents = {
                    "split.md": "# 시설 사업\n\n## 관광안내판\n\n해체공사 시행 계획을 수립한다.",
                    "direct.md": "# 시설 사업\n\n## 일반 시설\n\n관광안내판 해체공사 시행 계획을 수립한다.",
                }
                with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                    for name, body in documents.items():
                        path = root / name
                        path.write_text(body, encoding="utf-8")
                        self.assertTrue(index.index_file(path)["indexed"])
                rows = index.search("관광안내판 해체공사")
                self.assertEqual([r["name"] for r in rows], ["direct.md", "split.md"])
                self.assertEqual([r["relevance_rank"] for r in rows], [1, 2])
                self.assertEqual(rows[1]["snippet"], "해체공사 시행 계획을 수립한다.")
                self.assertEqual(rows[1]["breadcrumb"], ["시설 사업", "관광안내판"])
                self.assertEqual(rows[1]["context_hits"], 1)
                self.assertIn("관광안내판 해체공사", rows[0]["snippet"])
                self.assertEqual(rows[0]["breadcrumb"], ["시설 사업", "일반 시설"])
            finally:
                index.close()

    def test_full_ancestor_text_multiple_levels_and_sibling_boundary(self):
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            root = Path(tmp)
            index = ContentSearchIndex(root / "index.sqlite")
            try:
                path = root / "nested.md"
                path.write_text("# 시설 사업\n\n## " + "긴제목 " * 20 +
                                "관광안내판\n\n### 안전관리\n\n해체공사 계획이다.\n\n"
                                "## 다른 시설\n\n해체공사 해체공사 무관한 형제 본문이다.", encoding="utf-8")
                with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                    self.assertTrue(index.index_file(path)["indexed"])
                # 표시 경로의 60자 제한 밖에 있는 단어도 검색 문맥에는 들어간다.
                result = index.search("관광안내판 안전관리 해체공사")[0]
                self.assertEqual(result["snippet"], "해체공사 계획이다.")
                self.assertEqual(result["context_hits"], 2)
                self.assertEqual(result["breadcrumb"][-1], "안전관리")
                self.assertNotIn("관광안내판", result["breadcrumb_text"])
                # 안전관리가 없는 질의에서도 다른 장의 본문으로 조상이 새지 않는다.
                result = index.search("관광안내판 해체공사")[0]
                self.assertEqual(result["snippet"], "해체공사 계획이다.")
                self.assertEqual(result["context_hits"], 1)
                self.assertEqual(index.search("관광안내판 해체공사 -안전관리"), [])
                self.assertEqual(index.search("관광안내판 해체공사", roots=[str(root / "elsewhere")]), [])
                self.assertEqual(index.needs_reindex_count(), 0)
                stored = index.db.execute("SELECT body FROM content_paragraph_fts ORDER BY rowid").fetchall()
                self.assertIn("해체공사 계획이다.", [r[0] for r in stored])
            finally:
                index.close()

    def test_direct_match_ranking_unchanged_when_context_candidate_added(self):
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            root = Path(tmp)
            index = ContentSearchIndex(root / "index.sqlite")
            try:
                with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                    for name, text in (("a.md", "# 관광안내판 해체공사\n\n설명이다."),
                                       ("b.md", "# 시설 사업\n\n관광안내판 해체공사 계획이다.")):
                        path = root / name
                        path.write_text(text, encoding="utf-8")
                        self.assertTrue(index.index_file(path)["indexed"])
                    before = {sort: index.search("관광안내판 해체공사", sort=sort)
                              for sort in ("hierarchy_relevance", "mentions", "relevance")}
                    path = root / "context.md"
                    path.write_text("# 관광안내판\n\n해체공사 계획이다.", encoding="utf-8")
                    self.assertTrue(index.index_file(path)["indexed"])
                for sort, expected in before.items():
                    with self.subTest(sort=sort):
                        result = index.search("관광안내판 해체공사", sort=sort)
                        self.assertEqual(result[:2], expected)
                        self.assertEqual(result[2]["name"], "context.md")
            finally:
                index.close()
