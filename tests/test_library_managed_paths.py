"""복사본 경로를 사용하고 이전 실패 때 원본으로 돌아가지 않는지 검사한다."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

from docfit_app.library_bridge import library_item_stats, open_library_item
from docfit_app.notebook_service import NotebookService


class ManagedPathTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.origin = Path(self.tmp.name) / "original.pdf"
        self.managed = Path(self.tmp.name) / "copy.pdf"
        self.origin.write_bytes(b"original")
        self.managed.write_bytes(b"copy edited")

    def test_opener_and_stats_prefer_copy_even_with_stale_source(self):
        item = {"ok": True, "id": 3, "kind": "file", "source_path": str(self.origin),
                "origin_path": str(self.origin), "managed_path": str(self.managed)}
        opened = []
        open_library_item(item, opener=opened.append)
        self.assertEqual(opened, [str(self.managed)])
        stats = library_item_stats(lambda request: item, [3])
        self.assertEqual(stats["3"]["size"], len(b"copy edited"))
        self.assertEqual(self.origin.read_bytes(), b"original")

    def test_registered_open_prepares_and_uses_only_copy(self):
        from desktop_web_ui import DesktopWebBridge

        host = DesktopWebBridge.__new__(DesktopWebBridge)
        host._library_bridge = Mock()
        host._library_bridge.request.return_value = {
            "ok": True, "kind": "file", "source_path": str(self.origin),
            "managed_path": str(self.managed), "origin_path": str(self.origin)}
        host._library_opener = Mock()
        result = host.library({"cmd": "open_item", "item_id": 3, "source_path": str(self.origin)})
        self.assertTrue(result["ok"])
        host._library_bridge.request.assert_called_once_with({"cmd": "item_prepare", "id": 3})
        host._library_opener.assert_called_once_with(str(self.managed))

    def test_registered_open_never_falls_back_to_origin(self):
        from desktop_web_ui import DesktopWebBridge

        host = DesktopWebBridge.__new__(DesktopWebBridge)
        host._library_bridge = Mock()
        host._library_opener = Mock()
        host._library_bridge.request.return_value = {
            "ok": True, "kind": "file", "source_path": str(self.origin), "managed_path": ""}
        result = host.library({"cmd": "open_item", "item_id": 3})
        self.assertEqual(result["code"], "MANAGED_MISSING")
        host._library_opener.assert_not_called()
        host._library_bridge.request.return_value = {
            "ok": False, "code": "SOURCE_MISSING", "message": "원본 없음"}
        self.assertEqual(host.library({"cmd": "open_item", "item_id": 3})["code"], "SOURCE_MISSING")
        host._library_opener.assert_not_called()

    def test_notebook_python_extractor_reads_copy_only(self):
        bridge = Mock()
        bridge.request.side_effect = [
            {"ok": True, "kind": "file", "source_path": str(self.origin), "managed_path": str(self.managed)},
            {"ok": True, "id": 9}]
        extractor = Mock(return_value=[{"text": "본문", "level": 0}])
        result = NotebookService(bridge, extractor=extractor).handle({
            "cmd": "nb_source_from_item", "notebook_id": 1, "item_id": 3})
        self.assertTrue(result["ok"])
        extractor.assert_called_once_with(str(self.managed.resolve()))
        self.assertEqual(bridge.request.call_args_list[0].args[0], {"cmd": "item_prepare", "id": 3})
        self.assertEqual(self.origin.read_bytes(), b"original")

    def test_notebook_preparation_failure_never_extracts_origin(self):
        bridge = Mock()
        bridge.request.return_value = {"ok": False, "code": "SOURCE_MISSING", "message": "원본 없음"}
        extractor = Mock()
        result = NotebookService(bridge, extractor=extractor).handle({
            "cmd": "nb_source_from_item", "notebook_id": 1, "item_id": 3})
        self.assertFalse(result["ok"])
        extractor.assert_not_called()
        self.assertEqual(bridge.request.call_count, 1)
