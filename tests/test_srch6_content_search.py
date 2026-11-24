"""합성 자료로 opt-in Rust 검색의 순위·범위·캐시 수명을 검증한다."""
import os
from pathlib import Path
import tempfile
import unittest

from docfit_core.timemachine.search_backend import RustBackend
from scripts.search_quality_eval import build_srch6_corpus, metrics


class Srch6MetricsTests(unittest.TestCase):
    def test_suite_has_all_five_types_and_at_least_twelve_queries(self):
        with tempfile.TemporaryDirectory() as temp:
            gold, kinds = build_srch6_corpus(Path(temp))
            self.assertGreaterEqual(len(gold), 12)
            self.assertTrue({"제목", "밀도", "위계", "AND", "phrase"}.issubset(set(kinds.values())))
            for relevant in gold.values():
                self.assertEqual(len(relevant), 4)
                self.assertTrue(all((Path(temp) / name).is_file() for name in relevant))

    def test_pairwise_order_penalizes_missing_and_reversed_gold(self):
        self.assertEqual(metrics(["b", "a"], ["a", "b"])["gold_order_pairwise_accuracy"], 0)
        self.assertEqual(metrics(["a", "b"], ["a", "b"])["gold_order_pairwise_accuracy"], 1)
        self.assertEqual(metrics([], ["a", "b"])["gold_order_pairwise_accuracy"], 0)


@unittest.skipUnless(os.environ.get("KLIB_EXE"), "KLIB_EXE가 있어야 Rust 실제 검색 시험")
class Srch6RustSearchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="srch6-contract-")
        self.root = Path(self.temp.name)
        self.backend = RustBackend(os.environ["KLIB_EXE"], index_dir=self.root / "index")

    def tearDown(self):
        self.backend.close()
        self.temp.cleanup()

    def put(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_heading_priority_density_and_page_preserve_locations(self):
        paths = [self.put(f"scope/doc-{i}.md", "# 행정 절차\n\n" + "행정 절차 검토 " * (i + 1)) for i in range(6)]
        self.backend.index_files(paths)
        rows = self.backend.search("행정 절차", roots=[str(paths[0].parent)], hierarchy=True)
        self.assertEqual([Path(r["path"]).name for r in rows], [f"doc-{i}.md" for i in reversed(range(6))])
        page = self.backend.search("행정 절차", roots=[str(paths[0].parent)], hierarchy=True, offset=2, limit=2)
        self.assertEqual([r["path"] for r in page], [r["path"] for r in rows[2:4]])
        self.assertTrue(all(r["locations"] and r["breadcrumb_text"] for r in page))
        self.assertTrue(any(len(r["locations"]) >= 2 for r in rows))

    def test_roots_are_applied_before_page_and_profile_is_opt_in(self):
        paths = [self.put(f"outside/a-{i}.md", "# 행정\n\n행정 검토 " * 5) for i in range(30)]
        inside = self.put("inside/a.md", "# 행정\n\n행정 검토")
        self.backend.index_files([*paths, inside])
        rows = self.backend.search("행정", roots=[str(inside.parent)], hierarchy=True, limit=1)
        self.assertEqual([r["path"] for r in rows], [str(inside)])
        request = {"op": "search", "query": "행정", "limit": 1}
        plain = self.backend._request(request)
        profiled = self.backend._request(dict(request, profile=True))
        self.assertNotIn("_profile", plain)
        self.assertIn("tantivy_ms", profiled["_profile"])
        self.assertEqual(plain["results"], profiled["results"])

    def test_cache_invalidates_after_update_delete_and_reopen(self):
        path = self.put("a.md", "# 행정\n\n행정 지원")
        self.backend.index_file(path)
        self.assertTrue(self.backend.search("행정", hierarchy=True))
        path.write_text("# 안전\n\n안전 점검", encoding="utf-8")
        self.backend.index_file(path)
        self.assertEqual(self.backend.search("행정", hierarchy=True), [])
        self.assertIn("안전", self.backend.search("안전", hierarchy=True)[0]["snippet"])
        self.backend.close()
        self.assertTrue(self.backend.search("안전", hierarchy=True))
        self.backend.remove_file(path)
        self.assertEqual(self.backend.search("안전", hierarchy=True), [])

    def test_document_and_can_match_separate_paragraphs_but_phrase_cannot(self):
        path = self.put("split.md", "# 계획\n\n행정 지원\n\n정책 운영")
        self.backend.index_file(path)
        self.assertEqual(len(self.backend.search("행정 정책", hierarchy=True)), 1)
        self.assertEqual(self.backend.search('"행정 정책"', hierarchy=True), [])


if __name__ == "__main__":
    unittest.main()
