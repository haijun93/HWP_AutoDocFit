import os
import ctypes
import json
from concurrent.futures import ThreadPoolExecutor
from unittest.mock import Mock, patch
import stat
import sys
import tempfile
import unittest
from pathlib import Path

from docfit_app import hierarchy_bridge as hb

PARAS = [{"text": "1. 추진 배경", "indent": 0, "font_size": 15},
         {"text": "가. 세부", "indent": 10, "font_size": 13}]


def fake(directory, body):
    path = Path(directory) / "fake.py"
    path.write_text("#!%s\n%s\n" % (sys.executable, body), encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)
    return path


@unittest.skipIf(sys.platform == "win32", "가짜 실행 파일은 POSIX 전용")
class FakeExecutableTests(unittest.TestCase):
    def test_disabled_by_default(self):
        self.assertIsNone(hb.analyze_levels(PARAS, {}, executable="/nonexistent"))
        self.assertIsNone(hb.analyze_levels(PARAS, {"hierarchy_engine": False}))

    def test_success(self):
        with tempfile.TemporaryDirectory() as d:
            payload = '{"role_sequence":["제목","중제목"],"styles":[]}'
            exe = fake(d, f'import sys,json;json.load(sys.stdin);print({payload!r})')
            r = hb.analyze_levels(PARAS, {"hierarchy_engine": True}, executable=exe)
        self.assertEqual(r["role_sequence"], ["제목", "중제목"])

    def test_timeout(self):
        with tempfile.TemporaryDirectory() as d:
            exe = fake(d, "import time;time.sleep(5)")
            self.assertIsNone(hb.analyze_levels(PARAS, {"hierarchy_engine": True}, executable=exe, timeout=0.3))

    def test_error_paths(self):
        with tempfile.TemporaryDirectory() as d:
            for body in ("import sys;sys.exit(2)", "print('not json')", 'print(\'{"role_sequence":["제목"]}\')'):
                exe = fake(d, body)
                self.assertIsNone(hb.analyze_levels(PARAS, {"hierarchy_engine": True}, executable=exe), body)
        self.assertIsNone(hb.analyze_levels(PARAS, {"hierarchy_engine": True}, executable="/nonexistent/x"))


class RealExecutableTests(unittest.TestCase):
    def test_real_call(self):
        exe = hb.find_executable()
        if exe is None:
            self.skipTest("hierarchy 실행 파일 없음")
        r = hb.analyze_levels(PARAS, {"hierarchy_engine": True}, executable=exe)
        self.assertEqual(len(r["role_sequence"]), 2)
        self.assertEqual(hb.analyze_levels([], {"hierarchy_engine": True})["role_sequence"], [])


class CacheTests(unittest.TestCase):
    def setUp(self):
        hb._LIBRARIES.clear()
        hb._SETTINGS_CACHE = None

    def tearDown(self):
        hb._LIBRARIES.clear()
        hb._SETTINGS_CACHE = None

    def test_library_loaded_once_per_path_across_threads(self):
        buffer = ctypes.create_string_buffer(b'{"role_sequence":["title","body"]}')
        dll = Mock()
        dll.hierarchy_analyze_json.return_value = ctypes.addressof(buffer)
        with patch.object(hb.ctypes, "CDLL", return_value=dll) as load:
            with ThreadPoolExecutor(max_workers=8) as pool:
                results = list(pool.map(lambda _: hb.run_library("cache-test.dll", PARAS), range(300)))
            self.assertTrue(all(r == {"role_sequence": ["title", "body"]} for r in results))
            self.assertEqual(load.call_count, 1)
            hb.run_library(Path.cwd() / "cache-test.dll", PARAS)
            self.assertEqual(load.call_count, 1)
            hb.run_library("cache-other.dll", PARAS)
            self.assertEqual(load.call_count, 2)
        self.assertEqual(dll.hierarchy_json_free.call_count, 302)

    def test_settings_reused_and_changes_apply_on_next_analysis(self):
        from docfit_app.settings_service import SettingsService
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, APPDATA=directory):
            path = Path(directory) / "HwpAutoDocFit" / "settings.json"
            path.parent.mkdir()
            path.write_text('{"hierarchy_engine":true}', encoding="utf-8")
            read = SettingsService.read
            with patch.object(SettingsService, "read", autospec=True, side_effect=read) as reads, \
                    patch.object(hb, "find_library", return_value=None), \
                    patch.object(hb, "run_executable", return_value={"role_sequence": []}) as run:
                for _ in range(300):
                    self.assertIsNotNone(hb.analyze_levels(PARAS, executable="unused"))
                self.assertEqual(reads.call_count, 1)
                previous = path.stat()
                # Same size, different mtime must invalidate.
                path.write_text('{"hierarchy_engine":null}', encoding="utf-8")
                os.utime(path, ns=(previous.st_atime_ns, previous.st_mtime_ns + 1000000000))
                self.assertIsNone(hb.analyze_levels(PARAS, executable="unused"))
                self.assertEqual(reads.call_count, 2)
                # Same mtime, different size must also invalidate.
                stamp = path.stat()
                path.write_text('{"hierarchy_engine":true,"other":1}', encoding="utf-8")
                os.utime(path, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
                self.assertIsNotNone(hb.analyze_levels(PARAS, executable="unused"))
                self.assertEqual(reads.call_count, 3)
                path.unlink()
                self.assertEqual(hb._read_settings().get("hierarchy_engine"),
                                 __import__("docfit_app.settings_schema", fromlist=["DEFAULTS"]).DEFAULTS.get("hierarchy_engine"))
                path.write_text('{"hierarchy_engine":false}', encoding="utf-8")
                self.assertIsNone(hb.analyze_levels(PARAS, executable="unused"))
                self.assertEqual(reads.call_count, 5)
                self.assertEqual(run.call_count, 301)

    def test_library_failure_is_retried(self):
        with patch.object(hb.ctypes, "CDLL", side_effect=OSError("missing")) as load:
            self.assertIsNone(hb.run_library("missing.dll", PARAS))
            self.assertIsNone(hb.run_library("missing.dll", PARAS))
            self.assertEqual(load.call_count, 2)


if __name__ == "__main__":
    unittest.main()
