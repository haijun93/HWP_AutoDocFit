import tempfile
import unittest
from pathlib import Path

from docfit_app.application_service import ApplicationService, clean_hidden_tabs


class HiddenTabsSettingTest(unittest.TestCase):
    def test_clean_keeps_only_valid_unique_sorted_ints_and_never_hides_document_edit(self):
        self.assertEqual(clean_hidden_tabs([7, 3, 3, 0, 12, -1, "5", True, 2.0, 11]), [3, 7, 11])
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


if __name__ == "__main__":
    unittest.main()
