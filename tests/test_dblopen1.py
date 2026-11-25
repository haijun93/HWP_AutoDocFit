"""DBLOPEN1: 문서뷰·사진뷰 '연결 앱으로 열기'. opener 는 가짜를 쓰며 실제 앱은 실행하지 않는다."""
from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

from docfit_app import open_listed_file
from docfit_app.application_service import ApplicationService
from docfit_app.doc_grid_service import DOC_EXTENSIONS, DocGridService
from docfit_app.photo_album_service import PHOTO_EXTENSIONS, PhotoAlbumService


class OpenCommandTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.opener = MagicMock()
        self.photo = PhotoAlbumService(
            db_path=self.root / "photos.db", thumb_dir=self.root / "thumbs", opener=self.opener
        )
        self.docs = DocGridService(thumb_dir=self.root / "doc-thumbs", opener=self.opener)

    def tearDown(self):
        self.temp.cleanup()

    def _file(self, name: str) -> Path:
        path = self.root / name
        path.write_bytes(b"synthetic")
        return path

    def _open(self, service, path) -> dict:
        return service.command("open", {"path": str(path)})

    def test_photo_extensions_open_with_resolved_path(self):
        for ext in sorted(PHOTO_EXTENSIONS):
            with self.subTest(ext=ext):
                self.opener.reset_mock()
                path = self._file(f"합성 사진{ext.upper()}")
                res = self._open(self.photo, path)
                self.assertEqual(res, {"ok": True, "path": str(path.resolve())})
                self.opener.assert_called_once_with(str(path.resolve()))

    def test_document_extensions_open(self):
        for ext in sorted(DOC_EXTENSIONS):
            with self.subTest(ext=ext):
                self.opener.reset_mock()
                path = self._file(f"합성 문서{ext}")
                self.assertTrue(self._open(self.docs, path)["ok"])
                self.opener.assert_called_once_with(str(path.resolve()))

    def test_missing_file_is_file_not_found(self):
        for service, name in ((self.photo, "none.jpg"), (self.docs, "none.hwp")):
            res = self._open(service, self.root / name)
            self.assertEqual((res["ok"], res["code"]), (False, "FILE_NOT_FOUND"))
        self.opener.assert_not_called()

    def test_folder_is_not_opened(self):
        folder = self.root / "folder.jpg"
        folder.mkdir()
        self.assertEqual(self._open(self.photo, folder)["code"], "FILE_NOT_FOUND")
        self.opener.assert_not_called()

    def test_other_extensions_are_refused(self):
        for name in ("a.exe", "a.bat", "a.cmd", "a.lnk", "a.ps1", "a.jpg.exe", "noext"):
            path = self._file(name)
            for service in (self.photo, self.docs):
                with self.subTest(name=name, service=type(service).__name__):
                    res = self._open(service, path)
                    self.assertEqual((res["ok"], res["code"]), (False, "UNSUPPORTED_TYPE"))
        # 서로의 목록 대상도 열지 않는다.
        self.assertEqual(self._open(self.photo, self._file("b.hwp"))["code"], "UNSUPPORTED_TYPE")
        self.assertEqual(self._open(self.docs, self._file("b.jpg"))["code"], "UNSUPPORTED_TYPE")
        self.opener.assert_not_called()

    @unittest.skipUnless(os.name == "nt", "NTFS 대체 데이터 스트림")
    def test_alternate_data_stream_is_refused(self):
        host = self._file("host.exe")
        stream = f"{host}:hidden.jpg"
        with open(stream, "wb") as handle:
            handle.write(b"synthetic")
        self.assertFalse(self._open(self.photo, stream)["ok"])
        self.opener.assert_not_called()

    def test_invalid_path_payload(self):
        for service in (self.photo, self.docs):
            for payload in ({}, {"path": ""}, {"path": 3}):
                self.assertEqual(service.command("open", payload)["code"], "INVALID_PATH")
        self.opener.assert_not_called()

    def test_opener_failure_is_open_failed(self):
        self.opener.side_effect = OSError("연결된 앱 없음")
        with patch.object(open_listed_file.logger, "exception"):
            res = self._open(self.photo, self._file("c.png"))
        self.assertEqual((res["ok"], res["code"]), (False, "OPEN_FAILED"))
        self.assertIn("연결된 앱 없음", res["message"])

    def test_default_opener_is_used_without_injected_opener(self):
        path = self._file("d.jpg")
        self.photo.opener = None
        with patch.object(open_listed_file, "default_open") as default_open:
            self.assertTrue(self._open(self.photo, path)["ok"])
        default_open.assert_called_once_with(str(path.resolve()))


class OpenBridgeTests(unittest.TestCase):
    """application_service 와 데스크톱(WebView) 브리지 두 경로."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.path = self.root / "bridge.jpg"
        self.path.write_bytes(b"synthetic")
        self.opener = MagicMock()
        self.service = PhotoAlbumService(
            db_path=self.root / "photos.db", thumb_dir=self.root / "thumbs", opener=self.opener
        )

    def tearDown(self):
        self.temp.cleanup()

    def _assert_opened(self, res):
        self.assertTrue(res.get("ok"), res)
        self.opener.assert_called_once_with(str(self.path.resolve()))

    def test_application_service_dispatch(self):
        settings = self.root / "settings.json"
        settings.write_text("{}", encoding="utf-8")
        app = ApplicationService(settings)
        app._photo_album_service = self.service
        self._assert_opened(app.dispatch("photo_album", ["open", {"path": str(self.path)}]))

    def test_desktop_bridge(self):
        from desktop_web_ui import DesktopWebBridge, _BrowserApi

        bridge = DesktopWebBridge(MagicMock())
        bridge._photo_album_service = self.service
        self.assertTrue(callable(_BrowserApi(bridge).photo_album))
        self._assert_opened(bridge.photo_album("open", {"path": str(self.path)}))


if __name__ == "__main__":
    unittest.main()
