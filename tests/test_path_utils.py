import unittest

from docfit_core.path_utils import path_is_within


class PathUtilsTests(unittest.TestCase):
    def test_windows_paths_on_different_drives_are_outside_without_raising(self):
        for candidate in (r"E:\Reports\a.pdf", r"F:\Reports\a.pdf"):
            with self.subTest(candidate=candidate):
                self.assertFalse(path_is_within(candidate, "C:\\"))

    def test_windows_paths_are_checked_by_component_on_each_drive(self):
        self.assertTrue(path_is_within(r"E:\Reports\2026\a.pdf", r"E:\Reports"))
        self.assertFalse(path_is_within(r"E:\Reports-old\a.pdf", r"E:\Reports"))

    def test_posix_paths_are_checked_by_component(self):
        self.assertTrue(path_is_within("/tmp/docs/a.pdf", "/tmp/docs"))
        self.assertFalse(path_is_within("/tmp/docs-old/a.pdf", "/tmp/docs"))


if __name__ == "__main__":
    unittest.main()
