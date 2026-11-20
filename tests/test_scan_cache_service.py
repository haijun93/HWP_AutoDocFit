import io
import json
import os
import struct
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app import library_scan_cache
from docfit_app import rust_scan
from docfit_app.background_scan import BackgroundScan


class _Pipe:
    def __init__(self, state="ready", mode="service"):
        self.state = state
        self.mode = mode
        self.response = io.BytesIO()

    def __enter__(self): return self
    def __exit__(self, *_): return False

    def write(self, frame):
        size = struct.unpack("<I", frame[:4])[0]
        req = json.loads(frame[4:4 + size])
        if req["command"] == "names_status":
            result = {"state": self.state, "mode": self.mode}
        else:
            result = {"state": "ready", "next": 2, "done": True, "items": [
                {"path": r"C:\\Docs\\one.pdf", "name": "one.pdf"},
                {"path": r"C:\\Docs\\two.jpg", "name": "two.jpg", "size": 12, "mtime": 1.0, "cloud": False},
            ]}
        body = json.dumps({"ok": True, "result": result}).encode()
        self.response = io.BytesIO(struct.pack("<I", len(body)) + body)
        return len(frame)

    def flush(self): pass
    def read(self, n=-1): return self.response.read(n)


class ScanCacheServiceTests(unittest.TestCase):
    def test_names_pipe_ready_returns_filtered_rows_and_optional_metadata(self):
        pipe = _Pipe()
        with patch("docfit_app.rust_scan.os.name", "nt"):
            rows = rust_scan.names_query({".pdf"}, opener=lambda _: pipe)
        self.assertEqual([r["name"] for r in rows], ["one.pdf"])
        self.assertNotIn("size", rows[0])

    def test_default_scan_selects_service_engine(self):
        with patch("docfit_app.rust_scan.service_names_query", return_value=[
                {"path": "C:/Docs/a.pdf", "name": "a.pdf"}]) as service, \
             patch("docfit_app.rust_scan.scan_default_roots") as klib:
            rows, engine = rust_scan.scan_default({"pdf"})
        self.assertEqual(engine, "service")
        self.assertEqual(rows[0]["path"], "C:/Docs/a.pdf")
        self.assertEqual(rows[0]["size"], 0)
        service.assert_called_once()
        klib.assert_not_called()

    def test_names_pipe_building_and_missing_fall_back(self):
        with patch("docfit_app.rust_scan.os.name", "nt"):
            self.assertIsNone(rust_scan.names_query({"pdf"}, opener=lambda _: _Pipe("building")))
            self.assertIsNone(rust_scan.names_query({"pdf"}, opener=lambda _: _Pipe("ready", "user")))
            self.assertIsNone(rust_scan.names_query({"pdf"}, opener=lambda _: (_ for _ in ()).throw(FileNotFoundError())))
        with patch("docfit_app.rust_scan.service_names_query", return_value=None), \
             patch("docfit_app.rust_scan.scan_default_roots", return_value=[{"path":"x"}]) as klib:
            rows, engine = rust_scan.scan_default({"pdf"}, executable=Path("missing"))
        self.assertEqual(engine, "klib")
        klib.assert_called_once()

    def test_python_override_and_missing_klib_engine(self):
        with patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE":"python"}), \
             patch("docfit_app.rust_scan.service_names_query") as service, \
             patch("docfit_app.rust_scan.scan_default_roots") as klib:
            rows, engine = rust_scan.scan_default({"pdf"})
        self.assertIsNone(rows)
        self.assertEqual(engine, "python")
        service.assert_not_called()
        klib.assert_not_called()
        with patch.dict(os.environ, {}, clear=True), patch("docfit_app.rust_scan.service_names_query", return_value=None), \
             patch("docfit_app.rust_scan.scan_default_roots", return_value=None):
            rows, engine = rust_scan.scan_default({"pdf"}, executable=Path("missing"))
        self.assertIsNone(rows)
        self.assertEqual(engine, "python")

    def test_cache_atomic_roundtrip_and_settings_invalidation(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            items = [{"path":"C:/Docs/a.pdf", "name":"a.pdf"}]
            library_scan_cache.save("documents", {"pdf"}, [], items, root=root, roots=["C:/"])
            self.assertEqual(library_scan_cache.load("documents", {"pdf"}, [], root=root, roots=["C:/"]), items)
            self.assertIsNone(library_scan_cache.load("documents", {"pdf"}, ["C:/skip"], root=root, roots=["C:/"]))
            self.assertFalse(list((root / "scan-cache").glob("*.tmp")))
            (root / "scan-cache" / "documents.json").write_text("[]", encoding="utf-8")
            self.assertIsNone(library_scan_cache.load("documents", {"pdf"}, [], root=root, roots=["C:/"]))

    def test_cached_start_is_immediate_and_background_replaces_engine(self):
        scan = BackgroundScan()
        started = scan.start(lambda report, cancel: {"engine":"service", "documents":[{"path":"new"}]},
                             initial_result={"engine":"cache", "documents":[{"path":"old"}]}, engine="cache")
        self.assertEqual(started["state"], "cached")
        self.assertEqual(started["result"]["documents"][0]["path"], "old")
        state = scan.progress()
        for _ in range(100):
            state = scan.progress()
            if state["state"] == "complete": break
        self.assertEqual(state["state"], "complete")
        self.assertEqual(state["engine"], "service")
        self.assertEqual(state["result"]["documents"][0]["path"], "new")


if __name__ == "__main__":
    unittest.main()
