# -*- coding: utf-8 -*-
"""합성 코퍼스(시드 20261124) 기반 연관 검색 결과 집합·순위·근거 검증 시험."""
import os
import tempfile
import unittest
from pathlib import Path

from docfit_core.timemachine.related_search import search_related
from docfit_core.timemachine.search_backend import SqliteBackend
from scripts.search_synthetic import make_corpus
from tests.test_related_search import MockIndex


class RelatedSyntheticCorpusTests(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.folder = Path(self.tmpdir.name)
        # 시드 20261124로 합성 코퍼스 20개 생성
        self.files = make_corpus(self.folder, count=20, seed=20261124)
        self.index = MockIndex()

        # 생성된 텍스트 파일을 파싱하여 MockIndex에 적재
        for file_path in self.files:
            text = file_path.read_text(encoding="utf-8")
            lines = [line.strip() for line in text.splitlines() if line.strip()]
            paragraphs = []
            for idx, line in enumerate(lines):
                if line.startswith("제") and "장" in line:
                    level = "chapter"
                    level_rank = 1
                else:
                    level = "body"
                    level_rank = 8
                paragraphs.append((line, level, level_rank))

            stat = file_path.stat()
            self.index.add_document(
                str(file_path),
                file_path.name,
                "txt",
                stat.st_mtime,
                stat.st_size,
                paragraphs,
            )

    def tearDown(self):
        self.index.close()
        self.tmpdir.cleanup()

    def test_synthetic_corpus_generation_deterministic(self):
        """동일 시드(20261124)에서 항상 동일한 코퍼스가 생성되는지 확인."""
        tmp2 = tempfile.TemporaryDirectory()
        folder2 = Path(tmp2.name)
        files2 = make_corpus(folder2, count=20, seed=20261124)

        for f1, f2 in zip(self.files, files2):
            self.assertEqual(f1.read_text(encoding="utf-8"), f2.read_text(encoding="utf-8"))
        tmp2.cleanup()

    def test_grouped_search_on_synthetic_corpus(self):
        """합성 코퍼스에 대해 연관 질의(교집합 및 문단 AND) 수행 검증."""
        query = "행정 절차 지원; 계획"
        results = search_related(self.index, query)

        self.assertGreater(len(results), 0, "합성 코퍼스에서 결과가 검색되어야 함")

        for row in results:
            # 1. 문서 단위 교집합 확인: 모든 묶음에 대한 일치 수가 1 이상이어야 함
            group_matches = row["group_matches"]
            self.assertEqual(len(group_matches), 2)
            self.assertGreater(group_matches[0]["match_count"], 0)
            self.assertGreater(group_matches[1]["match_count"], 0)

            # 2. 문단 단위 AND 확인: 각 묶음의 일치 문단은 묶음의 모든 낱말을 포함해야 함
            for g in group_matches:
                terms = g["terms"]
                for m in g["matches"]:
                    snippet = m["snippet"]
                    for term in terms:
                        self.assertIn(term, snippet, f"발췌문에 묶음 낱말 '{term}'이 포함되어야 함")

    def test_ranking_on_synthetic_corpus(self):
        """합성 코퍼스 검색 결과의 위계 우선 및 일치수 순위 검증."""
        query = "추진 계획; 환경"
        results = search_related(self.index, query)

        if len(results) >= 2:
            for i in range(len(results) - 1):
                cur = results[i]
                nxt = results[i + 1]

                cur_min_rank = min(m["level_rank"] for g in cur["group_matches"] for m in g["matches"])
                nxt_min_rank = min(m["level_rank"] for g in nxt["group_matches"] for m in g["matches"])

                if cur_min_rank != nxt_min_rank:
                    self.assertLess(cur_min_rank, nxt_min_rank, "최고 위계(더 낮은 rank 번호)가 우선해야 함")
                else:
                    self.assertGreaterEqual(
                        cur["total_matches"], nxt["total_matches"],
                        "동일 위계에서는 일치 문단 수 합이 더 많은 쪽이 우선해야 함"
                    )

    def test_backend_protocol_compatibility(self):
        """SqliteBackend 어댑터가 SearchBackend 인터페이스 규격을 온전히 준수하는지 검증."""
        backend = SqliteBackend(self.index)
        results = backend.search("행정; 데이터", related_groups=True, limit=5)

        self.assertLessEqual(len(results), 5)
        for r in results:
            self.assertIn("path", r)
            self.assertIn("group_matches", r)
            self.assertIn("score", r)
            self.assertIn("total_matches", r)


if __name__ == "__main__":
    unittest.main()
