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


class DirectStateReadTests(unittest.TestCase):
    def test_pending_dialog_is_read_without_tk(self):
        gui = SimpleNamespace(_flutter_dialog={"id": "d1", "title": "확인", "done": object(), "answer": False})
        bridge, round_trips = _counting_bridge(gui)
        self.assertEqual(bridge.get_pending_dialog(), {"id": "d1", "title": "확인"})
        gui._flutter_dialog = None
        self.assertIsNone(bridge.get_pending_dialog())
        with bridge._close_lock:
            bridge._close_request = {"id": "c1", "kind": "app_close"}
        self.assertEqual(bridge.get_pending_dialog(), {"id": "c1", "kind": "app_close"})
        self.assertEqual(round_trips, [])

    def test_settings_snapshot_is_read_without_tk(self):
        bridge, round_trips = _counting_bridge(Mock(running=True))
        bridge._settings_snapshot = lambda: {"values": {"developer_mode": True}, "running": True}
        result = bridge.get_settings()
        self.assertTrue(result["ok"])
        self.assertIs(result["developer_mode"], True)
        self.assertEqual(round_trips, [])

    def test_settings_errors_keep_the_response_envelope(self):
        from docfit_app.api_contract import ServiceError
        bridge, round_trips = _counting_bridge(Mock())

        def broken():
            raise ServiceError("SETTINGS_READ_FAILED", "설정 파일을 읽지 못했습니다.")

        bridge._settings_snapshot = broken
        result = bridge.get_settings()
        self.assertIs(result["ok"], False)
        self.assertEqual(result["error"]["code"], "SETTINGS_READ_FAILED")
        self.assertEqual(round_trips, [])

    def test_cancel_job_reads_job_id_directly_and_stops_only_on_tk(self):
        gui = Mock(_job_id="job-1")
        bridge, round_trips = _counting_bridge(gui)
        manager = Mock()
        manager.cancel.return_value = {"id": "job-1", "state": "cancelling"}
        bridge._jobs = lambda: manager
        self.assertEqual(bridge.cancel_job("other"), {"id": "job-1", "state": "cancelling"})
        gui.작업중단.assert_not_called()
        self.assertEqual(round_trips, [])
        bridge.cancel_job("job-1")
        gui.작업중단.assert_called_once_with()
        self.assertEqual(len(round_trips), 1)

    def test_bulk_start_checks_running_without_tk(self):
        bridge, round_trips = _counting_bridge(Mock(running=True))
        result = bridge.hwp_bulk("start", {"roots": ["C:/"]})
        self.assertFalse(result["ok"])
        self.assertIn("다른 문서 작업", result["message"])
        self.assertEqual(round_trips, [])

    def test_conversion_start_copies_the_file_list_without_tk(self):
        files = ["a.hwp"]
        gui = Mock(running=False, files=files, _job_id=None)
        bridge, round_trips = _counting_bridge(gui)
        seen = []
        bridge._conversion_targets = lambda current: (seen.append(current), [
            {"format": "hwpx", "enabled": False, "reason": "준비 안 됨"}])[1]
        with self.assertRaisesRegex(ValueError, "준비 안 됨"):
            bridge._start_legacy("file_format")
        self.assertEqual(seen, [["a.hwp"]])
        self.assertIsNot(seen[0], files)
        self.assertEqual(round_trips, [])

    def test_direct_reads_are_rejected_while_closing(self):
        bridge, _ = _counting_bridge(Mock(running=False, files=[], _job_id=None))
        bridge._jobs = lambda: Mock()
        bridge._closed = True
        for call in (bridge.get_pending_dialog, bridge.get_settings, lambda: bridge.cancel_job("x"),
                     lambda: bridge.hwp_bulk("start", {}), lambda: bridge._start_legacy("file_format")):
            with self.assertRaisesRegex(RuntimeError, "종료"):
                call()


