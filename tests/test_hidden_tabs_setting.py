import tempfile
import unittest
from pathlib import Path

from docfit_app.application_service import ApplicationService, clean_hidden_tabs


class HiddenTabsSettingTest(unittest.TestCase):
    def test_clean_keeps_only_valid_unique_sorted_ints_and_never_hides_document_edit(self):
        self.assertEqual(clean_hidden_tabs([7, 3, 3, 0, 12, 13, 14, -1, "5", True, 2.0, 11]), [3, 7, 11, 14])
        self.assertEqual(clean_hidden_tabs(None), [])
        self.assertEqual(clean_hidden_tabs("3"), [])

    def test_hidden_tabs_default_empty_and_survive_restart(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "settings.json"
            service = ApplicationService(path)
            self.assertEqual(service.dispatch("get_state", [])["hidden_tabs"], [])
            self.assertEqual(service.dispatch("set_hidden_tabs", [[4, 2, 0, 99]]), [2, 4])
            reloaded = ApplicationService(path)
            self.assertEqual(reloaded.dispatch("get_state", [])["hidden_tabs"], [2, 4])
            reloaded.dispatch("set_hidden_tabs", [[]])
            self.assertEqual(ApplicationService(path).dispatch("get_state", [])["hidden_tabs"], [])

    def test_required_tabs_are_ignored_when_saved_and_after_reload(self):
        # 0 is document editing; 12 and 13 are settings and usage instructions.
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "settings.json"
            service = ApplicationService(path)
            self.assertEqual(service.dispatch("set_hidden_tabs", [[0, 12, 13, 2]]), [2])
            self.assertEqual(ApplicationService(path).dispatch("get_state", [])["hidden_tabs"], [2])

    def test_invalid_values_are_safely_filtered_when_saved(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "settings.json"
            service = ApplicationService(path)
            invalid_values = [-1, 12, 13, 99, "2", "", None, True, 2.0]
            self.assertEqual(service.dispatch("set_hidden_tabs", [invalid_values]), [])
            self.assertEqual(ApplicationService(path).dispatch("get_state", [])['hidden_tabs'], [])
            self.assertEqual(service.dispatch("set_hidden_tabs", [[4, 2, 4, 2]]), [2, 4])
            self.assertEqual(ApplicationService(path).dispatch("get_state", [])['hidden_tabs'], [2, 4])

    def test_outliner_is_an_optional_new_last_tab_without_shifting_existing_numbers(self):
        with tempfile.TemporaryDirectory() as folder:
            service = ApplicationService(Path(folder) / 'settings.json')
            self.assertEqual(clean_hidden_tabs([11, 12, 13, 14, 0]), [11, 14])
            self.assertEqual(service.dispatch('set_hidden_tabs', [[14, 11]]), [11, 14])
            self.assertEqual(ApplicationService(Path(folder) / 'settings.json').dispatch('get_state', [])['hidden_tabs'], [11, 14])


if __name__ == "__main__":
    unittest.main()
