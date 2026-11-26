"""문서뷰·사진뷰 검색의 부분 결과 전달(SCANSPEED1) 시험."""
import os
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app import background_scan, library_scan_cache
from docfit_app.background_scan import BackgroundScan
from docfit_app.doc_grid_service import DocGridService
from docfit_app.photo_album_service import PhotoAlbumService
from docfit_app.rust_scan import find_klib_executable, scan_default_roots


def row_for(path: Path) -> dict:
    st = path.stat()
    return {"path": str(path), "name": path.name, "ext": path.suffix[1:].lower(), "size": st.st_size,
            "mtime": st.st_mtime, "mtime_ns": st.st_mtime_ns, "cloud": False}


def wait_until(predicate, seconds: float = 5.0) -> bool:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.005)
    return False


def drain(command, since: int = 0):
    """검색이 끝날 때까지 새 행만 이어 받는다."""
    rows: list[dict] = []
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        state = command("scan_progress", {"since": since})
        rows.extend(state["rows"])
        since = state["next"]
        if state["state"] not in ("running", "cancelling") and since >= state["total"]:
            return rows, state
        time.sleep(0.005)
    raise AssertionError("검색이 끝나지 않았습니다.")


class BackgroundScanStreamTests(unittest.TestCase):
    def test_progress_since_returns_only_new_rows_in_bounded_pages(self):
        scan = BackgroundScan()
        first_batch, release = threading.Event(), threading.Event()

        def worker(report, cancel):
            for index in range(5):
                report.publish({"n": index})
            first_batch.set()
            release.wait(5)
            report.publish({"n": 5})
            return {"documents": [{"n": n} for n in range(6)], "engine": "klib"}

        with patch.object(background_scan, "PAGE_ROWS", 3):
            scan.start(worker, streamed_key="documents")
            self.assertTrue(first_batch.wait(5))
            page = scan.progress(0)
            self.assertEqual([row["n"] for row in page["rows"]], [0, 1, 2])
            self.assertEqual((page["next"], page["total"], page["state"]), (3, 5, "running"))
            page = scan.progress(page["next"])
            self.assertEqual([row["n"] for row in page["rows"]], [3, 4])
            self.assertEqual(scan.progress(5)["rows"], [])
            release.set()
            self.assertTrue(wait_until(lambda: scan.progress()["state"] == "complete"))
            done = scan.progress(5)
        self.assertEqual([row["n"] for row in done["rows"]], [5])
        self.assertNotIn("documents", done["result"])
        # since 없이 부르면 전체 결과를 그대로 준다.
        self.assertEqual(len(scan.progress()["result"]["documents"]), 6)
        self.assertNotIn("rows", scan.progress())

    def test_new_scan_starts_with_empty_rows(self):
        scan = BackgroundScan()
        scan.start(lambda report, cancel: report.publish({"n": 1}) or {})
        self.assertTrue(wait_until(lambda: scan.progress()["state"] == "complete"))
        scan.start(lambda report, cancel: {})
        self.assertTrue(wait_until(lambda: scan.progress()["state"] == "complete"))
        self.assertEqual(scan.progress(0)["total"], 0)


class DocumentStreamTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name).resolve()
        self.root = self.base / "drive"
        self.root.mkdir()
        patcher = patch.object(library_scan_cache, "_data_root", lambda: self.base / "data")
        patcher.start()
        self.addCleanup(patcher.stop)
        env = patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE": ""})
        env.start()
        self.addCleanup(env.stop)
        self.service = DocGridService(thumb_dir=self.base / "thumbs", drive_roots=[self.root])

    def test_rows_arrive_before_the_scan_finishes(self):
        paths = [self.root / f"{name}.pdf" for name in ("b", "a")]
        for path in paths:
            path.write_bytes(b"pdf")
        release = threading.Event()

        def slow_klib(extensions, excluded, *, on_row=None, **kwargs):
            rows = [row_for(path) for path in paths]
            on_row(rows[0])
            on_row(rows[0])  # 겹치는 루트에서 같은 파일이 두 번 와도 한 번만 전달한다.
            release.wait(5)
            on_row(rows[1])
            return rows

        with patch("docfit_app.doc_grid_service.query_names_service", return_value=None), \
             patch("docfit_app.doc_grid_service._scan_default_roots", side_effect=slow_klib), \
             patch.object(self.service.cache, "key", side_effect=AssertionError("검색 중 키 계산")):
            self.service.command("list_default")
            self.assertTrue(wait_until(lambda: self.service.command("scan_progress", {"since": 0})["total"] == 1))
            partial = self.service.command("scan_progress", {"since": 0})
            self.assertEqual(partial["state"], "running")
            self.assertEqual([row["name"] for row in partial["rows"]], ["b.pdf"])
            release.set()
            rows, state = drain(self.service.command, partial["next"])
        self.assertEqual([row["name"] for row in rows], ["a.pdf"])
        self.assertEqual(state["state"], "complete")
        full = self.service.command("scan_progress")["result"]["documents"]
        self.assertEqual([row["name"] for row in full], ["a.pdf", "b.pdf"])

    def test_python_walk_streams_the_same_rows_as_the_final_list(self):
        (self.root / "sub").mkdir()
        for name in ("one.pdf", "sub/two.hwpx", "skip.txt"):
            (self.root / name).write_bytes(b"x")
        with patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE": "python"}):
            self.service.command("list_default")
            rows, state = drain(self.service.command)
        self.assertEqual(state["state"], "complete")
        self.assertEqual(sorted(row["name"] for row in rows), ["one.pdf", "two.hwpx"])


