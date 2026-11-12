"""Python 본문 색인·검색의 NFC 정규화 단위 시험."""
from __future__ import annotations

import os
import sqlite3
import tempfile
import unicodedata
import unittest
from pathlib import Path

from docfit_core.timemachine.content_search import (
    CURRENT_HIERARCHY_SCHEMA_VERSION,
    ContentSearchIndex,
    _nfc,
)


class NfcSearchIndexTests(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.base_dir = Path(self.temp_dir.name)
        self.db_path = self.base_dir / "test_content.db"
        self.index = ContentSearchIndex(self.db_path)

    def tearDown(self):
        self.index.close()
        self.temp_dir.cleanup()

    def test_nfc_helper_converts_nfd_to_nfc(self):
        # 조합형(NFD: ㄱ + ㅏ)과 완성형(NFC: 가)
        nfd_str = unicodedata.normalize("NFD", "가나다라 대한민국 2026")
        self.assertNotEqual(nfd_str, "가나다라 대한민국 2026")
        self.assertEqual(_nfc(nfd_str), "가나다라 대한민국 2026")
        self.assertEqual(_nfc(None), "")
        self.assertEqual(_nfc("abc ABC 123"), "abc ABC 123")

    def test_parse_query_normalizes_to_nfc(self):
        nfd_query = unicodedata.normalize("NFD", '"조합형 문구" 관광안내판 -제외단어 path:폴더명')
        parsed = self.index.parse_query(nfd_query)

        # 구문, 검색어, 제외어, 경로 필터 모두 NFC인지 검증
        for phrase in parsed["phrases"]:
            self.assertEqual(phrase, unicodedata.normalize("NFC", phrase))
            self.assertEqual(phrase, "조합형 문구")
        for term in parsed["terms"]:
            self.assertEqual(term, unicodedata.normalize("NFC", term))
            self.assertEqual(term, "관광안내판")
        for ex in parsed["exclude"]:
            self.assertEqual(ex, unicodedata.normalize("NFC", ex))
            self.assertEqual(ex, "제외단어")
        for p in parsed["path"]:
            self.assertEqual(p, unicodedata.normalize("NFC", p))
            self.assertEqual(p, "폴더명")

    def test_index_nfd_body_find_with_nfc_query(self):
        # 본문을 조합형(NFD)으로 저장한 파일
        nfd_body = unicodedata.normalize("NFD", "제1장 시설물 관리\n\n관광안내판 철거 및 보수 공사를 추진한다.\n세부 일정은 별첨.")
        file_path = self.base_dir / "nfd_document.txt"
        file_path.write_text(nfd_body, encoding="utf-8")

        res = self.index.index_file(file_path)
        self.assertTrue(res["indexed"])

        # 완성형(NFC) 검색어로 search() 검색
        nfc_query = unicodedata.normalize("NFC", "관광안내판")
        search_results = self.index.search(nfc_query)
        self.assertEqual(len(search_results), 1)
        self.assertEqual(search_results[0]["path"], os.path.abspath(file_path))

        # 발췌문과 하이라이트 오프셋 일치 검증
        snippet = search_results[0]["snippet"]
        highlights = search_results[0]["highlights"]
        self.assertGreater(len(highlights), 0)
        h_start, h_end = highlights[0]
        self.assertEqual(snippet[h_start:h_end], "관광안내판")

        # keyword_search()로도 완성형 검색어로 검색
        kw_results = self.index.keyword_search(nfc_query)
        self.assertEqual(len(kw_results), 1)
        kw_loc = kw_results[0]["locations"][0]
        kw_snippet = kw_loc["snippet"]
        kw_h_start, kw_h_end = kw_loc["highlights"][0]
        self.assertEqual(kw_snippet[kw_h_start:kw_h_end], "관광안내판")

    def test_index_nfc_body_find_with_nfd_query(self):
        # 본문을 완성형(NFC)으로 저장한 파일
        nfc_body = unicodedata.normalize("NFC", "제1장 도시환경 정비\n\n공원 편의시설 점검 및 보수를 진행한다.\n")
        file_path = self.base_dir / "nfc_document.txt"
        file_path.write_text(nfc_body, encoding="utf-8")

        res = self.index.index_file(file_path)
        self.assertTrue(res["indexed"])

        # 조합형(NFD) 검색어로 search() 검색
        nfd_query = unicodedata.normalize("NFD", "편의시설")
        search_results = self.index.search(nfd_query)
        self.assertEqual(len(search_results), 1)
        self.assertEqual(search_results[0]["path"], os.path.abspath(file_path))

        snippet = search_results[0]["snippet"]
        highlights = search_results[0]["highlights"]
        self.assertGreater(len(highlights), 0)
        h_start, h_end = highlights[0]
        self.assertEqual(snippet[h_start:h_end], "편의시설")

        # 조합형(NFD) 검색어로 keyword_search() 검색
        kw_results = self.index.keyword_search(nfd_query)
        self.assertEqual(len(kw_results), 1)
        kw_loc = kw_results[0]["locations"][0]
        kw_snippet = kw_loc["snippet"]
        kw_h_start, kw_h_end = kw_loc["highlights"][0]
        self.assertEqual(kw_snippet[kw_h_start:kw_h_end], "편의시설")

    def test_filename_search_nfc_and_nfd(self):
        # 1) 조합형(NFD) 파일명을 가진 파일
        nfd_filename = unicodedata.normalize("NFD", "사업계획서_최종본.txt")
        file_nfd = self.base_dir / nfd_filename
        file_nfd.write_text("본문 내용", encoding="utf-8")

        # 2) 완성형(NFC) 파일명을 가진 파일
        nfc_filename = unicodedata.normalize("NFC", "운영결과_보고서.txt")
        file_nfc = self.base_dir / nfc_filename
        file_nfc.write_text("본문 내용", encoding="utf-8")

        self.index.index_file(file_nfd)
        self.index.index_file(file_nfc)

        # 완성형 검색어로 NFD 파일 찾기
        nfc_q1 = unicodedata.normalize("NFC", "사업계획서")
        found1 = self.index.search_files(nfc_q1)
        self.assertEqual(len(found1), 1)
        self.assertEqual(found1[0]["name"], unicodedata.normalize("NFC", "사업계획서_최종본.txt"))

        # 조합형 검색어로 NFD 파일 찾기
        nfd_q1 = unicodedata.normalize("NFD", "사업계획서")
        found1_nfd = self.index.search_files(nfd_q1)
        self.assertEqual(len(found1_nfd), 1)

        # 조합형 검색어로 NFC 파일 찾기
        nfd_q2 = unicodedata.normalize("NFD", "운영결과")
        found2 = self.index.search_files(nfd_q2)
        self.assertEqual(len(found2), 1)
        self.assertEqual(found2[0]["name"], unicodedata.normalize("NFC", "운영결과_보고서.txt"))

    def test_schema_version_is_bumped_and_triggers_reindex(self):
        # 스키마 버전이 5로 올라갔는지 확인
        self.assertEqual(CURRENT_HIERARCHY_SCHEMA_VERSION, 5)

        # 구버전(버전 4) 색인 기록을 가상으로 주입했을 때 재색인 대상으로 잡히는지 확인
        dummy_path = str(self.base_dir / "legacy_doc.txt")
        with self.index._lock:
            self.index.db.execute(
                "INSERT INTO content_meta(path, size, mtime, ctime, reference_date, name, extension, hierarchy_schema_version) "
                "VALUES(?, 100, 1000.0, 1000.0, NULL, 'legacy_doc.txt', 'txt', 4)",
                (dummy_path,)
            )
            self.index.db.execute(
                "INSERT INTO content_paragraph_fts(path, name, paragraph_index, level, level_rank, body, location_label, section) "
                "VALUES(?, 'legacy_doc.txt', 0, 'body', 8, '레거시 문서 본문', '문단 1', NULL)",
                (dummy_path,)
            )
            self.index.db.commit()

        reindex_needed = self.index.needs_reindex_count()
        self.assertGreaterEqual(reindex_needed, 1)


if __name__ == "__main__":
    unittest.main()
