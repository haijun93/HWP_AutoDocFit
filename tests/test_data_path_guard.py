from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app import kagent_client, kfind_process


class DataPathGuardTests(unittest.TestCase):
    def test_test_package_redirects_profile_paths_and_default_data_dir(self):
        from docfit_app.kagent_client import data_dir

        self.assertEqual(os.environ.get("DOCFIT_FORBID_REAL_DATA"), "1")
        for name in ("LOCALAPPDATA", "APPDATA", "USERPROFILE", "HOME"):
            self.assertIn(tempfile.gettempdir(), os.environ[name])
        # 임시 폴더 자체가 사용자 폴더 아래에 있을 수 있으므로 문자열이 아니라 '시험 샌드박스 안인지'로 확인한다.
        sandbox = os.path.normcase(os.path.abspath(os.environ['LOCALAPPDATA']))
        self.assertTrue(os.path.normcase(os.path.abspath(str(data_dir()))).startswith(sandbox))
        self.assertNotEqual(sandbox, os.path.normcase(os.path.abspath(os.environ['DOCFIT_REAL_LOCALAPPDATA'])))

    def test_guard_rejects_real_localappdata(self):
        real = Path(tempfile.gettempdir()) / "docfit-real-user-data"
        with patch.dict(os.environ, {
            "DOCFIT_FORBID_REAL_DATA": "1",
            "DOCFIT_REAL_LOCALAPPDATA": str(real),
            "LOCALAPPDATA": str(real),
        }, clear=False):
            with self.assertRaises(RuntimeError):
                kagent_client.data_dir()
            with self.assertRaises(RuntimeError):
                kfind_process.data_dir()

