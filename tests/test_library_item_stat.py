"""자료함 '탐색기 형식' 보기의 크기·수정일 조회(item_stat) 시험. 저장된 경로만 읽는지 확인한다."""

import os
import tempfile
import unittest

from docfit_app.library_bridge import library_item_stats


class ItemStatTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.doc = os.path.join(self.tmp.name, "보고서.hwpx")
        with open(self.doc, "wb") as f:
            f.write(b"12345")
        self.stored = {
            1: {"ok": True, "id": 1, "kind": "file", "source_path": self.doc},
            2: {"ok": True, "id": 2, "kind": "folder", "source_path": self.tmp.name},
            3: {"ok": True, "id": 3, "kind": "file", "source_path": os.path.join(self.tmp.name, "없음.pdf")},
            4: {"ok": True, "id": 4, "kind": "link", "source_path": "https://example.com/a"},
            5: {"ok": True, "id": 5, "kind": "text", "source_path": ""},
        }
        self.requests = []

    def _request(self, request):
        self.requests.append(request)
        return self.stored.get(request["id"], {"ok": False, "message": "없음"})

    def test_reads_stored_paths(self):
        stats = library_item_stats(self._request, [1, "2", 3, 4, 5, 99, "x"])
        self.assertEqual(stats["1"]["size"], 5)
        self.assertTrue(stats["1"]["exists"])
        self.assertGreater(stats["1"]["mtime"], 0)
        self.assertTrue(stats["2"]["is_dir"])
        self.assertIsNone(stats["2"]["size"])
        self.assertFalse(stats["3"]["exists"])
        self.assertIsNone(stats["4"]["exists"])
        self.assertIsNone(stats["5"]["exists"])
        self.assertFalse(stats["99"]["exists"])
        self.assertNotIn("x", stats)
        self.assertEqual([r["cmd"] for r in self.requests], ["item_get"] * 6)

    def test_limit(self):
        stats = library_item_stats(self._request, list(range(1, 6)), limit=2)
        self.assertEqual(sorted(stats), ["1", "2"])


class DesktopItemStatTests(unittest.TestCase):
    def test_bridge_command(self):
        from desktop_web_ui import DesktopWebBridge

        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        doc = os.path.join(tmp.name, "a.txt")
        with open(doc, "wb") as f:
            f.write(b"abc")

        class FakeLibrary:
            def request(self, request):
                return {"ok": True, "id": request["id"], "kind": "file", "source_path": doc}

        bridge = DesktopWebBridge.__new__(DesktopWebBridge)
        bridge._library_bridge = FakeLibrary()
        bridge._developer_mode_enabled = lambda: True
        # 화면이 보낸 경로(source_path)는 무시하고 저장된 경로만 읽는다.
        result = bridge.library({"cmd": "item_stat", "item_ids": [7], "source_path": "C:/무시.exe"})
        self.assertTrue(result["ok"])
        self.assertEqual(result["items"]["7"]["size"], 3)
        self.assertFalse(bridge.library({"cmd": "item_stat", "item_ids": "7"})["ok"])
        bridge._developer_mode_enabled = lambda: False


if __name__ == "__main__":
    unittest.main()
