import tempfile
import unittest
from pathlib import Path

from docfit_app import package_integrity as pi


class PackageIntegrityTests(unittest.TestCase):
    def test_message_mentions_file_and_reinstall(self):
        msg = pi.missing_window_message('설정 창 프로그램', 'settings_win.exe')
        self.assertIn('settings_win.exe', msg)
        self.assertIn('다시 실행', msg)
        self.assertIn('설치가 완료되지 않았을 수 있습니다', msg)

    def test_find_missing(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / 'outliner_win.exe').write_bytes(b'x')
            missing = pi.find_missing_window_exes(d)
            self.assertNotIn('outliner_win.exe', missing)
            self.assertEqual(len(missing), len(pi.REQUIRED_WINDOW_EXES) - 1)

    def test_check_not_frozen_is_noop(self):
        self.assertEqual(pi.check_frozen_package(), [])

    def test_check_explicit_dir_logs(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertLogs(pi.__name__, 'WARNING'):
                self.assertEqual(len(pi.check_frozen_package(d)), len(pi.REQUIRED_WINDOW_EXES))

    def test_setup_iss_has_completion_check(self):
        text = (Path(__file__).resolve().parents[1] / 'installer' / 'setup.iss').read_text(encoding='utf-8')
        for name in pi.REQUIRED_WINDOW_EXES:
            self.assertIn(name, text.split('function MissingWindowExes')[1])
        self.assertIn('procedure DeinitializeSetup', text)


if __name__ == '__main__':
    unittest.main()
