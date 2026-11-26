"""FILEOPS1: 문서뷰·사진뷰 파일 작업. 임시 폴더의 합성 파일만 쓴다.

휴지통·클립보드는 가짜를 쓴다. 실제 휴지통 시험은 DOCFIT_TEST_RECYCLE=1 일 때만 돈다
(합성 파일 하나가 사용자 휴지통에 남는다).
"""
from __future__ import annotations

import errno
import json
import os
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest.mock import patch

from docfit_app import file_ops, library_scan_cache
from docfit_app.application_service import ApplicationService
from docfit_app.doc_grid_service import DocGridService
from docfit_app.photo_album_service import PhotoAlbumService


class _Base(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name).resolve()
        self.source = self.root / "원본"
        self.dest = self.root / "대상"
        self.source.mkdir()
        self.dest.mkdir()
        self.data = self.root / "data"
        patcher = patch.object(library_scan_cache, "_data_root", lambda: self.data)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.recycled: list[Path] = []
        self.clipboard: list[list[Path]] = []
        self.photo = PhotoAlbumService(db_path=self.data / "photos.db", thumb_dir=self.data / "thumbs",
                                       drive_roots=[self.root])
        self.docs = DocGridService(thumb_dir=self.data / "doc-thumbs", drive_roots=[self.root])
        for service in (self.photo, self.docs):
            service.file_ops.recycler = self._recycle
            service.file_ops.clipboard = lambda paths: self.clipboard.append(list(paths))

    def tearDown(self):
        self.temp.cleanup()

    def _recycle(self, path: Path) -> None:
        self.recycled.append(path)
        path.unlink()

    def _file(self, name: str, content: bytes = b"synthetic", folder: Path | None = None) -> Path:
        path = (folder or self.source) / name
        path.write_bytes(content)
        return path

    def _run(self, service, op: str, paths, **extra) -> dict:
        started = service.command("file_op_start", {"op": op, "paths": [str(p) for p in paths], **extra})
        self.assertTrue(started["ok"], started)
        service.file_ops.wait(30)
        done = service.command("file_op_progress", {"task_id": started["task_id"]})
        self.assertEqual(done["state"], "complete")
        return done


