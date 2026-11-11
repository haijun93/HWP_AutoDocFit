"""COM 없이 작업 수명의 현재 불일치를 고정한다."""
import ast
from pathlib import Path
import threading
import unittest

from docfit_app.api_contract import ServiceError
from docfit_app.jobs import JobManager, legacy_event, prepare_legacy
from types import SimpleNamespace


class LifecycleSymptomsTests(unittest.TestCase):
    def setUp(self):
        self.manager = JobManager(cancel_timeout=3600, cleanup=lambda: [])
        self.stop = threading.Event()
        self.identifier = self.manager.begin('spacing', stop_event=self.stop)
        self.manager.report(self.identifier, 'started', {})
        self.gui = SimpleNamespace(running=True, _job_manager=self.manager,
                                   _job_id=self.identifier)

    def tearDown(self):
        self.manager.report(self.identifier, 'cancelled', {})

    def lost_stop(self):
        self.manager.cancel(self.identifier)
        # 작업_실행 전체(COM 포함) 대신 실제 소스의 초기화 문장만 실행한다.
        source = Path(__file__).resolve().parents[1] / 'docfit_engine' / 'pipeline.py'
        tree = ast.parse(source.read_text(encoding='utf-8-sig'))
        clears = [node for node in ast.walk(tree) if isinstance(node, ast.Expr)
                  and isinstance(node.value, ast.Call)
                  and isinstance(node.value.func, ast.Attribute)
                  and isinstance(node.value.func.value, ast.Name)
                  and node.value.func.value.id == '중단_event'
                  and node.value.func.attr == 'clear']
        self.assertEqual(len(clears), 1)
        exec(compile(ast.Module(body=clears, type_ignores=[]), str(source), 'exec'),
             {'중단_event': self.stop})

    def cleanup_window(self):
        release = threading.Event()
        worker = threading.Thread(target=lambda: release.wait(5))
        worker.start()
        self.addCleanup(worker.join, 6)
        self.addCleanup(release.set)
        self.manager.attach(self.identifier, worker)
        self.gui.worker = worker
        legacy_event(self.manager, self.identifier, ('finished', 1, 0))
        self.gui.running = False  # Tk가 같은 finished 튜플을 소비한 상태

    def dead_worker(self):
        def exit_without_terminal():
            raise SystemExit('no terminal tuple')
        worker = threading.Thread(target=exit_without_terminal)
        self.manager.attach(self.identifier, worker)
        self.gui.worker = worker
        worker.start()
        worker.join(2)
        self.assertFalse(worker.is_alive())

    def test_current_stop_is_cleared_but_manager_reports_cancelled(self):
        self.lost_stop()
        self.assertFalse(self.stop.is_set())
        self.assertTrue(self.manager.status(self.identifier)['cancel_requested'])
        legacy_event(self.manager, self.identifier, ('finished', 1, 0))
        self.assertEqual(self.manager.status(self.identifier)['state'], 'cancelled')

    def test_current_finished_worker_still_blocks_restart(self):
        self.cleanup_window()
        self.assertEqual(self.manager.status(self.identifier)['state'], 'done')
        with self.assertRaises(ServiceError) as caught:
            self.manager.begin('spacing')
        self.assertEqual(caught.exception.code, 'JOB_RUNNING')

    def test_current_dead_worker_remains_running(self):
        self.dead_worker()
        prepare_legacy(self.gui)
        self.assertTrue(self.gui.running)
        self.assertEqual(self.manager.status(self.identifier)['state'], 'running')
