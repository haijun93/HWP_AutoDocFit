"""문서뷰폴더(DocGridService) 브리지 및 서비스 통합 시험 (DVF3)."""
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

from docfit_app.application_service import ApplicationService
from docfit_app.doc_grid_service import DocGridService
from tests.test_doc_grid_service import make_hwpx, picture_bytes


class TestDocGridBridge(unittest.TestCase):
    """DocGridService의 command 및 ApplicationService 브리지 연동 시험."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.settings_file = self.root / "settings.json"
        self.settings_file.write_text("{}", encoding="utf-8")
        self.app = ApplicationService(self.settings_file)
        self.docs_dir = self.root / "documents"
        self.docs_dir.mkdir()

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_bridge_list_empty_folder(self):
        """빈 폴더를 조회하면 빈 문서 목록이 반환된다."""
        res = self.app.dispatch("doc_grid", ["list", {"folder": str(self.docs_dir)}])
        self.assertTrue(res.get("ok"))
        self.assertEqual(res.get("documents"), [])
        self.assertEqual(res.get("folder"), str(self.docs_dir))

    def test_bridge_list_with_documents(self):
        """지원 문서들이 있는 폴더를 조회하면 정렬된 목록이 반환된다."""
        doc1 = self.docs_dir / "보고서.hwpx"
        make_hwpx(doc1, text="첫 문단 내용입니다")
        doc2 = self.docs_dir / "참고자료.pdf"
        doc2.write_bytes(b"%PDF-1.4\n%%EOF")
        ignored = self.docs_dir / "메모.txt"
        ignored.write_text("무시되는 텍스트", encoding="utf-8")

        res = self.app.dispatch("doc_grid", ["list", {"folder": str(self.docs_dir)}])
        self.assertTrue(res.get("ok"))
        docs = res.get("documents")
        self.assertEqual(len(docs), 2)
        names = [d["name"] for d in docs]
        self.assertIn("보고서.hwpx", names)
        self.assertIn("참고자료.pdf", names)

    def test_bridge_thumbnail_generation(self):
        """HWPX 문서의 썸네일을 요청하면 base64 인코딩된 데이터를 반환한다."""
        doc = self.docs_dir / "썸네일문서.hwpx"
        make_hwpx(doc, text="글자 카드 텍스트 미리보기", has_image=True)

        res = self.app.dispatch("doc_grid", ["thumbnail", {"path": str(doc)}])
        self.assertTrue(res.get("ok"))
        self.assertIn(res.get("type"), ("image", "text_card", "cached"))
        data = res.get("data")
        self.assertIsInstance(data, str)
        self.assertGreater(len(data), 0)

    def test_bridge_open_file(self):
        """문서 열기 요청 시 연결 프로그램(opener)이 호출된다."""
        doc = self.docs_dir / "열기문서.hwpx"
        make_hwpx(doc, text="열기 테스트")

        mock_opener = MagicMock()
        service = self.app._doc_grid()
        service.opener = mock_opener

        res = self.app.dispatch("doc_grid", ["open", {"path": str(doc)}])
        self.assertTrue(res.get("ok"))
        mock_opener.assert_called_once_with(str(doc.resolve()))

    def test_bridge_open_nonexistent_file(self):
        """존재하지 않는 파일 열기 요청 시 에러를 반환한다."""
        fake_path = str(self.docs_dir / "nonexistent.hwp")
        res = self.app.dispatch("doc_grid", ["open", {"path": fake_path}])
        self.assertFalse(res.get("ok"))
        self.assertEqual(res.get("code"), "FILE_NOT_FOUND")

    def test_bridge_clear_cache(self):
        """캐시 삭제 명령이 정상 동작한다."""
        res = self.app.dispatch("doc_grid", ["clear_cache", {}])
        self.assertTrue(res.get("ok"))
        self.assertIsInstance(res.get("cleared"), int)

    def test_bridge_invalid_commands(self):
        """잘못된 액션이나 매개변수에 대해 적절한 에러를 반환한다."""
        # 알 수 없는 액션
        res = self.app.dispatch("doc_grid", ["unknown_action", {}])
        self.assertFalse(res.get("ok"))
        self.assertEqual(res.get("code"), "NOT_AVAILABLE")

        # list에 folder 누락
        res = self.app.dispatch("doc_grid", ["list", {}])
        self.assertFalse(res.get("ok"))
        self.assertEqual(res.get("code"), "INVALID_FOLDER")

        # thumbnail에 path 누락
        res = self.app.dispatch("doc_grid", ["thumbnail", {}])
        self.assertFalse(res.get("ok"))
        self.assertEqual(res.get("code"), "INVALID_PATH")

        # 존재하지 않는 폴더 list
        res = self.app.dispatch("doc_grid", ["list", {"folder": str(self.root / "no_folder")}])
        self.assertFalse(res.get("ok"))
        self.assertEqual(res.get("code"), "NOT_FOUND")


if __name__ == "__main__":
    unittest.main()