class CopyMoveTests(_Base):
    def test_copy_never_overwrites_and_numbers_the_name(self):
        original = self._file("보고.hwpx", b"new")
        existing = self._file("보고.hwpx", b"old", self.dest)
        self._file("보고 (2).hwpx", b"old2", self.dest)
        done = self._run(self.docs, "copy", [original], destination=str(self.dest))
        result = done["results"][0]
        self.assertEqual(result["new_name"], "보고 (3).hwpx")
        self.assertEqual((self.dest / "보고 (3).hwpx").read_bytes(), b"new")
        self.assertEqual(existing.read_bytes(), b"old")
        self.assertEqual((self.dest / "보고 (2).hwpx").read_bytes(), b"old2")
        self.assertEqual(original.read_bytes(), b"new")

    def test_copy_into_same_folder_makes_a_second_file(self):
        original = self._file("a.pdf")
        done = self._run(self.docs, "copy", [original], destination=str(self.source))
        self.assertEqual(done["results"][0]["new_name"], "a (2).pdf")
        self.assertTrue(original.exists())

    def test_move_never_overwrites(self):
        original = self._file("a.jpg", b"new")
        existing = self._file("a.jpg", b"old", self.dest)
        done = self._run(self.photo, "move", [original], destination=str(self.dest))
        self.assertEqual(done["results"][0]["new_name"], "a (2).jpg")
        self.assertFalse(original.exists())
        self.assertEqual(existing.read_bytes(), b"old")
        self.assertEqual((self.dest / "a (2).jpg").read_bytes(), b"new")

    def test_exclusive_copy_and_rename_refuse_an_existing_target(self):
        # 이름을 고른 뒤 다른 프로그램이 같은 이름을 만든 경우.
        original = self._file("a.pdf", b"new")
        existing = self._file("a.pdf", b"old", self.dest)
        for call in (file_ops._copy_exclusive, file_ops._rename_exclusive):
            with self.subTest(call=call.__name__):
                with self.assertRaises(FileExistsError):
                    call(original, existing)
                self.assertEqual(existing.read_bytes(), b"old")
                self.assertEqual(original.read_bytes(), b"new")

    def test_move_to_same_folder_is_reported(self):
        original = self._file("a.jpg")
        done = self._run(self.photo, "move", [original], destination=str(self.source))
        self.assertEqual(done["results"][0]["code"], "SAME_FOLDER")
        self.assertTrue(original.exists())

    def test_cross_volume_move_copies_then_removes(self):
        original = self._file("a.pdf", b"body")
        with patch.object(file_ops, "_rename_exclusive", side_effect=OSError(errno.EXDEV, "cross-device")):
            done = self._run(self.docs, "move", [original], destination=str(self.dest))
        self.assertTrue(done["results"][0]["ok"], done)
        self.assertFalse(original.exists())
        self.assertEqual((self.dest / "a.pdf").read_bytes(), b"body")

    def test_cross_volume_move_rolls_back_when_source_cannot_be_removed(self):
        original = self._file("a.pdf", b"body")
        real_unlink = os.unlink

        def unlink(path, *args, **kwargs):
            if Path(path) == original:
                raise PermissionError(13, "in use")
            real_unlink(path, *args, **kwargs)

        with patch.object(file_ops, "_rename_exclusive", side_effect=OSError(errno.EXDEV, "cross-device")), \
                patch.object(file_ops.os, "unlink", unlink):
            done = self._run(self.docs, "move", [original], destination=str(self.dest))
        self.assertEqual(done["results"][0]["code"], "IO_ERROR")
        self.assertTrue(original.exists())
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_partial_failure_reports_each_file(self):
        good = self._file("a.pdf")
        other = self._file("a.exe")
        missing = self.source / "none.pdf"
        done = self._run(self.docs, "move", [good, other, missing, Path("relative.pdf")],
                         destination=str(self.dest))
        self.assertEqual([r["ok"] for r in done["results"]], [True, False, False, False])
        self.assertEqual([r.get("code") for r in done["results"][1:]],
                         ["UNSUPPORTED_TYPE", "FILE_NOT_FOUND", "INVALID_PATH"])
        self.assertEqual((done["succeeded"], done["failed"]), (1, 3))
        self.assertTrue(other.exists())

    def test_each_view_only_touches_its_own_extensions(self):
        photo = self._file("a.jpg")
        doc = self._file("a.hwp")
        self.assertEqual(self._run(self.docs, "delete", [photo])["results"][0]["code"], "UNSUPPORTED_TYPE")
        self.assertEqual(self._run(self.photo, "delete", [doc])["results"][0]["code"], "UNSUPPORTED_TYPE")
        self.assertEqual(self.recycled, [])

    def test_bad_requests_start_nothing(self):
        original = self._file("a.pdf")
        for payload, code in (
            ({"op": "shred", "paths": [str(original)]}, "INVALID_INPUT"),
            ({"op": "delete", "paths": []}, "INVALID_INPUT"),
            ({"op": "delete", "paths": str(original)}, "INVALID_INPUT"),
            ({"op": "rename", "paths": [str(original), str(original) + "x"], "new_name": "b"}, "INVALID_INPUT"),
            ({"op": "move", "paths": [str(original)]}, "INVALID_FOLDER"),
            ({"op": "copy", "paths": [str(original)], "destination": str(self.root / "none")}, "INVALID_FOLDER"),
        ):
            with self.subTest(payload=payload):
                self.assertEqual(self.docs.command("file_op_start", payload)["code"], code)
        self.assertTrue(original.exists())
        self.assertEqual(self.docs.command("file_op_progress", {"task_id": "x"})["code"], "NOT_FOUND")


