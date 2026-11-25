# -*- coding: utf-8 -*-
"""SQLite 기반 연관 검색(; doc_set 교집합, 문단 AND, 순위, 근거 응답) 단위 시험."""
import os
import sqlite3
import tempfile
import threading
import unittest
from pathlib import Path

from docfit_core.timemachine.related_search import evidence, search_related
from docfit_core.timemachine.search_backend import SqliteBackend


class MockIndex:
    """search_related 및 SqliteBackend 검증용 인메모리/임시 SQLite 인덱스 래퍼."""

    def __init__(self, db_path=":memory:"):
        self.db = sqlite3.connect(db_path, check_same_thread=False)
        self.db.row_factory = sqlite3.Row
        self._lock = threading.RLock()
        self._init_schema()

    def _init_schema(self):
        with self.db:
            self.db.execute("""
                CREATE TABLE content_meta (
                    path TEXT PRIMARY KEY,
                    name TEXT,
                    extension TEXT,
                    mtime REAL,
                    size INTEGER,
                    ctime REAL,
                    hierarchy_schema_version INTEGER DEFAULT 5
                )
            """)
            self.db.execute("""
                CREATE VIRTUAL TABLE content_paragraph_fts USING fts5(
                    path UNINDEXED,
                    paragraph_index UNINDEXED,
                    body,
                    level UNINDEXED,
                    level_rank UNINDEXED
                )
            """)

    def _ensure_paragraph_map(self):
        pass

    def add_document(self, path, name, extension, mtime, size, paragraphs):
        with self._lock, self.db:
            self.db.execute(
                "INSERT INTO content_meta (path, name, extension, mtime, size) VALUES (?, ?, ?, ?, ?)",
                (path, name, extension, mtime, size),
            )
            for idx, (body, level, level_rank) in enumerate(paragraphs):
                self.db.execute(
                    "INSERT INTO content_paragraph_fts (path, paragraph_index, body, level, level_rank) VALUES (?, ?, ?, ?, ?)",
                    (path, idx, body, level, level_rank),
                )

    def close(self):
        self.db.close()


