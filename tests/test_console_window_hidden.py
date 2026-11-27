"""창 없는 앱이 콘솔 프로그램을 실행할 때 콘솔 창이 떴다 사라지지 않게 한다(OUTFLASH1)."""
import ast
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import tests  # noqa: F401 - 설치된 서비스·사용자 데이터를 읽지 않게 격리한다.
from docfit_app.outliner_service import OutlinerService

ROOT = Path(__file__).resolve().parents[1]
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)
# 콘솔 프로그램(outliner·pdf-edit·hierarchy·hwpx_fmt·pandoc·qms_bench)을 실행하는 파일.
CONSOLE_CALLERS = (
    "docfit_app/outliner_service.py", "docfit_app/pdf_edit_service.py", "docfit_app/hierarchy_bridge.py",
    "docfit_app/qms_bench_service.py", "docfit_core/hwpx_fmt_backend.py", "docfit_core/file_conversion.py",
)
LAUNCHERS = {"subprocess.run", "subprocess.Popen", "self.runner", "self.popen"}


class ConsoleWindowHiddenTests(unittest.TestCase):
    def test_outliner_engine_runs_without_a_console_window(self):
        done = mock.Mock(returncode=0, stdout=json.dumps({"ok": True, "result": {}}))
        runner = mock.Mock(return_value=done)
        with tempfile.TemporaryDirectory() as folder:
            executable = Path(folder) / "outliner.exe"
            executable.write_bytes(b"")
            OutlinerService(folder, executable=executable, runner=runner)._engine({"op": "load"})
        self.assertEqual(runner.call_args.kwargs.get("creationflags"), NO_WINDOW)

    def test_every_console_program_launch_hides_the_window(self):
        for name in CONSOLE_CALLERS:
            tree = ast.parse((ROOT / name).read_text(encoding="utf-8-sig"))
            calls = [node for node in ast.walk(tree)
                     if isinstance(node, ast.Call) and ast.unparse(node.func) in LAUNCHERS]
            self.assertTrue(calls, name)
            for call in calls:
                # 창 프로그램(outliner_win.exe)을 켜는 self.launcher 는 대상이 아니다.
                with self.subTest(file=name, line=call.lineno):
                    self.assertIn("creationflags", {keyword.arg for keyword in call.keywords})


if __name__ == "__main__":
    unittest.main()
