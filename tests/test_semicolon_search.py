"""합성 문단만 사용하는 SEMI1 회귀 시험."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_core.timemachine.content_search import ContentSearchIndex, ParagraphRecord
from docfit_core.timemachine.related_query import parse_related_query
from docfit_core.timemachine.search_backend import SqliteBackend, RustBackend


class RelatedQueryTests(unittest.TestCase):
    def test_groups_whitespace_quotes_empty_and_ampersand(self):
        groups = parse_related_query(' ; 2026;; 공사  계약 ; "a;b" R&D; "";')
        self.assertEqual([g.query for g in groups], ["2026", "공사  계약", '"a;b" R&D'])
        self.assertEqual(groups[1].terms, ("공사", "계약"))
        self.assertEqual(groups[2].terms, ("a;b", "R&D"))
        self.assertEqual(parse_related_query("; ;"), ())
        self.assertEqual(parse_related_query("“공사; 계약”")[0].terms, ("공사; 계약",))

    def test_escape_unicode_and_incomplete(self):
        self.assertEqual(parse_related_query(r'"a\";b"; "C:\temp"')[0].terms, ('a";b',))
        self.assertEqual(parse_related_query(r'"C:\temp"')[0].terms, ('C:\\temp',))
        self.assertEqual(parse_related_query("공사")[0].terms, ("공사",))
        for query in ('"미완료', 'a' * 201, None):
            with self.subTest(query=query), self.assertRaises(ValueError):
                parse_related_query(query)


class RelatedSearchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="semi1-")
        self.root = Path(self.temp.name)
        self.index = ContentSearchIndex(self.root / "index.sqlite")
        self.backend = SqliteBackend(self.index)

    def tearDown(self):
        self.index.close()
        self.temp.cleanup()

    def add(self, name, lines):
        path = self.root / name
        path.write_text("\n".join(lines), encoding="utf-8")
        self.index.index_file(path)
        return path

    def test_document_intersection_does_not_allow_split_terms_inside_group(self):
        good = self.add("good.txt", ["2026 추진", "공사 계약 검토"])
        self.add("split.txt", ["2026", "공사", "계약"])
        self.add("missing.txt", ["2025", "공사 계약"])
        rows = self.backend.search("2026; 공사 계약", related_groups=True)
        self.assertEqual([r["path"] for r in rows], [str(good)])
        self.assertEqual([g["match_count"] for g in rows[0]["group_matches"]], [1, 1])
        self.assertEqual([g["matches"][0]["paragraph_index"] for g in rows[0]["group_matches"]], [0, 1])

    def test_hierarchy_precedes_count_and_count_sums_groups(self):
        title = self.add("title.md", ["# 2026", "공사 계약"])
        self.add("many.md", ["2026", "2026", "공사 계약", "공사 계약"])
        rows = self.index.search("2026; 공사 계약", related_groups=True)
        self.assertEqual(rows[0]["path"], str(title))
        self.assertEqual([r["total_matches"] for r in rows], [2, 4])
        self.assertEqual(rows[0]["group_matches"][0]["matches"][0]["level_rank"], 0)

    def test_filename_quotes_empty_and_literal_ampersand(self):
        good = self.add("2026 공사 계약 R&D.txt", ["본문"])
        self.add("2026 공사 계약 RD.txt", ["본문"])
        rows = self.index.search_files(' ; 2026;; "공사 계약"; R&D;', related_groups=True)
        self.assertEqual([r["path"] for r in rows], [str(good)])
        self.assertEqual(len(rows[0]["group_matches"]), 3)

    def test_option_off_uses_existing_methods_and_preserves_fields(self):
        self.add("old.txt", ["2026 공사 계약"])
        expected = self.index.keyword_search("공사 계약")
        self.assertEqual(self.backend.search("공사 계약"), expected)
        grouped = self.backend.search("공사 계약", related_groups=True)
        self.assertTrue(set(expected[0]).issubset(grouped[0]))
        self.assertNotIn("group_matches", expected[0])

    def test_filter_pagination_and_no_original_access(self):
        a = self.add("a.txt", ["2026", "공사 계약"])
        self.add("b.txt", ["2026", "공사 계약"])
        with patch("builtins.open", side_effect=AssertionError("원문 접근")):
            rows = self.backend.search("2026; 공사 계약", related_groups=True, limit=1, offset=1)
        self.assertEqual(rows[0]["name"], "b.txt")
        self.assertEqual(rows[0]["relevance_rank"], 2)
        self.assertEqual(self.backend.search("2026; 공사 계약", related_groups=True,
                         roots=[str(self.root / "other")]), [])
        self.assertEqual(self.backend.search("2026; 공사 계약", related_groups=True,
                         end=a.stat().st_mtime-1), [])

    @unittest.skipUnless(os.environ.get("SEMI1_KLIB_EXE"), "SEMI1 Rust 실행 파일 미지정")
    def test_rust_parity_evidence_filters_updates_and_deletes(self):
        from docfit_core.timemachine.content_search import extract_paragraphs
        paths = [self.add("2026 공사 계약.md", ["# 2026", "공사 계약", "R&D a;b"]),
                 self.add("split.txt", ["2026", "공사", "계약"])]
        rust = RustBackend(os.environ["SEMI1_KLIB_EXE"], index_dir=self.root / "rust")
        try:
            rust.index_many((str(p), extract_paragraphs(p)) for p in paths)
            for query in ("2026; 공사 계약", '2026; "공사 계약"', 'R&D; "a;b"', '; ;'):
                for filenames in (False, True):
                    options = dict(related_groups=True, filenames=filenames)
                    left = self.backend.search(query, **options)
                    right = rust.search(query, **options)
                    with self.subTest(query=query, filenames=filenames):
                        self.assertEqual([r["path"] for r in left], [r["path"] for r in right])
                        self.assertEqual([r["group_matches"] for r in left], [r["group_matches"] for r in right])
            self.assertEqual(rust.search("2026", related_groups=True, roots=[self.root / "other"]), [])
            self.assertEqual(rust.search("2026", related_groups=True, end=0), [])
            rust.remove_file(str(paths[0]))
            self.assertEqual(rust.search("2026; 공사 계약", related_groups=True), [])
        finally:
            rust.close()
