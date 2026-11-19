import json
import os
from pathlib import Path
import tempfile
import time
import unittest

from docfit_app.api_contract import ServiceError
from docfit_app.qms_bench_service import QmsBenchService


class FakeCompletedProcess:
    def __init__(self, stdout="", stderr="", returncode=0):
        self.stdout = stdout
        self.stderr = stderr
        self.returncode = returncode


class FakePopen:
    def __init__(self, stdout_lines=None, stderr="", returncode=0, delay=0.0):
        self.stdout_lines = stdout_lines or []
        self.stderr_text = stderr
        self.returncode = returncode
        self.delay = delay
        self._terminated = False
        self._start_time = time.time()
        self.stdout = iter(self.stdout_lines)

    @property
    def stderr(self):
        class StderrReader:
            def __init__(self, text):
                self.text = text
            def read(self):
                return self.text
        return StderrReader(self.stderr_text)

    def poll(self):
        if self._terminated:
            return -15
        if time.time() - self._start_time < self.delay:
            return None
        return self.returncode

    def wait(self, timeout=None):
        if self._terminated:
            return -15
        return self.returncode

    def terminate(self):
        self._terminated = True


class QmsBenchServiceTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.data_root = Path(self.temp_dir.name)
        self.fake_exe = self.data_root / "fake_qms_bench.exe"
        self.fake_exe.write_text("fake binary", encoding="utf-8")

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_detect_success(self):
        mock_report = {
            "ok": True,
            "command": "detect",
            "everything": {
                "installed": True,
                "running": True,
                "status": "gui_client_running",
                "guidance": "Everything GUI 클라이언트가 실행 중입니다.",
                "pids": [1234],
                "path": "C:\\Everything.exe",
                "version": "1.4.1",
                "db_loaded": True,
            },
            "kfind": {
                "running": True,
                "pids": [5678],
                "pipe_name": "\\\\.\\pipe\\kfind",
                "pipe_exists": True,
            },
        }

        def fake_runner(cmd, **kwargs):
            self.assertEqual(cmd, [str(self.fake_exe), "detect"])
            return FakeCompletedProcess(stdout=json.dumps(mock_report))

        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            runner=fake_runner,
        )
        report = svc.bench_detect()
        self.assertTrue(report["ok"])
        self.assertEqual(report["everything"]["status"], "gui_client_running")

    def test_detect_engine_missing(self):
        missing_exe = self.data_root / "nonexistent_bench.exe"
        svc = QmsBenchService(
            data_root=self.data_root,
            executable=missing_exe,
        )
        with self.assertRaises(ServiceError) as ctx:
            svc.bench_detect()
        self.assertEqual(ctx.exception.code, "ENGINE_MISSING")

    def test_start_developer_mode_required(self):
        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            developer_mode_checker=lambda: False,
        )
        with self.assertRaises(ServiceError) as ctx:
            svc.bench_start()
        self.assertEqual(ctx.exception.code, "DEVELOPER_MODE_REQUIRED")

    def test_start_and_status_progress(self):
        lines = [
            json.dumps({"event": "start", "run_id": "run-test-1", "adapter": "everything"}) + "\n",
            json.dumps({"event": "progress", "phase": "warm", "query_id": "q1", "repeat": 1, "latency_ms": 2.5, "result_count": 10, "status": "ok"}) + "\n",
            json.dumps({"event": "complete", "ok": True, "run_id": "run-test-1"}) + "\n",
        ]

        def fake_popen(cmd, **kwargs):
            return FakePopen(stdout_lines=lines, returncode=0)

        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            popen=fake_popen,
            developer_mode_checker=lambda: True,
        )
        res = svc.bench_start({"repeats": 5, "query_set_type": "default"})
        self.assertTrue(res["started"])
        self.assertEqual(res["repeats"], 5)

        # 상태 조회
        time.sleep(0.05)
        st = svc.bench_status()
        self.assertEqual(st["status"], "completed")
        self.assertEqual(len(st["events"]), 3)
        self.assertEqual(st["events"][1]["query_id"], "q1")

    def test_already_running_rejected(self):
        def fake_popen(cmd, **kwargs):
            return FakePopen(stdout_lines=[], delay=5.0)

        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            popen=fake_popen,
            developer_mode_checker=lambda: True,
        )
        svc.bench_start()
        with self.assertRaises(ServiceError) as ctx:
            svc.bench_start()
        self.assertEqual(ctx.exception.code, "ALREADY_RUNNING")
        svc.bench_cancel()

    def test_cancel(self):
        def fake_popen(cmd, **kwargs):
            return FakePopen(stdout_lines=[], delay=5.0)

        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            popen=fake_popen,
            developer_mode_checker=lambda: True,
        )
        svc.bench_start()
        c_res = svc.bench_cancel()
        self.assertTrue(c_res["cancelled"])
        self.assertEqual(svc.bench_status()["status"], "cancelled")

    def test_engine_failed(self):
        def fake_popen(cmd, **kwargs):
            return FakePopen(stdout_lines=[], stderr="Everything not running error", returncode=2)

        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            popen=fake_popen,
            developer_mode_checker=lambda: True,
        )
        svc.bench_start()
        time.sleep(0.05)
        st = svc.bench_status()
        self.assertEqual(st["status"], "failed")
        self.assertIn("Everything not running", st["error"])

    def test_path_traversal_rejected(self):
        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
        )
        with self.assertRaises(ServiceError) as ctx:
            svc._verify_path_inside_benchmarks("../../secret_folder")
        self.assertEqual(ctx.exception.code, "INVALID_PATH")

        with self.assertRaises(ServiceError) as ctx:
            svc.bench_open_folder({"folder": "C:/Windows"})
        self.assertEqual(ctx.exception.code, "INVALID_PATH")

    def test_open_folder_valid(self):
        opened_paths = []
        def fake_opener(path):
            opened_paths.append(path)

        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
            opener=fake_opener,
        )
        res = svc.bench_open_folder()
        self.assertTrue(res["opened"])
        self.assertEqual(len(opened_paths), 1)
        self.assertEqual(opened_paths[0], str(svc.benchmarks_dir))

    def test_list_results(self):
        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
        )
        # 결과 파일 생성
        res_file = svc.benchmarks_dir / "run-everything-123.json"
        res_file.write_text(
            json.dumps(
                {
                    "run": {"run_id": "run-everything-123", "adapter": "everything", "repeats": 10, "total_queries": 8},
                    "summary": {"completed": True},
                }
            ),
            encoding="utf-8",
        )

        res = svc.bench_list_results()
        self.assertEqual(len(res["results"]), 1)
        self.assertEqual(res["results"][0]["run_id"], "run-everything-123")
        self.assertEqual(res["results"][0]["adapter"], "everything")

    def test_execute_dispatcher(self):
        svc = QmsBenchService(
            data_root=self.data_root,
            executable=self.fake_exe,
        )
        # 지원하지 않는 액션
        with self.assertRaises(ServiceError) as ctx:
            svc.execute("unknown_action")
        self.assertEqual(ctx.exception.code, "INVALID_ACTION")


if __name__ == "__main__":
    unittest.main()
