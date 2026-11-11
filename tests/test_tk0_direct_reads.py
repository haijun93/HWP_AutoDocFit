"""Tk 제거 0단계: Tk 위젯이 필요 없는 호출은 Tk 스레드를 왕복하지 않는다."""

from threading import Event
from types import SimpleNamespace
import unittest
from unittest.mock import Mock

from desktop_web_ui import DesktopWebBridge


def _counting_bridge(gui):
    bridge = DesktopWebBridge(gui)
    names = []

    def tk(callback, **kwargs):
        names.append(getattr(callback, "__name__", "?"))
        return callback()

    bridge._tk = tk
    return bridge, names


class ZeroClickInvalidationTests(unittest.TestCase):
    def setUp(self):
        self.bridge, self.round_trips = _counting_bridge(Mock())
        self.bridge.get_state = Mock(return_value={"status": "대기"})

    def test_cancel_and_start_do_not_visit_tk(self):
        self.bridge._ux().zero_click = {"token": "old"}
        self.assertEqual(self.bridge.cancel_zero_click(), {"status": "대기"})
        self.assertEqual(self.bridge._ux().zero_click, {})
        self.bridge.start_zero_click("token")
        self.assertEqual(self.round_trips, [])

    def test_edit_wrapper_invalidates_without_an_extra_round_trip(self):
        self.bridge._ux().zero_click = {"token": "old"}
        # add_folder는 스스로 Tk를 쓰지 않으므로 감싼 무효화가 유일한 왕복이었다.
        self.assertEqual(self.bridge.add_folder(), {"ok": False, "error": "FILE_SELECTION_REQUIRED"})
        self.assertEqual(self.bridge._ux().zero_click, {})
        self.assertEqual(self.round_trips, [])

    def test_edit_method_still_runs_its_own_tk_work_once(self):
        self.bridge.gui.running = False
        self.bridge.set_range(True, "2", "3")
        self.assertEqual(self.round_trips, ["update"])

    def test_closing_app_rejects_direct_commands_like_tk_did(self):
        self.bridge.tk_stopped = Event()
        self.bridge.tk_stopped.set()
        for call in (self.bridge.cancel_zero_click, self.bridge.add_folder):
            with self.assertRaisesRegex(RuntimeError, "종료"):
                call()
        self.bridge.tk_stopped = None
        self.bridge._closed = True
        with self.assertRaisesRegex(RuntimeError, "종료"):
            self.bridge.start_zero_click("token")


if __name__ == "__main__":
    unittest.main()
