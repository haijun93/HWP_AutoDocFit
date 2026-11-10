import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.file_rename_service import FileRenameService
from docfit_app.settings_schema import DEFAULTS


class Settings:
    def __init__(self, values=None):
        self.values = values or {}

    def read(self):
        return self.values


class RustRenamePreviewTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "files"
        self.root.mkdir()
        self.source = self.root / "IMG_12 report.JPG"
        self.source.write_text("fixture", encoding="utf-8")
        self.service = FileRenameService(Path(self.tmp.name) / "settings", settings=Settings())
        self.service.add_paths([str(self.source)])

    def tearDown(self):
        self.tmp.cleanup()

    def test_rust_preview_is_enabled_by_default_and_used(self):
        self.assertIs(DEFAULTS["rust_rename_preview"], True)
        self.service._settings.values = {}
        rust = {"items": [{"id": next(iter(self.service._entries)), "new_name": "IMG_012 report.jpg",
                            "status": "ok", "reason": None, "warnings": []}]}
        with patch.object(self.service, "_run_rust_preview", return_value=rust) as run:
            result = self.service.preview({"rules": [{"kind": "digits_pad", "digits": 3},
                                                       {"kind": "ext_case"}]})
        run.assert_called_once()
        self.assertEqual(result["items"][0]["new_name"], "IMG_012 report.jpg")
        self.assertEqual(result["summary"]["ok"], 1)

    def test_rust_failure_falls_back_to_python_and_setting_can_disable(self):
        rules = [{"kind": "digits_pad", "digits": 3}, {"kind": "ext_case"}]
        with patch.object(self.service, "_run_rust_preview", side_effect=OSError("unavailable")) as run:
            failed = self.service.preview({"rules": rules})
        run.assert_called_once()
        self.assertEqual(failed["items"][0]["new_name"], "IMG_012 report.jpg")

        self.service._settings.values = {"rust_rename_preview": False}
        with patch.object(self.service, "_run_rust_preview") as disabled:
            result = self.service.preview({"rules": rules})
        disabled.assert_not_called()
        self.assertEqual(result["items"][0]["new_name"], "IMG_012 report.jpg")


if __name__ == "__main__":
    unittest.main()
