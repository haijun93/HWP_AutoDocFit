"""기본 라이브러리 백그라운드 스캔 API 회귀 시험."""
import tempfile
import time
import unittest
import os
import threading
from pathlib import Path

from docfit_app.doc_grid_service import DocGridService
from docfit_app.photo_album_service import PhotoAlbumService
from docfit_app.rust_scan import scan as rust_scan


class DefaultLibraryScanTests(unittest.TestCase):
    def make_service(self, root: Path) -> DocGridService:
        return DocGridService(thumb_dir=root / "thumb-cache", drive_roots=[root])

    def test_default_scan_reports_monotonic_progress_and_prunes_excluded_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "drive"
            (root / "docs").mkdir(parents=True)
            (root / "node_modules" / "nested").mkdir(parents=True)
            (root / "docs" / "visible.pdf").write_bytes(b"pdf")
            (root / "node_modules" / "nested" / "hidden.pdf").write_bytes(b"pdf")
            service = self.make_service(root)

            started = service.command("list_default")
            self.assertTrue(started["ok"])
            observed = []
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                state = service.command("scan_progress")
                observed.append(state.get("visited", 0))
                if state["state"] == "complete":
                    break
                time.sleep(0.002)
            self.assertEqual(state["state"], "complete")
            self.assertEqual(observed, sorted(observed))
            self.assertEqual([item["name"] for item in state["result"]["documents"]], ["visible.pdf"])

    def test_scan_cancel_stops_traversal_and_progress(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "drive"
            bulk = root / "bulk"
            bulk.mkdir(parents=True)
            for index in range(4000):
                (bulk / f"{index:05}.pdf").touch()
            service = self.make_service(root)

            self.assertTrue(service.command("list_default")["ok"])
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                state = service.command("scan_progress")
                if state["visited"] >= 5:
                    service.command("scan_cancel")
                    break
                time.sleep(0.001)
            else:
                self.fail("스캔이 진행 상태에 도달하지 않았습니다.")

            while time.monotonic() < deadline and state["state"] not in ("cancelled", "complete"):
                time.sleep(0.002)
                state = service.command("scan_progress")
            self.assertEqual(state["state"], "cancelled")
            stopped_at = state["visited"]
            time.sleep(0.02)
            self.assertEqual(service.command("scan_progress")["visited"], stopped_at)

    def test_photo_default_scan_uses_injected_roots_and_background_api(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "drive"
            (root / "photos").mkdir(parents=True)
            (root / "node_modules").mkdir()
            (root / "photos" / "visible.jpg").write_bytes(b"not-an-image")
            (root / "node_modules" / "hidden.jpg").write_bytes(b"not-an-image")
            service = PhotoAlbumService(
                db_path=Path(temp) / "photos.sqlite",
                thumb_dir=Path(temp) / "thumb-cache",
                drive_roots=[root],
            )
            started = service.command("scan_default")
            self.assertTrue(started["ok"])
            deadline = time.monotonic() + 5
            observed = []
            while time.monotonic() < deadline:
                state = service.command("scan_progress")
                observed.append(state.get("visited", 0))
                if state["state"] == "complete":
                    break
                time.sleep(0.002)
            self.assertEqual(state["state"], "complete")
            self.assertEqual(observed, sorted(observed))
            self.assertEqual(state["result"]["scanned"], 1)

    def test_rust_scan_cancel_terminates_child_process(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary = Path(os.environ.get("KLIB_EXE", Path(__file__).resolve().parents[1] / "klib" / "target" / "release" / "klib.exe"))
            if not binary.is_file():
                self.skipTest(f"klib binary is unavailable: {binary}")
            for index in range(1000):
                (root / f"{index:04}.pdf").touch()
            cancel = threading.Event()
            def stop_after_ten(visited, _folder):
                if visited >= 10:
                    cancel.set()
            started = time.monotonic()
            rows = rust_scan(root, {".pdf"}, executable=binary, cancel_event=cancel, on_progress=stop_after_ten)
            self.assertIsNotNone(rows)
            self.assertTrue(cancel.is_set())
            self.assertLess(len(rows), 1000)
            self.assertLess(time.monotonic() - started, 3)


if __name__ == "__main__":
    unittest.main()
