"""Tk 제거 0단계: 무기한 대기 명령은 시작되기까지만 짧게 기다리고, 시작한 뒤에는 끝까지 기다린다."""

from concurrent.futures import Future
from queue import Queue
from threading import Event, Thread
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from desktop_web_ui import DesktopWebBridge, TkBusyError


class _QueuedRoot:
    def __init__(self):
        self.callbacks = Queue()

    def after(self, delay, callback):
        self.callbacks.put(callback)


class StartTimeoutTests(unittest.TestCase):
    def setUp(self):
        self.root = _QueuedRoot()
        self.bridge = DesktopWebBridge(SimpleNamespace(root=self.root))
        self.bridge.tk_stopped = Event()
        limit = patch("desktop_web_ui._TK_START_TIMEOUT", 0.05)
        limit.start()
        self.addCleanup(limit.stop)

    def in_thread(self, call):
        outcome = Future()

        def run():
            try:
                outcome.set_result(call())
            except BaseException as exc:
                outcome.set_exception(exc)

        thread = Thread(target=run, daemon=True)
        thread.start()
        return thread, outcome

    def test_busy_tk_rejects_an_indefinite_command_and_never_runs_it_later(self):
        callback = Mock()
        with self.assertRaisesRegex(TkBusyError, "다른 작업을 처리하고") as caught:
            self.bridge._tk(callback, timeout=None, front=True)
        self.assertIsInstance(caught.exception, RuntimeError)
        self.assertEqual(self.bridge._requests, set())
        self.root.callbacks.get_nowait()()   # Tk가 뒤늦게 큐에서 꺼내도 실행하지 않는다.
        callback.assert_not_called()

    def test_started_command_is_awaited_past_the_start_limit(self):
        entered, release = Event(), Event()

        def write():
            entered.set()
            self.assertTrue(release.wait(5))
            return "saved"

        thread, outcome = self.in_thread(lambda: self.bridge._tk(write, timeout=None))
        tk_thread = Thread(target=self.root.callbacks.get(timeout=2), daemon=True)
        tk_thread.start()
        self.assertTrue(entered.wait(2))
        with self.assertRaises(Exception):   # 시작 한도(0.05초)를 한참 넘겨도 아직 기다리는 중이다.
            outcome.result(timeout=0.3)
        self.assertFalse(outcome.done())
        release.set()
        self.assertEqual(outcome.result(timeout=2), "saved")
        tk_thread.join(timeout=2)
        thread.join(timeout=2)
        self.assertEqual(self.bridge._requests, set())

    def test_command_that_finishes_quickly_is_unchanged(self):
        thread, outcome = self.in_thread(lambda: self.bridge._tk(lambda: "opened", timeout=None))
        self.root.callbacks.get(timeout=2)()
        self.assertEqual(outcome.result(timeout=2), "opened")
        thread.join(timeout=2)

    def test_callback_error_still_reaches_the_caller(self):
        def broken():
            raise ValueError("나쁜 값")

        thread, outcome = self.in_thread(lambda: self.bridge._tk(broken, timeout=None))
        self.root.callbacks.get(timeout=2)()
        with self.assertRaisesRegex(ValueError, "나쁜 값"):
            outcome.result(timeout=2)
        thread.join(timeout=2)

    def test_default_timeout_commands_keep_waiting_in_the_queue(self):
        thread, outcome = self.in_thread(lambda: self.bridge._tk(lambda: "late"))
        dispatch = self.root.callbacks.get(timeout=2)
        with self.assertRaises(Exception):
            outcome.result(timeout=0.3)   # 시작 한도를 넘겨도 120초 기본 대기 명령은 취소되지 않는다.
        self.assertFalse(outcome.done())
        dispatch()
        self.assertEqual(outcome.result(timeout=2), "late")
        thread.join(timeout=2)

    def test_explicit_none_waits_for_the_start_without_a_limit(self):
        thread, outcome = self.in_thread(
            lambda: self.bridge._tk(lambda: "closed", timeout=None, start_timeout=None))
        dispatch = self.root.callbacks.get(timeout=2)
        with self.assertRaises(Exception):
            outcome.result(timeout=0.3)
        self.assertFalse(outcome.done())
        dispatch()
        self.assertEqual(outcome.result(timeout=2), "closed")
        thread.join(timeout=2)

    def test_native_close_is_not_rejected_as_busy(self):
        self.root.winfo_exists = Mock(return_value=True)

        def destroy():
            self.bridge._shutdown()
            self.root.winfo_exists.return_value = False

        self.bridge.gui.종료 = destroy
        thread, outcome = self.in_thread(self.bridge.request_close)
        dispatch = self.root.callbacks.get(timeout=2)
        with self.assertRaises(Exception):
            outcome.result(timeout=0.3)
        self.assertFalse(outcome.done())
        dispatch()
        self.assertTrue(outcome.result(timeout=2))
        thread.join(timeout=2)

    def test_shutdown_releases_a_caller_waiting_for_the_start(self):
        with patch("desktop_web_ui._TK_START_TIMEOUT", 30):
            thread, outcome = self.in_thread(lambda: self.bridge._tk(Mock(), timeout=None))
            self.root.callbacks.get(timeout=2)
            self.bridge._shutdown()
            with self.assertRaisesRegex(RuntimeError, "종료"):
                outcome.result(timeout=2)
            thread.join(timeout=2)

    def test_file_open_commands_report_busy_instead_of_hanging(self):
        self.bridge.gui._결과목록 = []
        for call in (lambda: self.bridge.open_result(0), lambda: self.bridge.show_result(0),
                     lambda: self.bridge.open_source_file(0)):
            with self.assertRaises(TkBusyError):
                call()
        self.assertEqual(self.bridge._requests, set())


if __name__ == "__main__":
    unittest.main()
