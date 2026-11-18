"""기본 문서·사진 범위의 제외 폴더 설정 저장·복원 및 적용 시험."""
import tempfile
import time
import unittest
from pathlib import Path

from docfit_app.doc_grid_service import DocGridService
from docfit_app.photo_album_service import PhotoAlbumService
from docfit_app.settings_service import SettingsService


class LibraryExclusionSettingsTests(unittest.TestCase):
    def wait_for_scan(self, service):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            progress = service.command("scan_progress")
            if progress["state"] in ("complete", "failed", "cancelled"):
                return progress
            time.sleep(0.002)
        self.fail("기본 범위 스캔이 완료되지 않았습니다.")

    def test_excluded_folder_setting_is_persisted_restored_and_used_by_services(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            root = base / "drive"
            private = root / "Private"
            private.mkdir(parents=True)
            (root / "visible.pdf").touch()
            (private / "hidden.pdf").touch()
            (private / "hidden.jpg").write_bytes(b"not-an-image")
            store = SettingsService(base / "settings.json")
            store.update(lambda current: current.update(theme_mode="dark", unrelated_value=17))

            documents = DocGridService(thumb_dir=base / "doc-thumbs", drive_roots=[root], settings_store=store)
            added = documents.command("excluded_folder_add", {"folder": str(private)})
            self.assertTrue(added["ok"])
            self.assertEqual(added["folders"], [str(private)])
            self.assertEqual(store.read()["theme_mode"], "dark")
            self.assertEqual(store.read()["unrelated_value"], 17)

            restored_store = SettingsService(base / "settings.json")
            photos = PhotoAlbumService(db_path=base / "photos.sqlite", thumb_dir=base / "photo-thumbs",
                                       drive_roots=[root], settings_store=restored_store)
            self.assertEqual(photos.command("excluded_folders_get")["folders"], [str(private)])

            documents.command("list_default")
            doc_state = self.wait_for_scan(documents)
            self.assertEqual([item["name"] for item in doc_state["result"]["documents"]], ["visible.pdf"])

            photos.command("scan_default")
            photo_state = self.wait_for_scan(photos)
            self.assertEqual(photo_state["result"]["scanned"], 0)

            removed = photos.command("excluded_folder_remove", {"folder": str(private)})
            self.assertTrue(removed["ok"])
            self.assertEqual(removed["folders"], [])
            self.assertEqual(SettingsService(base / "settings.json").read()["theme_mode"], "dark")


if __name__ == "__main__":
    unittest.main()