class RelatedSearchTests(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tmpdir.name)
        self.index = MockIndex()

        # 문서 1: 2026과 공사 계약 모두 포함 (장 제목에 2026, 본문에 공사 계약)
        # -> 최고 위계: chapter (rank 1), 묶음 2개 모두 일치
        self.doc1 = str(self.root / "doc1_contract.hwpx")
        self.index.add_document(
            self.doc1,
            "doc1_contract.hwpx",
            "hwpx",
            1700000000.0,
            1024,
            [
                ("제1장 2026년 사업 개요", "chapter", 1),
                ("본 공사의 계약 체결은 조달청 지침에 따른다.", "body", 8),
            ],
        )

        # 문서 2: 2026과 공사 계약 모두 포함 (본문에만 2026과 공사 계약 일치)
        # -> 최고 위계: body (rank 8), 일치 문단 수 많음
        self.doc2 = str(self.root / "doc2_detail.pdf")
        self.index.add_document(
            self.doc2,
            "doc2_detail.pdf",
            "pdf",
            1700000100.0,
            2048,
            [
                ("2026년도 추진 일정 안내", "body", 8),
                ("이번 공사 계약 건에 대한 세부 내역", "body", 8),
                ("추가 공사 계약 진행 현황", "body", 8),
            ],
        )

        # 문서 3: 2026만 있고 '공사 계약'은 없는 문서 -> 교집합에서 제외되어야 함
        self.doc3 = str(self.root / "doc3_only_year.txt")
        self.index.add_document(
            self.doc3,
            "doc3_only_year.txt",
            "txt",
            1700000200.0,
            512,
            [("2026년 환경 정책 추진 계획", "chapter", 1)],
        )

        # 문서 4: '공사'와 '계약'이 서로 다른 문단에만 존재하는 문서
        # -> '공사 계약' 묶음(한 문단 AND)이 일치하지 않아 교집합에서 제외되어야 함
        self.doc4 = str(self.root / "doc4_split.docx")
        self.index.add_document(
            self.doc4,
            "doc4_split.docx",
            "docx",
            1700000300.0,
            4096,
            [
                ("2026년 업무 보고", "body", 8),
                ("건축 공사 일정표", "body", 8),
                ("체결된 계약 목록", "body", 8),
            ],
        )

        # 한 문단 안에서도 문장 경계가 다르면 한 묶음으로 일치하지 않는다.
        self.doc5 = str(self.root / "doc5_sentences.txt")
        self.index.add_document(
            self.doc5,
            "doc5_sentences.txt",
            "txt",
            1700000400.0,
            512,
            [("2026년 공사 일정이다. 계약 검토를 시작한다.", "body", 8)],
        )

    def tearDown(self):
        self.index.close()
        self.tmpdir.cleanup()

    def test_document_intersection_and_sentence_and(self):
        """'2026; 공사 계약' 검색 시 문서 교집합과 문장 단위 AND가 적용되는지 검증."""
        results = search_related(self.index, "2026; 공사 계약")
        matched_paths = [r["path"] for r in results]

        # doc1과 doc2는 두 묶음 모두 문단 단위로 일치하므로 포함
        self.assertIn(os.path.abspath(self.doc1), matched_paths)
        self.assertIn(os.path.abspath(self.doc2), matched_paths)

        # doc3은 '공사 계약' 묶음이 없으므로 제외
        self.assertNotIn(os.path.abspath(self.doc3), matched_paths)

        # doc4는 '공사'와 '계약'이 서로 다른 문단에 나뉘어 있으므로 제외
        self.assertNotIn(os.path.abspath(self.doc4), matched_paths)
        self.assertNotIn(os.path.abspath(self.doc5), matched_paths)

    def test_ranking_hierarchy_then_match_count(self):
        """순위: 최고 위계(rank 낮은 것 우선) > 묶음별 일치 문단 수 합 > 경로 정렬."""
        results = search_related(self.index, "2026; 공사 계약")
        self.assertEqual(len(results), 2)

        # doc1은 chapter (rank 1)이 있어 doc2 (최고 rank 8)보다 상위여야 함
        self.assertEqual(results[0]["path"], os.path.abspath(self.doc1))
        self.assertEqual(results[0]["relevance_rank"], 1)
        self.assertEqual(results[1]["path"], os.path.abspath(self.doc2))
        self.assertEqual(results[1]["relevance_rank"], 2)

    def test_response_fields_preservation_and_group_matches(self):
        """기존 응답 필드가 온전히 유지되고 group_matches가 추가되는지 검증."""
        results = search_related(self.index, "2026; 공사 계약")
        self.assertGreater(len(results), 0)
        row = results[0]

        # 기존 호환 필드
        required_legacy_keys = [
            "path", "name", "snippet", "highlights", "score", "total_matches",
            "best_level", "position", "breadcrumb", "breadcrumb_text",
            "context_hits", "relevance_rank", "extension", "mtime",
            "folder_path", "folder_name", "locations", "sections"
        ]
        for key in required_legacy_keys:
            self.assertIn(key, row, f"필수 기존 필드 누락: {key}")

        # 새 group_matches 필드
        self.assertIn("group_matches", row)
        self.assertEqual(len(row["group_matches"]), 2)
        group0 = row["group_matches"][0]
        self.assertEqual(group0["query"], "2026")
        self.assertIn("matches", group0)
        self.assertGreater(group0["match_count"], 0)

        match_item = group0["matches"][0]
        self.assertIn("paragraph_index", match_item)
        self.assertIn("level", match_item)
        self.assertIn("level_rank", match_item)
        self.assertIn("snippet", match_item)
        self.assertIn("highlights", match_item)

    def test_filenames_search_with_related_query(self):
        """파일명 검색(filenames=True) 시 파일명을 단일 문단으로 취급하여 교집합 수행."""
        # 파일명에 '2026'과 '공사'가 모두 있는 파일 추가
        doc_fn = str(self.root / "2026년_긴급공사_계약서.pdf")
        self.index.add_document(
            doc_fn,
            "2026년_긴급공사_계약서.pdf",
            "pdf",
            1700000500.0,
            1234,
            [],
        )
        results = search_related(self.index, "2026; 공사 계약 pdf", filenames=True)
        matched_names = [r["name"] for r in results]
        self.assertIn("2026년_긴급공사_계약서.pdf", matched_names)
        # doc1_contract.hwpx는 파일명에 2026이 없으므로 파일명 검색에서는 제외
        self.assertNotIn("doc1_contract.hwpx", matched_names)

    def test_sqlite_backend_adapter(self):
        """SqliteBackend 래퍼를 통한 related_groups=True 검색 동작 검증."""
        backend = SqliteBackend(self.index)
        results = backend.search("2026; 공사 계약", related_groups=True)
        self.assertEqual(len(results), 2)
        self.assertEqual(results[0]["path"], os.path.abspath(self.doc1))


if __name__ == "__main__":
    unittest.main()
