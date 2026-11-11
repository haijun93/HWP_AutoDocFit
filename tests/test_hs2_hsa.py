"""HS2-H 제목/본문 분산 검색 재현 및 회귀."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core.timemachine.content_search import ContentSearchIndex


class AncestorSearchTests(unittest.TestCase):
    def test_split_heading_and_body_baseline(self):
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
                self.assertEqual([r["name"] for r in rows], ["split.md", "direct.md"])
                self.assertEqual([r["relevance_rank"] for r in rows], [1, 2])
                self.assertEqual(rows[0]["snippet"], "## 관광안내판")
                self.assertEqual(rows[0]["breadcrumb"], ["시설 사업"])
                self.assertEqual(rows[0]["context_hits"], 0)
                self.assertIn("관광안내판 해체공사", rows[1]["snippet"])
                self.assertEqual(rows[1]["breadcrumb"], ["시설 사업", "일반 시설"])
            finally:
                index.close()
