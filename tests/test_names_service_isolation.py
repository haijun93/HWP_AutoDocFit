"""시험 중에는 이 PC 에 설치된 색인 서비스를 읽지 않는다."""
import os
import unittest
from unittest.mock import patch

from docfit_app import rust_scan


class NamesServiceIsolationTests(unittest.TestCase):
    def test_real_service_is_not_touched_when_real_data_is_forbidden(self):
        with patch.dict(os.environ, {"DOCFIT_FORBID_REAL_DATA": "1"}), \
                patch("docfit_app.kfind_process.is_pipe_alive") as alive:
            self.assertIsNone(rust_scan.names_query({"pdf"}))
        alive.assert_not_called()
