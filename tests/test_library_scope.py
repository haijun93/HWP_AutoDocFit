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


if __name__ == "__main__":
    unittest.main()
