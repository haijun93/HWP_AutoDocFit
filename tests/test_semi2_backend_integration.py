"""공통 프로세스·색인에서 SRCH/SEMI 경로의 공존과 비교 계약을 검증한다."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from docfit_core.timemachine.content_search import ContentSearchIndex, extract_paragraphs
from docfit_core.timemachine.search_backend import ModeSearchBackend, RustBackend, SqliteBackend


@unittest.skipUnless(os.environ.get("KLIB_EXE"), "KLIB_EXE가 있어야 실제 Rust 통합 시험")
class CommonRelatedBackendTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="semi2-common-")
        self.root = Path(self.temp.name)
        self.index = ContentSearchIndex(self.root / "content.sqlite")
        self.sqlite = SqliteBackend(self.index)
        self.rust = RustBackend(index_dir=self.root / "tantivy")
        self.paths = []
        for name, text in {
            "2026 공사 계약.md": "# 2026\n\n공사 계약 검토.\nR&D a;b\n",
            "many.txt": "2026\n2026\n공사 계약\n공사 계약\n",
            "split.txt": "2026 공사 일정. 계약 검토.\n",
            "Report R&D.txt": "İ REPORT a;b\n",
            "#ieeee-2026.txt": "2026 공사 계약\n",
        }.items():
            path = self.root / name
            path.write_text(text, encoding="utf-8")
            self.paths.append(path)
        self.sqlite.index_files(self.paths)
        self.rust.index_files(self.paths)

    def tearDown(self):
        self.rust.close()
        self.index.close()
        self.temp.cleanup()

    def assert_same(self, query, **options):
        left = self.sqlite.search(query, related_groups=True, **options)
        right = self.rust.search(query, related_groups=True, **options)
        self.assertEqual([r["path"] for r in left], [r["path"] for r in right])
        self.assertEqual([r["group_matches"] for r in left], [r["group_matches"] for r in right])
        return right

    def test_native_index_batch_related_filters_and_default_coexist(self):
        before = self.rust.search("공사", hierarchy=True)
        process = self.rust._process
        for query in ("2026; 공사 계약", 'R&D; "a;b"', 'i; "a;b"', "report", "port", "; ;"):
            for filenames in (False, True):
                with self.subTest(query=query, filenames=filenames):
                    self.assert_same(query, filenames=filenames)
        self.assert_same("2026; 공사 계약", offset=1, limit=1)
        self.assert_same("2026", roots=[self.root / "missing"])
        self.assert_same("2026", end=0)
        self.assert_same("2026", start=0)
        self.assertEqual(before, self.rust.search("공사", hierarchy=True))
        self.assertNotIn("group_matches", before[0])
        self.assertIs(process, self.rust._process)

    def test_records_replace_reopen_native_update_delete_and_cancel(self):
        path = self.paths[0]
        self.rust.index_records(path, extract_paragraphs(path))
        self.assert_same("2026; 공사 계약")
        self.assertTrue(self.rust.search("공사", hierarchy=True))
        self.rust.close()
        self.assert_same("2026; 공사 계약")
        path.write_text("안전 점검", encoding="utf-8")
        self.sqlite.index_file(path)
        self.rust.index_file(path)
        self.assert_same("2026; 공사 계약")
        self.sqlite.remove_file(path)
        self.rust.remove_file(path)
        self.assert_same("안전")
        import threading
        self.assertTrue(self.rust.cancel(threading.Event()))
        self.assert_same("2026")

    def test_large_related_metadata_keeps_default_tokens_and_all_evidence(self):
        path = self.root / "large.txt"
        path.write_text(("공사 계약 " + "자료 " * 60 + "\n") * 500, encoding="utf-8")
        self.sqlite.index_file(path)
        self.rust.index_file(path)
        rows = self.assert_same("공사 계약", roots=[self.root], limit=500)
        large = next(row for row in rows if row["path"] == str(path))
        self.assertEqual(large["group_matches"][0]["match_count"], 500)
        self.assertTrue(any(r["path"] == str(path) for r in self.rust.search("공사", hierarchy=True)))

    def test_auxiliary_extraction_error_does_not_break_default_content_index(self):
        path = self.root / "legacy-lossy.txt"
        path.write_bytes(b"native common \xff")
        self.assertTrue(self.rust.index_file(path)["indexed"])
        self.assertTrue(any(r["path"] == str(path) for r in self.rust.search("native", hierarchy=True)))
        with self.assertRaisesRegex(RuntimeError, "연관 검색 자료 추출 실패"):
            self.rust.search("native", related_groups=True)
        self.assertTrue(any(r["path"] == str(path) for r in self.rust.search("native", hierarchy=True)))

    def test_shadow_compares_groups_order_and_returns_sqlite_authority(self):
        log = self.root / "shadow.jsonl"
        backend = ModeSearchBackend(self.sqlite, self.rust, lambda: "shadow_compare", shadow_log=log)
        result = backend.search("2026; 공사 계약", related_groups=True)
        self.assertEqual(result, self.sqlite.search("2026; 공사 계약", related_groups=True))
        record = json.loads(log.read_text(encoding="utf-8"))
        for key in ("related_groups", "result_set_match", "result_order_match", "group_matches_match"):
            self.assertTrue(record[key], key)
        for engine in (self.sqlite, self.rust):
            with self.assertRaises(ValueError if engine is self.sqlite else RuntimeError):
                engine.search('"미완료', related_groups=True)
