import unittest
from unittest.mock import patch

from docfit_core.hwp_process import restart_between_documents_enabled


class RestartBetweenDocumentsPolicyTests(unittest.TestCase):
    def test_single_document_never_restarts(self):
        self.assertFalse(restart_between_documents_enabled(1, env_value="1"))
        self.assertFalse(restart_between_documents_enabled(0, env_value="1"))

    def test_multiple_documents_do_not_restart_by_default(self):
        with patch.dict("os.environ", {}, clear=True):
            self.assertFalse(restart_between_documents_enabled(2))

    def test_environment_variable_can_disable_restart(self):
        for value in ("0", "false", "NO", "off", ""):
            with self.subTest(value=value):
                self.assertFalse(restart_between_documents_enabled(5, env_value=value))

    def test_environment_variable_can_enable_restart(self):
        for value in ("1", "true", "yes", "on"):
            with self.subTest(value=value):
                self.assertTrue(restart_between_documents_enabled(2, env_value=value))
        self.assertFalse(restart_between_documents_enabled(2, env_value="unexpected"))


if __name__ == "__main__":
    unittest.main()
