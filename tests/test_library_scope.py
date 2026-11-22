import unittest
from pathlib import Path

from docfit_app.library_scope import default_roots, is_excluded_path


class LibraryScopeTests(unittest.TestCase):
    def test_roots_include_profile_and_onedrive_desktop_without_duplicates(self):
        roots = default_roots(
            ["C:\\", "D:\\", "C:\\"], "C:\\Users\\sample",
            desktop_paths=["C:\\Users\\sample\\Desktop"],
            one_drive_paths=["C:\\Users\\sample\\OneDrive\\Desktop"],
        )
        self.assertEqual(len(roots), 4)
        self.assertEqual(str(roots[0]).casefold(), "c:\\")
        self.assertEqual(str(roots[-1]).casefold(), "c:\\users\\sample\\onedrive\\desktop")

    def test_system_and_development_folders_are_excluded_by_component(self):
        for path in (r"C:\Windows\Temp", r"C:\Program Files\App", r"C:\Users\a\AppData\Local", r"D:\src\node_modules\pkg", r"D:\repo\.git\objects"):
            with self.subTest(path=path):
                self.assertTrue(is_excluded_path(path))
        self.assertFalse(is_excluded_path(r"D:\Documents\Reports"))

    def test_user_exclusion_matches_subtree_not_prefix_sibling(self):
        excluded = [r"D:\Private"]
        self.assertTrue(is_excluded_path(r"D:\Private\a.pdf", excluded))
        self.assertFalse(is_excluded_path(r"D:\Private-old\a.pdf", excluded))
        self.assertFalse(is_excluded_path(r"E:\Private\a.pdf", excluded))


if __name__ == "__main__":
    unittest.main()


class ReservedNamesRelativeToBaseTests(unittest.TestCase):
    """예약 폴더 이름은 스캔 시작 폴더 아래 상대 경로에서만 본다(시작 폴더가 AppData 아래여도 제외되지 않음)."""

    def test_base_under_appdata_is_not_excluded_but_child_appdata_is(self):
        from docfit_app.library_scope import is_excluded_path

        base = r"C:\Users\someone\AppData\Local\Temp\case1"
        self.assertTrue(is_excluded_path(base + r"\visible.pdf"))  # base 없이는 기존 동작(경로 전체)
        self.assertFalse(is_excluded_path(base + r"\visible.pdf", base=base))
        self.assertFalse(is_excluded_path(base, base=base))
        self.assertTrue(is_excluded_path(base + r"\AppData\x.pdf", base=base))
        self.assertTrue(is_excluded_path(base + r"\node_modules", base=base))
