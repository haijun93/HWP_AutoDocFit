"""PHOTOFIX1: 사진뷰 서비스의 설정 저장소 연결. 임시 폴더의 합성 파일·설정만 쓴다."""
from __future__ import annotations

import os
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import desktop_web_ui
from docfit_app.application_service import ApplicationService
from docfit_app.settings_service import SettingsService


class PhotoAlbumSettingsTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.source = self.root / "원본"
        self.private = self.root / "제외"
        self.source.mkdir()
        self.private.mkdir()
        self.settings_path = self.root / "settings.json"
        # DB·썸네일·저장된 목록이 실제 사용자 폴더 대신 임시 폴더를 쓰게 한다.
        env = patch.dict(os.environ, {"KAGENT_DATA": str(self.root / "data"), "DOCFIT_SCAN_ENGINE": "python"})
        env.start()
        self.addCleanup(env.stop)

    # 호출할 때마다 새 브리지·새 서비스를 만든다(앱을 다시 켠 것과 같다).
    def _desktop(self):
        bridge = desktop_web_ui.DesktopWebBridge.__new__(desktop_web_ui.DesktopWebBridge)
        store = SettingsService(self.settings_path)
        bridge._settings_store = lambda: store
        return (lambda action, payload=None: bridge.photo_album(action, payload or {})), bridge

    def _application(self):
        app = ApplicationService(self.settings_path)
        return (lambda action, payload=None: app.dispatch("photo_album", [action, payload or {}])), app

    def _hosts(self):
        return (("desktop", self._desktop), ("application", self._application))

    def test_thumb_size_and_excluded_folder_survive_a_new_service(self):
        for host, make in self._hosts():
            with self.subTest(host=host):
                self.settings_path.unlink(missing_ok=True)
                first, _ = make()
                saved = first("thumb_size_set", {"size": 215})
                self.assertTrue(saved["ok"], saved)
                added = first("excluded_folder_add", {"folder": str(self.private)})
                self.assertTrue(added["ok"], added)

                second, _ = make()
                self.assertEqual(second("thumb_size_get")["size"], 215.0)
                self.assertEqual(second("excluded_folders_get")["folders"], [str(self.private)])

                removed = second("excluded_folder_remove", {"folder": str(self.private)})
                self.assertEqual(removed["folders"], [])
                self.assertEqual(make()[0]("excluded_folders_get")["folders"], [])

    def _listed(self, call, action="list_by_date"):
        response = call(action)
        self.assertTrue(response["ok"], response)
        if action == "search":
            return {item["name"] for item in response["results"]}
        return {item["name"] for group in response["groups"] for item in group["items"]}

    def test_photo_moved_into_excluded_folder_leaves_the_list(self):
        for host, make in self._hosts():
            with self.subTest(host=host):
                self.settings_path.unlink(missing_ok=True)
                moved = self.source / f"{host}-이동.jpg"
                kept = self.source / f"{host}-유지.jpg"
                moved.write_bytes(b"synthetic")
                kept.write_bytes(b"synthetic")
                call, owner = make()
                self.assertTrue(call("scan", {"folder": str(self.source)})["ok"])
                self.assertTrue(call("toggle_favorite", {"path": str(moved)})["favorite"])
                self.assertTrue(call("excluded_folder_add", {"folder": str(self.private)})["ok"])

                # 위 scan 호출로 만들어진 서비스의 기본 목록 범위를 임시 폴더로 바꾼다.
                service = owner._photo_album_service
                service._drive_roots = (self.root,)
                started = call("file_op_start",
                               {"op": "move", "paths": [str(moved)], "destination": str(self.private)})
                self.assertTrue(started["ok"], started)
                service.file_ops.wait(30)
                done = call("file_op_progress", {"task_id": started["task_id"]})
                self.assertEqual(done["succeeded"], 1, done)
                self.assertFalse(done["results"][0]["listed"], done)

                for reopened in (call, make()[0]):
                    for action in ("list_by_date", "search"):
                        names = self._listed(reopened, action)
                        self.assertNotIn(moved.name, names, action)
                        self.assertIn(kept.name, names, action)

                # 제외를 풀면 태그·즐겨찾기 기록과 함께 다시 보인다.
                call("excluded_folder_remove", {"folder": str(self.private)})
                restored = {item["name"]: item for group in call("list_by_date")["groups"] for item in group["items"]}
                self.assertTrue(restored[moved.name]["favorite"])

    def test_already_scanned_photos_are_hidden_after_excluding_their_folder(self):
        hidden = self.private / "숨김.jpg"
        shown = self.source / "보임.jpg"
        hidden.write_bytes(b"synthetic-a")
        shown.write_bytes(b"synthetic-b")
        call, _ = self._desktop()
        for folder in (self.source, self.private):
            call("scan", {"folder": str(folder)})
        call("add_tag", {"path": str(hidden), "tag": "비공개"})
        call("add_tag", {"path": str(shown), "tag": "공개"})
        call("excluded_folder_add", {"folder": str(self.private)})

        self.assertEqual(self._listed(call), {shown.name})
        self.assertEqual(self._listed(call, "search"), {shown.name})
        self.assertEqual([item["tag"] for item in call("list_tags")["tags"]], ["공개"])
        places = call("place_groups", {"group_by": "folder"})
        self.assertEqual({item["name"] for group in places["groups"] for item in group["items"]}, {shown.name})
        duplicates = call("find_duplicates")
        self.assertEqual(duplicates["scanned"] + duplicates["skipped_missing"], 1, duplicates)

    def test_default_scan_saved_list_omits_excluded_photos(self):
        hidden = self.private / "숨김.jpg"
        shown = self.source / "보임.jpg"
        hidden.write_bytes(b"synthetic-a")
        shown.write_bytes(b"synthetic-b")
        call, owner = self._desktop()
        call("scan", {"folder": str(self.private)})  # 제외하기 전에 이미 DB에 들어간 사진
        owner._photo_album_service._drive_roots = (self.root,)
        call("excluded_folder_add", {"folder": str(self.private)})
        self.assertTrue(call("scan_default")["ok"])
        deadline = time.monotonic() + 10
        while call("scan_progress")["state"] in ("running", "cancelling") and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertEqual(call("scan_progress")["state"], "complete")
        owner._photo_album_service.exif.wait(10)  # 임시 폴더를 지우기 전에 촬영 정보 정리를 끝낸다.
        cached = owner._photo_album_service.cached_default_photos()
        self.assertEqual({item["name"] for item in cached["photos"]}, {shown.name})


if __name__ == "__main__":
    unittest.main()