class RenameDeleteClipboardTests(_Base):
    def test_rename_keeps_extension(self):
        original = self._file("a.HWPX")
        done = self._run(self.docs, "rename", [original], new_name="새 이름")
        self.assertEqual(done["results"][0]["new_name"], "새 이름.HWPX")
        again = self._run(self.docs, "rename", [self.source / "새 이름.HWPX"], new_name="b.hwpx")
        self.assertEqual(again["results"][0]["new_name"], "b.HWPX")
        other = self._run(self.docs, "rename", [self.source / "b.HWPX"], new_name="c.exe")
        self.assertEqual(other["results"][0]["new_name"], "c.exe.HWPX")

    def test_rename_rejects_separators_and_reserved_names(self):
        original = self._file("a.pdf")
        for name in ("", "  ", "..", "a/b", "a\\b", "..\\x", "a:b", "a*", "a?", 'a"', "a<", "a|", "a\x01",
                     "CON", "con", "NUL.x", "com1", "LPT9", "AUX ", "끝점.", "끝공백 ", "x" * 300):
            with self.subTest(name=name):
                done = self._run(self.docs, "rename", [original], new_name=name)
                self.assertEqual(done["results"][0]["code"], "INVALID_NAME", name)
                self.assertTrue(original.exists())

    def test_rename_does_not_overwrite_but_allows_case_change(self):
        original = self._file("a.pdf", b"a")
        other = self._file("b.pdf", b"b")
        done = self._run(self.docs, "rename", [original], new_name="b")
        self.assertEqual(done["results"][0]["code"], "NAME_EXISTS")
        self.assertEqual((original.read_bytes(), other.read_bytes()), (b"a", b"b"))
        done = self._run(self.docs, "rename", [original], new_name="A")
        self.assertTrue(done["results"][0]["ok"], done)
        self.assertIn("A.pdf", os.listdir(self.source))

    def test_delete_uses_the_recycler_only(self):
        paths = [self._file("a.pdf"), self._file("b.pdf")]
        done = self._run(self.docs, "delete", paths)
        self.assertEqual(done["succeeded"], 2)
        self.assertEqual(self.recycled, paths)

    def test_delete_failure_keeps_the_file_and_database(self):
        path = self._file("a.jpg")
        self.photo.add_tag(path, "행사")

        def refuse(_path):
            raise file_ops.FileOpError("RECYCLE_UNAVAILABLE", "휴지통 없음")

        self.photo.file_ops.recycler = refuse
        done = self._run(self.photo, "delete", [path])
        self.assertEqual(done["results"][0]["code"], "RECYCLE_UNAVAILABLE")
        self.assertTrue(path.exists())
        self.assertEqual(self.photo.get_tags(path)["tags"], ["행사"])

    @unittest.skipUnless(os.name == "nt", "Windows 휴지통")
    def test_network_path_is_never_deleted(self):
        with self.assertRaises(file_ops.FileOpError) as caught:
            file_ops.recycle_file(Path("//server/share/a.pdf"))
        self.assertEqual(caught.exception.code, "RECYCLE_UNAVAILABLE")

    @unittest.skipUnless(os.name == "nt" and os.environ.get("DOCFIT_TEST_RECYCLE") == "1",
                         "DOCFIT_TEST_RECYCLE=1 일 때만 실제 휴지통을 쓴다")
    def test_real_recycle_bin(self):
        path = self._file(f"fileops1-synthetic-{uuid.uuid4().hex}.pdf")
        file_ops.recycle_file(path)
        self.assertFalse(path.exists())

    def test_clipboard_receives_only_valid_files(self):
        good = self._file("a.pdf")
        done = self._run(self.docs, "clipboard", [good, self.source / "none.pdf"])
        self.assertEqual([r["ok"] for r in done["results"]], [True, False])
        self.assertEqual(self.clipboard, [[good]])

    @unittest.skipUnless(os.name == "nt", "CF_HDROP")
    def test_clipboard_payload_is_readable_by_the_shell(self):
        # 사용자 클립보드는 건드리지 않고, 만든 자료를 Windows 의 DragQueryFileW 로 되읽는다.
        import ctypes
        from ctypes import wintypes

        paths = [self.source / "가 나.pdf", self.source / "b.hwp"]
        payload = file_ops.hdrop_payload(paths)
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        shell32 = ctypes.WinDLL("shell32", use_last_error=True)
        kernel32.GlobalAlloc.restype = kernel32.GlobalLock.restype = ctypes.c_void_p
        kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
        for function in (kernel32.GlobalLock, kernel32.GlobalUnlock, kernel32.GlobalFree):
            function.argtypes = [ctypes.c_void_p]
        shell32.DragQueryFileW.argtypes = [ctypes.c_void_p, wintypes.UINT, ctypes.c_wchar_p, wintypes.UINT]
        handle = kernel32.GlobalAlloc(0x42, len(payload))
        try:
            ctypes.memmove(kernel32.GlobalLock(handle), payload, len(payload))
            kernel32.GlobalUnlock(handle)
            count = shell32.DragQueryFileW(handle, 0xFFFFFFFF, None, 0)
            names = []
            for index in range(count):
                buffer = ctypes.create_unicode_buffer(1024)
                shell32.DragQueryFileW(handle, index, buffer, 1024)
                names.append(buffer.value)
        finally:
            kernel32.GlobalFree(handle)
        self.assertEqual(names, [str(p) for p in paths])


