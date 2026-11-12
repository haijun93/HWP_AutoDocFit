import subprocess
import unittest
from unittest.mock import patch

from docfit_core import hwp_process


class HwpProcessEncodingTests(unittest.TestCase):
    def test_run_decodes_cp949_tasklist_and_preserves_csv_pid(self):
        output = '"Hwp.exe","1234","Console","1","12,345 K","?? ?"\r\n'
        encoded = output.encode('cp949')
        completed = subprocess.CompletedProcess(args=['tasklist'], returncode=0,
                                                stdout=encoded, stderr=b'')

        with patch('subprocess.run', return_value=completed) as run:
            result = hwp_process._run(['tasklist'])

        self.assertIsInstance(result.stdout, str)
        self.assertEqual(result.stdout, output)
        self.assertEqual(hwp_process.hwp_pids(runner=lambda _args: result), {1234})
        self.assertTrue(run.call_args.kwargs['capture_output'])
        self.assertFalse(run.call_args.kwargs['text'])

    def test_run_uses_cp949_fallback_then_replacement(self):
        output = 'Hwp.exe,4321,??'.encode('cp949')
        completed = subprocess.CompletedProcess(args=['tasklist'], returncode=0,
                                                stdout=output, stderr=b'')
        with patch('subprocess.run', return_value=completed), \
             patch('locale.getpreferredencoding', return_value='utf-8'):
            result = hwp_process._run(['tasklist'])
        self.assertEqual(result.stdout, 'Hwp.exe,4321,??')

        bad = subprocess.CompletedProcess(args=['tasklist'], returncode=0,
                                          stdout=b'\xff', stderr=b'')
        with patch('subprocess.run', return_value=bad), \
             patch('locale.getpreferredencoding', return_value='utf-8'):
            result = hwp_process._run(['tasklist'])
        self.assertEqual(result.stdout, chr(0xfffd))


if __name__ == '__main__':
    unittest.main()
