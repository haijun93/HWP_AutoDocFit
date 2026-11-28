"""시험 중에는 이 PC 에 설치된 색인 서비스를 읽지 않는다."""
import os
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app import kfind_client, rust_scan


class NamesServiceIsolationTests(unittest.TestCase):
    def test_real_service_is_not_touched_when_real_data_is_forbidden(self):
        with patch.dict(os.environ, {"DOCFIT_FORBID_REAL_DATA": "1"}), \
                patch("docfit_app.kfind_process.is_pipe_alive") as alive:
            self.assertIsNone(rust_scan.names_query({"pdf"}))
        alive.assert_not_called()

    def test_default_index_client_cannot_reach_the_installed_service_in_tests(self):
        with patch.dict(os.environ, {"DOCFIT_FORBID_REAL_DATA": "1"}),                 patch.object(kfind_client, "_open_pipe") as opened:
            client = kfind_client.KfindClient()
            for call in (client.status, lambda: client.remove(["synthetic"])):
                with self.assertRaises(kfind_client.KfindError):
                    call()
        opened.assert_not_called()

    def test_library_scan_modules_are_isolated_when_selected_alone(self):
        # `discover -s tests -p <모듈>` 은 tests/__init__.py 를 스스로 읽지 않는다. 시험은 돌리지 않고 읽기만 한다.
        code = ("import os, sys, unittest; unittest.defaultTestLoader.discover('tests', pattern=sys.argv[1]); "
                "print(os.environ.get('DOCFIT_FORBID_REAL_DATA'))")
        env = {key: value for key, value in os.environ.items() if key != "DOCFIT_FORBID_REAL_DATA"}
        for module in ("test_default_library_scan.py", "test_library_exclusion_settings.py"):
            with self.subTest(module=module):
                done = subprocess.run([sys.executable, "-c", code, module], env=env, capture_output=True,
                                      text=True, timeout=60, cwd=Path(__file__).resolve().parents[1])
                self.assertEqual((done.returncode, done.stdout.strip()), (0, "1"), done.stderr)
