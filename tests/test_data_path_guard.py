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
        self.assertNotIn("C:\\Users\\", str(data_dir()))

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