class BookkeepingTests(_Base):
    def _photo(self, name: str = "a.jpg") -> Path:
        path = self._file(name)
        self.photo.db.upsert_photo(path=str(path), folder=str(path.parent), filename=name, size=9,
                                   mtime=path.stat().st_mtime, date_taken="2025-03-04 05:06:07")
        self.photo.add_tag(path, "행사")
        self.photo.toggle_favorite(path)
        return path

    def _thumb(self, service, path: Path) -> str:
        info = path.stat()
        key = service.cache.key(path, info.st_mtime_ns, info.st_size)
        service.cache.put(key, b"\xff\xd8\xffthumb")
        return key

    def _assert_kept(self, path: Path):
        row = self.photo.db.get_photo(str(path))
        self.assertEqual((row["tags"], row["favorite"], row["date_taken"], row["filename"], row["folder"]),
                         (["행사"], True, "2025-03-04 05:06:07", path.name, str(path.parent)))

    def test_move_and_rename_keep_photo_information_and_thumbnail(self):
        path = self._photo()
        old_key = self._thumb(self.photo, path)
        moved = self._run(self.photo, "move", [path], destination=str(self.dest))["results"][0]
        target = Path(moved["new_path"])
        self.assertTrue(moved["listed"])
        self._assert_kept(target)
        self.assertIsNone(self.photo.db.get_photo(str(path)))
        info = target.stat()
        self.assertIsNone(self.photo.cache.get(old_key))
        self.assertIsNotNone(self.photo.cache.get(
            self.photo.cache.key(target, info.st_mtime_ns, info.st_size)))
        renamed = self._run(self.photo, "rename", [target], new_name="행사 사진")["results"][0]
        self._assert_kept(Path(renamed["new_path"]))
        upper = self._run(self.photo, "rename", [renamed["new_path"]], new_name="행사 사진")["results"][0]
        self.assertEqual(upper["code"], "SAME_NAME")

    def test_case_only_rename_updates_the_row(self):
        path = self._photo("abc.jpg")
        done = self._run(self.photo, "rename", [path], new_name="ABC")["results"][0]
        self.assertTrue(done["ok"], done)
        names = [(row["filename"], row["tags"]) for row in self.photo.db.list_photos()]
        self.assertEqual(names, [("ABC.jpg", ["행사"])])

    def test_move_outside_the_listed_range_keeps_tags(self):
        outside = tempfile.TemporaryDirectory()
        self.addCleanup(outside.cleanup)
        path = self._photo()
        done = self._run(self.photo, "move", [path], destination=outside.name)["results"][0]
        self.assertFalse(done["listed"])
        self._assert_kept(Path(done["new_path"]).resolve())

    def test_copy_adds_a_row_without_tags_and_keeps_the_original(self):
        path = self._photo()
        done = self._run(self.photo, "copy", [path], destination=str(self.dest))["results"][0]
        self._assert_kept(path)
        row = self.photo.db.get_photo(done["new_path"])
        self.assertEqual((row["tags"], row["favorite"], row["date_taken"]), ([], False, "2025-03-04 05:06:07"))

    def test_delete_removes_row_tags_and_thumbnail(self):
        path = self._photo()
        key = self._thumb(self.photo, path)
        self._run(self.photo, "delete", [path])
        self.assertIsNone(self.photo.db.get_photo(str(path)))
        self.assertEqual(self.photo.db.get_tags(str(path)), [])
        self.assertIsNone(self.photo.cache.get(key))

    def test_list_by_date_follows_the_name_index_filter(self):
        path = self._photo()
        self.photo._default_name_paths = {os.path.normcase(str(path))}
        done = self._run(self.photo, "rename", [path], new_name="b")["results"][0]
        listed = [item["path"] for group in self.photo.list_by_date() for item in group["items"]]
        self.assertEqual(listed, [done["new_path"]])

    def test_only_succeeded_files_change_the_saved_lists(self):
        moved = self._file("a.pdf")
        deleted = self._file("b.pdf")
        kept = self._file("c.pdf")
        self.docs.db.import_rows([{"path": str(p), "name": p.name} for p in (moved, deleted, kept)])
        self._run(self.docs, "rename", [moved], new_name="d")
        self._run(self.docs, "delete", [deleted, self.source / "none.pdf"])
        saved = self.docs.db.load()
        self.assertEqual([row[2] for row in saved], ["c.pdf", "d.pdf"])
        self.assertEqual(saved[1][1], str(self.source / "d.pdf"))

    def test_saved_photo_list_follows_a_move(self):
        path = self._photo()
        self.photo.add_tag(path, "행사")
        done = self._run(self.photo, "move", [path], destination=str(self.dest))["results"][0]
        self.assertEqual([(r["path"], r["folder"], r["tags"]) for r in self.photo.db.list_photos()],
                         [(done["new_path"], str(self.dest), ["행사"])])

    def test_bookkeeping_error_is_a_warning_not_a_failed_file(self):
        path = self._photo()
        with patch.object(self.photo.db, "transfer_photo", side_effect=RuntimeError("db")):
            done = self._run(self.photo, "move", [path], destination=str(self.dest))["results"][0]
        self.assertTrue(done["ok"])
        self.assertIn("warning", done)


