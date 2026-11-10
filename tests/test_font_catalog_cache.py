import os
import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from docfit_engine import formatter


class FontCatalogDiskCacheTest(unittest.TestCase):
    def test_catalog_cache_hit_and_font_folder_mtime_invalidation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            font_folder = root / "fonts"
            font_folder.mkdir()
            local_app_data = root / "local"
            local_app_data.mkdir()
            scan = mock.Mock(return_value=["맑은 고딕", "한컴돋움"])
            bundle_scan = mock.Mock(return_value=[])
            with mock.patch.dict(os.environ, {"LOCALAPPDATA": str(local_app_data)}), \
                    mock.patch.object(formatter, "Path", Path, create=True), \
                    mock.patch.object(formatter, "re", re, create=True), \
                    mock.patch.object(formatter, "_hwp_설치폴더_후보_레지스트리", return_value=[]), \
                    mock.patch.object(formatter, "_hwp_설치폴더_후보_공통경로", return_value=[]), \
                    mock.patch.object(formatter, "한글_폰트_목록", side_effect=scan), \
                    mock.patch.object(formatter, "한컴_번들_폰트_폴더_목록", side_effect=bundle_scan):
                first = formatter.한글_폰트_목록_전체(str(font_folder))
                self.assertEqual(first, ["맑은 고딕", "한컴돋움"])
                self.assertEqual(scan.call_count, 1)
                self.assertEqual(bundle_scan.call_count, 1)

                second = formatter.한글_폰트_목록_전체(str(font_folder))
                self.assertEqual(second, first)
                self.assertEqual(scan.call_count, 1)
                self.assertEqual(bundle_scan.call_count, 1)

                (font_folder / "new-font.ttf").write_bytes(b"font")
                third = formatter.한글_폰트_목록_전체(str(font_folder))
                self.assertEqual(third, first)
                self.assertEqual(scan.call_count, 2)
                self.assertEqual(bundle_scan.call_count, 2)


if __name__ == "__main__":
    unittest.main()
