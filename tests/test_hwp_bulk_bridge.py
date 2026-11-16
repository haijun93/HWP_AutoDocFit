"""HWP → HWPX 일괄 변환 명령(2.0.7): 입력 검사, 진행 상태, 암호 비우기."""
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from desktop_web_ui import DesktopWebBridge, _BrowserApi


class HwpBulkBridgeTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.root = Path(self.folder.name)
        self.gui = Mock(files=[], running=False)
        self.bridge = DesktopWebBridge(self.gui)
        self.bridge._tk = Mock(side_effect=lambda callback, **kwargs: callback())

    def tearDown(self):
        self.folder.cleanup()

    def test_rejects_bad_input(self):
        for payload in ({}, {"roots": []}, {"roots": ["relative"]}, {"roots": [str(self.root / "없음")]},
                        {"roots": [str(self.root)], "passwords": [""]}, {"roots": [str(self.root)], "passwords": "pw"}):
            result = self.bridge.hwp_bulk("start", payload)
            self.assertFalse(result["ok"], payload)
        self.gui.running = True
        self.assertIn("다른 문서 작업", self.bridge.hwp_bulk("start", {"roots": [str(self.root)]})["message"])
        self.assertFalse(self.bridge.hwp_bulk("unknown")["ok"])

    def test_start_runs_in_background_and_clears_passwords(self):
        seen, finished = {}, threading.Event()

        def fake_run(roots, **kwargs):
            seen.update(roots=roots, passwords=list(kwargs["passwords"]), recursive=kwargs["recursive"])
            kwargs["state"].phase = "done"
            finished.set()
        passwords = ["pw1"]
        with patch("docfit_core.hwp_bulk.run_bulk", side_effect=fake_run):
            result = self.bridge.hwp_bulk("start", {"roots": [str(self.root)], "passwords": passwords,
                                                    "recursive": False})
            self.assertTrue(result["ok"])
            self.assertTrue(finished.wait(5))
        self.assertEqual(seen["roots"], [self.root.resolve()])
        self.assertEqual((seen["passwords"], seen["recursive"]), (["pw1"], False))
        for _ in range(50):
            if not passwords:
                break
            threading.Event().wait(0.05)
        self.assertEqual(passwords, [])  # 작업이 끝나면 암호를 버린다
        self.assertEqual(self.bridge.hwp_bulk("status")["bulk"]["phase"], "done")
        self.assertNotIn("pw1", repr(self.bridge.hwp_bulk("status")))

    def test_drives_and_browser_exposure(self):
        with patch("docfit_core.hwp_bulk.local_drives", return_value=["C:\\"]):
            self.assertEqual(self.bridge.hwp_bulk("drives")["drives"], ["C:\\"])
        self.assertTrue(callable(getattr(_BrowserApi(self.bridge), "hwp_bulk")))

    def test_retry_failed_action(self):
        # 1) failures가 없으면 에러
        res = self.bridge.hwp_bulk("retry_failed")
        self.assertFalse(res["ok"])
        self.assertIn("실패 문서가 없습니다", res["message"])

        # 2) failures가 있으면 해당 문서들만 다시 실행
        from docfit_core import hwp_bulk as bulk
        sample_hwp = self.root / "실패문서.hwp"
        sample_hwp.write_bytes(bulk.OLE_SIGNATURE + b"\0" * 32)
        state = self.bridge._bulk_state = bulk.BulkState(
            phase="done",
            failures=[{"source": str(sample_hwp), "message": "임시 실패", "kind": "잠김"}]
        )
        seen, finished = {}, threading.Event()

        def fake_run(roots, **kwargs):
            seen["roots"] = roots
            kwargs["state"].phase = "done"
            finished.set()

        with patch("docfit_core.hwp_bulk.run_bulk", side_effect=fake_run):
            res = self.bridge.hwp_bulk("retry_failed")
            self.assertTrue(res["ok"])
            self.assertTrue(finished.wait(5))
        self.assertEqual(seen["roots"], [sample_hwp.resolve()])

    def test_scan_all_counts_categories_and_reports_skips(self):
        from docfit_core import hwp_bulk
        rows = [
            {"path": "a.hwp", "kind": "normal"},
            {"path": "b.hwp", "kind": "password"},
            {"path": "c.hwp", "kind": "distributed"},
            {"path": "d.hwp", "kind": "drm"},
            {"path": "e.hwp", "kind": "hwp3"},
            {"path": "f.hwp", "kind": "corrupt"},
            {"path": "g.hwp", "kind": "existing", "file_kind": "normal"},
        ]
        summary = hwp_bulk.scan_all([self.root], runner=lambda roots: rows)
        self.assertEqual(summary["total"], 7)
        self.assertEqual(summary["counts"], {"normal": 1, "password": 1, "distributed": 1,
                         "drm": 1, "hwp3": 1, "corrupt": 1, "existing": 1})
        self.assertIn("windows", summary["skipped_folders"])
        self.assertEqual(summary["excluded_storage"], ["네트워크 드라이브", "클라우드 전용 파일"])

    def test_start_all_uses_fake_parallel_runner_and_action_allowlist(self):
        from docfit_core import hwp_bulk, hwp_bulk_parallel
        self.assertTrue({"scan_all", "start_all", "stop", "retry_failed", "open_report"}
                        <= hwp_bulk.HWP_BULK_ACTIONS)
        self.assertFalse(self.bridge.hwp_bulk("not_allowed") ["ok"])
        rows = [{"path": str(self.root / "normal.hwp"), "kind": "normal"},
                {"path": str(self.root / "protected.hwp"), "kind": "drm"}]
        with patch("docfit_core.hwp_bulk.scan_all", return_value={"total": 2,
                   "counts": {"normal": 1, "drm": 1}, "rows": rows, "roots": [str(self.root)],
                   "skipped_folders": [], "excluded_storage": [], "estimated_seconds": 2}):
            scan = self.bridge.hwp_bulk("scan_all", {"roots": [str(self.root)]})
        self.assertEqual(scan["summary"]["total"], 2)
        self.assertNotIn("rows", scan["summary"])
        seen, finished = {}, threading.Event()
        def fake_parallel(lines, **kwargs):
            seen["rows"] = [__import__("json").loads(line) for line in lines]
            kwargs["state"].total = 1
            kwargs["state"].done = kwargs["state"].succeeded = 1
            kwargs["state"].results = [hwp_bulk.BulkResult(rows[0]["path"], "success")]
            kwargs["on_progress"]({"done": 1, "total": 1, "succeeded": 1, "failed": 0,
                                   "skipped": 0, "current": "", "rate_per_minute": 30,
                                   "remaining_seconds": 0})
            finished.set()
            return kwargs["state"].results
        with patch("docfit_core.hwp_bulk_parallel.run_parallel", side_effect=fake_parallel), \
             patch("docfit_core.hwp_bulk.write_report", return_value=self.root / "report.csv"):
            started = self.bridge.hwp_bulk("start_all")
            self.assertTrue(started["ok"])
            self.assertTrue(finished.wait(5))
        self.assertEqual([row["kind"] for row in seen["rows"]], ["normal"])
        state = self.bridge.hwp_bulk("status")["bulk"]
        self.assertEqual(state["phase"], "done")
        self.assertEqual(state["protected"], 1)
        self.assertEqual(state["rate_per_minute"], 30)

    def test_retry_failed_all_computer_uses_parallel_runner(self):
        from docfit_core import hwp_bulk
        source = self.root / "failed.hwp"
        source.write_bytes(b"fake")
        self.bridge._bulk_state = hwp_bulk.BulkState(phase="done", scan_summary={"roots": [str(self.root)]},
             failures=[{"source": str(source), "message": "failed", "kind": "잠김"}])
        from types import SimpleNamespace
        header = SimpleNamespace(is_encrypted=False, is_distributed=False, is_drm=False, flags=1, version=0x05010001)
        retried, finished = [], threading.Event()
        def fake_parallel(lines, **kwargs):
            retried.extend(__import__("json").loads(x)["path"] for x in lines)
            kwargs["state"].results = []
            finished.set()
            return []
        with patch("docfit_core.hwp_bulk.read_hwp5_header", return_value=header), \
             patch("docfit_core.hwp_bulk_parallel.run_parallel", side_effect=fake_parallel), \
             patch("docfit_core.hwp_bulk.write_report", return_value=self.root / "retry.csv"):
            result = self.bridge.hwp_bulk("retry_failed")
            self.assertTrue(result["ok"])
            self.assertTrue(finished.wait(5))
        self.assertEqual(retried, [str(source)])


if __name__ == "__main__":
    unittest.main()