class DispatchTests(_Base):
    def test_both_views_through_application_service_and_desktop_bridge(self):
        import desktop_web_ui

        app = ApplicationService(self.root / "settings.json")
        app._photo_album_service = self.photo
        app._doc_grid_service = self.docs
        bridge = desktop_web_ui.DesktopWebBridge.__new__(desktop_web_ui.DesktopWebBridge)
        bridge._photo_album_service = self.photo
        bridge._doc_grid_service = self.docs
        callers = {
            "application": lambda method, *args: app.dispatch(method, list(args)),
            "desktop": lambda method, *args: getattr(bridge, method)(*args),
        }
        for host, call in callers.items():
            for method, suffix, service in (("photo_album", ".jpg", self.photo), ("doc_grid", ".pdf", self.docs)):
                with self.subTest(host=host, method=method):
                    path = self._file(f"{host}{suffix}")
                    started = call(method, "file_op_start",
                                   {"op": "copy", "paths": [str(path)], "destination": str(self.dest)})
                    self.assertTrue(started["ok"], started)
                    service.file_ops.wait(30)
                    done = call(method, "file_op_progress", {"task_id": started["task_id"]})
                    self.assertEqual(done["succeeded"], 1, done)
                    json.dumps(done)
                    self.assertTrue((self.dest / path.name).exists())

    def test_pick_destination_uses_the_folder_dialog(self):
        with patch("docfit_app.native_file_dialog_service.NativeFileDialogService.pick_directory",
                   side_effect=[str(self.dest), None]) as picker:
            self.assertEqual(self.docs.command("pick_destination", {}), {"ok": True, "path": str(self.dest)})
            self.assertEqual(self.photo.command("pick_destination", {}), {"ok": True, "cancelled": True})
        self.assertEqual(picker.call_args.args, ("file_ops_destination",))


if __name__ == "__main__":
    unittest.main()
