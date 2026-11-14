"""IDLE1 디스크 부하 판정: psutil 없이 Win32 API로 잰다(NBIDX1, 2026-11-13)."""
import os
import unittest
from types import SimpleNamespace
from unittest import mock

from docfit_app import idle_gate


class Win32DiskCountersTest(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "Windows 전용")
    def test_counters_need_no_psutil(self):
        total, own = idle_gate.win32_disk_bytes(), idle_gate.win32_own_io_bytes()
        self.assertIsInstance(total, int)
        self.assertIsInstance(own, int)
        self.assertGreaterEqual(total, 0)

    @unittest.skipUnless(os.name == "nt", "Windows 전용")
    def test_probe_measures_disk_without_psutil(self):
        clock = iter([0.0, 1.0])
        probe = idle_gate.WindowsIdleProbe(clock=lambda: next(clock))
        self.assertIsNone(probe._psutil)
        self.assertIsNone(probe._disk())  # 첫 측정은 기준만
        value = probe._disk()
        self.assertIsNotNone(value)
        self.assertGreaterEqual(value, 0.0)

    def test_win32_values_subtract_own_io(self):
        clock = iter([0.0, 2.0])
        probe = idle_gate.WindowsIdleProbe(clock=lambda: next(clock))
        with mock.patch.object(idle_gate, "win32_disk_bytes", side_effect=[1000, 9000]), \
                mock.patch.object(idle_gate, "win32_own_io_bytes", side_effect=[100, 2100]):
            self.assertIsNone(probe._disk())
            self.assertEqual(probe._disk(), 3000.0)  # (8000 - 2000) / 2초

    def test_falls_back_to_psutil_only_when_win32_fails(self):
        disk = iter([SimpleNamespace(read_bytes=0, write_bytes=0), SimpleNamespace(read_bytes=500, write_bytes=500)])
        fake = SimpleNamespace(disk_io_counters=lambda: next(disk),
                               Process=lambda: SimpleNamespace(io_counters=lambda: SimpleNamespace(read_bytes=0, write_bytes=0)))
        clock = iter([0.0, 1.0])
        probe = idle_gate.WindowsIdleProbe(psutil_module=fake, clock=lambda: next(clock))
        with mock.patch.object(idle_gate, "win32_disk_bytes", return_value=None):
            self.assertIsNone(probe._disk())
            self.assertEqual(probe._disk(), 1000.0)

    def test_no_measurement_without_win32_or_psutil(self):
        probe = idle_gate.WindowsIdleProbe()
        with mock.patch.object(idle_gate, "win32_disk_bytes", return_value=None):
            self.assertIsNone(probe._disk())


if __name__ == "__main__":
    unittest.main()
