import os
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


if __name__ == "__main__":
    unittest.main()
