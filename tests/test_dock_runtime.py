from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from docfit_app.application_service import ApplicationService
from docfit_app.dock_runtime import DockRuntime, bundled_exe


class FakeProcess:
    def __init__(self):
        self.running = True

    def poll(self):
        return None if self.running else 0


class DockRuntimeTests(unittest.TestCase):
    def test_start_ready_and_stop_use_injected_process_and_pipe(self):
        calls = []
        process = FakeProcess()

        def start_process(args, **kwargs):
            calls.append((args, kwargs))
            return process

        pipe_commands = []

        def pipe_request(command, timeout):
            pipe_commands.append((command, timeout))
            return {"ok": True, "ready": True}

        runtime = DockRuntime(
            executable=lambda: Path("resources/dock.exe"),
            process_start=start_process,
            pipe_request=pipe_request,
        )
        self.assertEqual(runtime.start(), {"started": True})
        self.assertEqual(calls[0][0], [str(Path("resources/dock.exe"))])
        self.assertFalse(calls[0][1]["shell"])
        self.assertTrue(runtime.ready(0.2))
        self.assertEqual(pipe_commands[0][0], "ready")
        self.assertEqual(runtime.stop(), {"stopped": True})
        self.assertEqual(pipe_commands[1][0], "quit")

    def test_missing_executable_does_not_start_a_process(self):
        runtime = DockRuntime(executable=lambda: None, process_start=lambda *a, **k: self.fail())
        with self.assertRaises(FileNotFoundError):
            runtime.start()

    def test_bundled_executable_uses_resources_directory(self):
        with TemporaryDirectory() as tmp:
            import unittest.mock
            with unittest.mock.patch("docfit_app.dock_runtime.sys") as mocked_sys:
                mocked_sys._MEIPASS = tmp
                target = Path(tmp) / "resources" / "dock.exe"
                target.parent.mkdir()
                target.touch()
                self.assertEqual(bundled_exe(), target)

    def test_application_api_registers_dock_lifecycle_commands(self):
        class FakeRuntime:
            def start(self): return {"started": True}
            def ready(self, timeout): return timeout == 1.5
            def stop(self): return {"stopped": True}

        with TemporaryDirectory() as tmp:
            service = ApplicationService(Path(tmp) / "settings.json")
            service.dock_runtime = FakeRuntime()
            self.assertEqual(service.dispatch("dock_start", []), {"started": True})
            self.assertTrue(service.dispatch("dock_ready", [1500]))
            self.assertEqual(service.dispatch("dock_stop", []), {"stopped": True})
            service.jobs.shutdown()


if __name__ == "__main__":
    unittest.main()