class SettingsStoreTests(unittest.TestCase):
    def setUp(self):
        import tempfile
        from pathlib import Path
        from docfit_app.settings_service import SettingsService
        folder = tempfile.TemporaryDirectory(prefix="docfit-tk0-")
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name).resolve()
        self.store = SettingsService(self.folder / "settings.json")
        gui = SimpleNamespace(running=False, _준말_서비스=lambda: SimpleNamespace(settings=self.store))
        self.bridge, self.round_trips = _counting_bridge(gui)

    def test_store_is_fetched_without_tk(self):
        self.assertIs(self.bridge._settings_store(), self.store)
        self.assertEqual(self.round_trips, [])

    def test_lazy_services_are_built_without_tk(self):
        rename = self.bridge._file_rename_service()
        self.assertIs(self.bridge._file_rename_service(), rename)
        machine = self.bridge._timemachine_service()
        self.assertIs(self.bridge._timemachine_service(), machine)
        self.assertEqual(self.round_trips, [])

    def test_kclip_window_uses_the_shared_store_without_tk(self):
        from unittest.mock import patch
        with patch("docfit_app.kclip_window_state.command", return_value={"ok": True}) as command:
            self.assertEqual(self.bridge.kclip_window("get"), {"ok": True})
        self.assertIs(command.call_args.args[0], self.store)
        self.assertEqual(self.round_trips, [])

    def test_document_tool_reads_the_settings_folder_without_tk(self):
        from unittest.mock import patch
        with patch("docfit_app.document_tools_service.DocumentToolsService") as service:
            service.return_value.command.return_value = {"done": True}
            result = self.bridge.document_tool("list", {})
        service.assert_called_once_with(self.folder)
        self.assertIs(result["ok"], True)
        self.assertEqual(result["data"], {"done": True})
        self.assertEqual(self.round_trips, [])

    def test_closing_app_is_reported_through_existing_error_paths(self):
        self.bridge._closed = True
        with self.assertRaisesRegex(RuntimeError, "종료"):
            self.bridge._settings_store()
        with self.assertLogs("desktop_web_ui", level="WARNING"):
            result = self.bridge.kclip_window("get")
        self.assertFalse(result["ok"])
        self.assertIn("종료", result["message"])

    def test_concurrent_readers_and_writer_never_see_a_torn_file(self):
        from concurrent.futures import ThreadPoolExecutor
        self.store.update(lambda current: current.update({"n": 0, "pad": "가" * 2000}))

        def write(index):
            self.bridge._settings_store().update(lambda current: current.update({"n": index}))

        def read(_):
            return self.bridge._settings_store().read()["pad"]

        with ThreadPoolExecutor(max_workers=8) as pool:
            writes = [pool.submit(write, index) for index in range(1, 21)]
            reads = [pool.submit(read, index) for index in range(60)]
            for future in writes:
                future.result(timeout=30)
            self.assertTrue(all(future.result(timeout=30) == "가" * 2000 for future in reads))
        self.assertIn(self.store.read()["n"], range(1, 21))
        self.assertEqual(self.round_trips, [])

    def test_table_sketch_and_table_units_read_the_store_without_tk(self):
        from unittest.mock import patch
        self.bridge._sketch_drop = {"ok": True, "kind": "png"}
        self.assertEqual(self.bridge.table_sketch("take_drop"), {"ok": True, "kind": "png"})
        with patch("docfit_app.table_units_service.TableUnitsService") as service:
            service.return_value.command.return_value = {"items": []}
            result = self.bridge._table_units_command("list", {})
        service.assert_called_once_with(settings=self.store)
        self.assertIs(result["ok"], True)
        self.assertEqual(self.round_trips, [])

    def test_initial_window_settings_are_read_without_tk(self):
        from desktop_web_ui import _initial_window_settings
        self.store.update(lambda current: current.update({"always_on_top": True}))
        values = _initial_window_settings(self.bridge)
        self.assertIs(values["always_on_top"], True)
        self.assertEqual(self.round_trips, [])
        self.bridge._closed = True
        with self.assertRaisesRegex(RuntimeError, "종료"):
            _initial_window_settings(self.bridge)


if __name__ == "__main__":
    unittest.main()
