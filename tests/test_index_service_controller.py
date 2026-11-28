import threading
import unittest
from unittest import mock

from docfit_app.index_service import IndexServiceController


class IndexServiceControllerTests(unittest.TestCase):
    def setUp(self):
        patcher = mock.patch.dict("os.environ", {"DOCFIT_FORBID_REAL_DATA": "0"})
        patcher.start()
        self.addCleanup(patcher.stop)

    def _controller(self, state=None, install=None, uninstall=None):
        states = {"value": state}
        controller = IndexServiceController(
            query_state=lambda: states["value"],
            install=install or (lambda: (True, "설치했습니다.")),
            uninstall=uninstall or (lambda: (True, "제거했습니다.")),
            pipe_alive=lambda: states["value"] == "RUNNING",
        )
        return controller, states

    def _wait(self, controller):
        for thread in threading.enumerate():
            if thread.name.startswith("index-service-"):
                thread.join(timeout=3)
        return controller.status()

    def test_status_reports_missing_stopped_and_running_service(self):
        controller, states = self._controller(None)
        status = controller.command("status")
        self.assertTrue(status["ok"])
        self.assertFalse(status["installed"])
        self.assertFalse(status["running"])
        states["value"] = "STOPPED"
        status = controller.command("status")
        self.assertTrue(status["installed"] and not status["running"])
        states["value"] = "RUNNING"
        status = controller.command("status")
        self.assertTrue(status["running"] and status["pipe_alive"])

    def test_install_runs_in_background_and_reports_result(self):
        started = threading.Event()
        release = threading.Event()

        def slow_install():
            started.set()
            release.wait(timeout=3)
            return True, "서비스를 시작했습니다."

        controller, states = self._controller(None, install=slow_install)
        first = controller.command("install")
        self.assertEqual(first["busy"], "install")
        self.assertTrue(started.wait(timeout=2))
        # 진행 중에 또 누르면 거절한다.
        again = controller.command("install")
        self.assertFalse(again["ok"])
        self.assertEqual(again["code"], "BUSY")
        states["value"] = "RUNNING"
        release.set()
        done = self._wait(controller)
        self.assertIsNone(done["busy"])
        self.assertTrue(done["last_ok"])
        self.assertEqual(done["message"], "서비스를 시작했습니다.")
        self.assertTrue(done["running"])

    def test_failed_or_denied_install_keeps_a_helpful_message(self):
        controller, _ = self._controller(None, install=lambda: (False, ""))
        controller.command("install")
        done = self._wait(controller)
        self.assertFalse(done["last_ok"])
        self.assertIn("관리자 승인", done["message"])

    def test_exception_in_install_does_not_escape(self):
        def boom():
            raise OSError("access denied")

        controller, _ = self._controller(None, install=boom)
        controller.command("install")
        done = self._wait(controller)
        self.assertFalse(done["last_ok"])
        self.assertIn("access denied", done["message"])

    def test_uninstall_and_unknown_action(self):
        calls = []
        controller, _ = self._controller("RUNNING", uninstall=lambda: calls.append(1) or (True, "제거했습니다."))
        controller.command("uninstall")
        self.assertTrue(self._wait(controller)["last_ok"])
        self.assertEqual(calls, [1])
        self.assertFalse(controller.command("reboot")["ok"])

    def test_install_is_refused_while_real_data_is_forbidden(self):
        controller, _ = self._controller(None)
        with mock.patch.dict("os.environ", {"DOCFIT_FORBID_REAL_DATA": "1"}):
            result = controller.command("install")
        self.assertFalse(result["ok"])
        self.assertEqual(result["code"], "FORBIDDEN")


if __name__ == "__main__":
    unittest.main()