class KlibDefaultScanTests(unittest.TestCase):
    def test_reserved_and_user_folders_stay_excluded_without_extra_arguments(self):
        klib = find_klib_executable()
        if klib is None:
            self.skipTest("klib 실행 파일 없음")
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE": ""}):
            root = Path(temp).resolve()
            for folder in ("docs", "node_modules", "docs/.venv", "private"):
                (root / folder).mkdir(parents=True, exist_ok=True)
                (root / folder / "a.pdf").write_bytes(b"pdf")
            streamed = []
            rows = scan_default_roots({".pdf"}, [str(root / "private")], executable=klib, roots=[root],
                                      on_row=streamed.append)
            self.assertEqual([os.path.relpath(row["path"], root) for row in rows], [os.path.join("docs", "a.pdf")])
            self.assertEqual(streamed, rows)


class PhotoStreamTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name).resolve()
        self.root = self.base / "drive"
        self.root.mkdir()
        patcher = patch.object(library_scan_cache, "_data_root", lambda: self.base / "data")
        patcher.start()
        self.addCleanup(patcher.stop)
        env = patch.dict(os.environ, {"DOCFIT_SCAN_ENGINE": ""})
        env.start()
        self.addCleanup(env.stop)
        self.paths = [self.root / f"{index}.jpg" for index in range(3)]
        for path in self.paths:
            path.write_bytes(b"not-an-image")
        # 루트 표기의 대소문자가 실제와 달라도(Windows 기본 루트는 소문자) 같은 사진으로 본다.
        shown = Path(os.path.normcase(str(self.root)))
        self.service = PhotoAlbumService(db_path=self.base / "photos.sqlite", thumb_dir=self.base / "thumbs",
                                         drive_roots=[shown])
        self.rows = [row_for(shown / path.name) for path in self.paths]

    def fake_klib(self, release: threading.Event | None = None):
        def run(extensions, excluded, *, on_row=None, **kwargs):
            on_row(self.rows[0])
            if release is not None:
                release.wait(5)
            for row in self.rows[1:]:
                on_row(row)
            return list(self.rows)
        return run

    def scan(self, release=None):
        with patch("docfit_app.doc_grid_service.query_names_service", return_value=None), \
             patch("docfit_app.photo_album_service._scan_default_roots", side_effect=self.fake_klib(release)):
            self.assertTrue(self.service.command("scan_default")["ok"])
            return drain(self.service.command)

    def test_items_are_shown_before_database_and_exif_work(self):
        release = threading.Event()
        with patch("docfit_app.doc_grid_service.query_names_service", return_value=None), \
             patch("docfit_app.photo_album_service._scan_default_roots", side_effect=self.fake_klib(release)), \
             patch("docfit_app.photo_album_service.read_exif_metadata", return_value=(None, 1)) as exif:
            self.service.command("scan_default")
            self.assertTrue(wait_until(lambda: self.service.command("scan_progress", {"since": 0})["total"] == 1))
            partial = self.service.command("scan_progress", {"since": 0})
            self.assertEqual(exif.call_count, 0)
            item = partial["rows"][0]
            self.assertEqual((item["filename"], item["favorite"], item["tags"]), ("0.jpg", False, []))
            self.assertRegex(item["date_taken"], r"^\d{4}-\d\d-\d\d ")
            release.set()
            rows, state = drain(self.service.command, partial["next"])
        self.assertEqual(state["state"], "complete")
        self.assertEqual(len(rows), 2)
        self.assertEqual(state["result"]["added"], 3)
        listed = [p["path"] for g in self.service.command("list_by_date")["groups"] for p in g["items"]]
        self.assertEqual(sorted(listed), sorted([item["path"], *(row["path"] for row in rows)]))

    def test_second_scan_does_not_reread_unchanged_photos_and_keeps_tags(self):
        rows, _ = self.scan()
        self.service.command("add_tag", {"path": rows[0]["path"], "tag": "현장"})
        with patch("docfit_app.photo_album_service.read_exif_metadata",
                   side_effect=AssertionError("바뀌지 않은 사진을 다시 읽음")), \
             patch.object(library_scan_cache, "load", return_value=None):
            again, state = self.scan()
        self.assertEqual(state["state"], "complete", state)
        self.assertEqual((state["result"]["scanned"], state["result"]["added"], state["result"]["updated"]), (3, 0, 0))
        self.assertEqual([row["tags"] for row in again if row["tags"]], [["현장"]])

    def test_list_and_search_skip_per_file_stat_and_hide_deleted(self):
        self.scan()
        self.paths[2].unlink()
        with patch.object(self.service.cache, "key", side_effect=AssertionError("목록에서 키 계산")):
            listed = [p["name"] for g in self.service.command("list_by_date")["groups"] for p in g["items"]]
            found = self.service.command("search", {"query": "1.jpg"})["results"]
        self.assertEqual(sorted(listed), ["0.jpg", "1.jpg"])
        self.assertEqual([item["name"] for item in found], ["1.jpg"])


if __name__ == "__main__":
    unittest.main()
