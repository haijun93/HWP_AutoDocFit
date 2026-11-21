"""Windows end-to-end coverage for the native Dock named-event bridge."""
import ctypes
import os
import threading
import unittest
from ctypes import wintypes

from docfit_app.dock_runtime import DockRuntime


@unittest.skipUnless(os.name == "nt", "Win32 named events are Windows-only")
class DockRuntimeEventTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        cls.kernel32.OpenEventW.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.LPCWSTR)
        cls.kernel32.OpenEventW.restype = wintypes.HANDLE
        cls.kernel32.SetEvent.argtypes = (wintypes.HANDLE,)
        cls.kernel32.SetEvent.restype = wintypes.BOOL
        cls.kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        cls.kernel32.CloseHandle.restype = wintypes.BOOL
        cls.kernel32.CreateEventW.argtypes = (ctypes.c_void_p, wintypes.BOOL, wintypes.BOOL, wintypes.LPCWSTR)
        cls.kernel32.CreateEventW.restype = wintypes.HANDLE

    def _runtime(self, received):
        class FakeProcess:
            def poll(self):
                return None

        return DockRuntime(
            executable=lambda: "fake-dock.exe",
            process_start=lambda *args, **kwargs: FakeProcess(),
            dispatch_js=received.append,
        )

    def _signal(self, name):
        handle = self.kernel32.OpenEventW(0x0002, False, name)  # EVENT_MODIFY_STATE
        self.assertTrue(handle, f"OpenEventW failed for {name}: {ctypes.get_last_error()}")
        try:
            self.assertTrue(self.kernel32.SetEvent(handle))
        finally:
            self.kernel32.CloseHandle(handle)

    def test_open_and_close_events_reach_fake_flutter_host_and_stop(self):
        received = []
        runtime = self._runtime(received)
        runtime.start()
        self._signal("docfit-open-tab-4")
        self._signal("docfit-close-tab-4")
        expected = {
            "window.dispatchEvent(new Event('docfit-open-tab-4'));",
            "window.dispatchEvent(new Event('docfit-close-tab-4'));",
        }
        deadline = __import__("time").monotonic() + 2
        while not expected.issubset(received) and __import__("time").monotonic() < deadline:
            threading.Event().wait(0.01)
        self.assertTrue(expected.issubset(received), received)
        receiver_thread = runtime._tab_events._thread
        runtime.stop()
        self.assertFalse(receiver_thread.is_alive())
        self.assertIsNone(runtime._tab_events)

    def test_existing_named_event_is_reused(self):
        name = "docfit-open-tab-9"
        existing = self.kernel32.CreateEventW(None, False, False, name)
        self.assertTrue(existing)
        received = []
        runtime = self._runtime(received)
        try:
            runtime.start()
            self._signal(name)
            expected = "window.dispatchEvent(new Event('docfit-open-tab-9'));"
            deadline = __import__("time").monotonic() + 2
            while expected not in received and __import__("time").monotonic() < deadline:
                threading.Event().wait(0.01)
            self.assertIn(expected, received)
        finally:
            runtime.stop()
            self.kernel32.CloseHandle(existing)


if __name__ == "__main__":
    unittest.main()
