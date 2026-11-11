"""Tk 제거 0단계: 오류추적이 켜져 있을 때 _tk 왕복의 호출 이름·걸린 시간을 남긴다."""

import json
from pathlib import Path
from queue import Queue
import tempfile
from threading import Event, Thread
from types import SimpleNamespace
import unittest

from desktop_web_ui import DesktopWebBridge
from docfit_app.freeze_tracer import FreezeTracer


class _QueuedRoot:
    def __init__(self):
        self.callbacks = Queue()

    def after(self, delay, callback):
        self.callbacks.put(callback)


class _ImmediateRoot:
    def after(self, delay, callback):
        callback()


class TracerRoundTripTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory(prefix="docfit-tk0-trace-")
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name)
        self.tracer = FreezeTracer(self.folder, threshold=0.02)

    def test_round_trips_are_listed_and_summed_by_name(self):
        self.tracer.record_tk("get_state:snapshot", 0.2, 0.1, 0.3, "ok")
        self.tracer.record_tk("get_state:snapshot", 1.5, 0.5, 2.0, "ok")
        self.tracer.record_tk("set_range:update", 5.0, None, 5.0, "timeout")
        summary = self.tracer.tk_summary()
        self.assertEqual([row["name"] for row in summary], ["set_range:update", "get_state:snapshot"])
        state = summary[1]
        self.assertEqual((state["count"], state["failed"]), (2, 0))
        self.assertEqual((state["total_seconds"], state["queue_seconds"]), (2.3, 1.7))
        self.assertEqual((state["max_seconds"], state["max_queue_seconds"]), (2.0, 1.5))
        self.assertEqual(summary[0]["failed"], 1)

    def test_freeze_snapshot_includes_round_trips(self):
        self.tracer.record_tk("open_result:open_one", 0.01, 4.0, 4.01, "ok")
        token = self.tracer.begin_tk("get_state:snapshot")
        self.tracer._heartbeat = self.tracer._tk_heartbeat = self.tracer.clock() - 1
        data = json.loads(self.tracer.check_now().read_text(encoding="utf-8"))
        self.tracer.end_tk(token)
        self.assertEqual(data["recent_tk_roundtrips"][-1]["name"], "open_result:open_one")
        self.assertEqual(data["recent_tk_roundtrips"][-1]["run_seconds"], 4.0)
        self.assertEqual(data["tk_roundtrip_stats"][0]["count"], 1)

    def test_recent_list_is_bounded(self):
        for index in range(80):
            self.tracer.record_tk(f"call-{index % 3}", 0, 0, 0, "ok")
        self.assertEqual(len(self.tracer._tk_calls), 50)
        self.assertEqual(sum(row["count"] for row in self.tracer.tk_summary()), 80)

    def test_stop_writes_summary_once_and_nothing_when_unused(self):
        self.tracer.stop()
        self.assertEqual(list(self.folder.glob("*.json")), [])
        self.tracer.record_tk("get_state:snapshot", 0.1, 0.2, 0.3, "ok")
        self.tracer.stop()
        files = list(self.folder.glob("tk-roundtrips-*.json"))
        self.assertEqual(len(files), 1)
        data = json.loads(files[0].read_text(encoding="utf-8"))
        self.assertEqual((data["round_trips"], data["total_seconds"]), (1, 0.3))
        self.assertEqual(data["calls"][0]["name"], "get_state:snapshot")
        self.assertEqual(data["recent"][0]["outcome"], "ok")
        self.tracer.stop()
        self.assertEqual(len(list(self.folder.glob("tk-roundtrips-*.json"))), 1)

    def test_summary_files_are_pruned_separately_from_freeze_snapshots(self):
        tracer = FreezeTracer(self.folder, keep=2)
        tracer._write_snapshot(("test",), [])
        for index in range(4):
            tracer.record_tk("call", 0, 0, index, "ok")
            tracer.write_tk_summary()
        self.assertEqual(len(list(self.folder.glob("tk-roundtrips-*.json"))), 2)
        self.assertEqual(len(list(self.folder.glob("freeze-*.json"))), 1)


class BridgeTracingTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory(prefix="docfit-tk0-bridge-")
        self.addCleanup(folder.cleanup)
        self.tracer = FreezeTracer(folder.name)

    def bridge(self, root, tracing=True):
        bridge = DesktopWebBridge(SimpleNamespace(root=root))
        bridge.tk_stopped = Event()
        if tracing:
            bridge._freeze_tracer = self.tracer
        return bridge

    def test_round_trip_is_named_after_the_calling_method(self):
        bridge = self.bridge(_ImmediateRoot())

        def update():
            return 7

        def set_sample():
            return bridge._tk(update)

        self.assertEqual(set_sample(), 7)
        entry = self.tracer._tk_calls[-1]
        self.assertEqual((entry["name"], entry["outcome"]), ("set_sample:update", "ok"))
        self.assertIsNotNone(entry["run_seconds"])
        self.assertGreaterEqual(entry["total_seconds"], entry["queue_seconds"])
        self.assertEqual(self.tracer._tk_pending, {})
        # 기존 최근 요청 기록(콜백 이름)도 그대로 남긴다.
        self.assertEqual(self.tracer._api[-1]["name"], "update")

    def test_callback_error_is_recorded_with_its_type(self):
        bridge = self.bridge(_ImmediateRoot())

        def broken():
            raise ValueError("나쁜 값")

        with self.assertRaisesRegex(ValueError, "나쁜 값"):
            bridge._tk(broken)
        self.assertEqual(self.tracer._tk_calls[-1]["outcome"], "ValueError")
        self.assertEqual(self.tracer.tk_summary()[0]["failed"], 1)

    def test_request_that_never_ran_leaves_no_pending_entry(self):
        root = _QueuedRoot()
        bridge = self.bridge(root)
        with self.assertRaisesRegex(RuntimeError, "초과"):
            bridge._tk(lambda: None, timeout=0)
        entry = self.tracer._tk_calls[-1]
        self.assertEqual(entry["outcome"], "timeout")
        self.assertIsNone(entry["run_seconds"])
        # 예전에는 실행되지 않은 요청이 대기 목록에 남아 계속 멈춤으로 판정됐다.
        self.assertEqual(self.tracer._tk_pending, {})
        root.callbacks.get_nowait()()
        self.assertEqual(self.tracer._tk_pending, {})

    def test_shutdown_released_request_is_recorded_and_cleared(self):
        root = _QueuedRoot()
        bridge = self.bridge(root)
        errors = []

        def call():
            try:
                bridge._tk(lambda: None, timeout=None)
            except RuntimeError as exc:
                errors.append(str(exc))

        thread = Thread(target=call, daemon=True)
        thread.start()
        root.callbacks.get(timeout=2)
        tracer = self.tracer
        bridge._shutdown()
        thread.join(timeout=2)
        self.assertEqual(errors, ["앱이 종료 중입니다."])
        self.assertEqual(tracer._tk_pending, {})

    def test_rejected_request_while_closing_is_not_left_pending(self):
        bridge = self.bridge(_ImmediateRoot())
        bridge._closed = True
        with self.assertRaisesRegex(RuntimeError, "종료"):
            bridge._tk(lambda: None)
        self.assertEqual(self.tracer._tk_pending, {})

    def test_nothing_is_recorded_when_tracing_is_off(self):
        bridge = self.bridge(_ImmediateRoot(), tracing=False)
        self.assertEqual(bridge._tk(lambda: 3), 3)
        self.assertEqual(len(self.tracer._tk_calls), 0)
        self.assertEqual(self.tracer.tk_summary(), [])


if __name__ == "__main__":
    unittest.main()
