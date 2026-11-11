"""COM 없이 작업 수명의 현재 불일치를 고정한다."""
import ast
from pathlib import Path
import threading
import unittest

from docfit_app.api_contract import ServiceError
from docfit_app.jobs import JobManager, legacy_event, prepare_legacy
from types import SimpleNamespace
from docfit_app.jobs import job_phase


class LifecycleFixture(unittest.TestCase):
    def setUp(self):
        self.manager = JobManager(cancel_timeout=3600, cleanup=lambda: [])
        self.stop = threading.Event()
        self.identifier = self.manager.begin('spacing', stop_event=self.stop)
        self.manager.report(self.identifier, 'started', {})
        self.gui = SimpleNamespace(running=True, _job_manager=self.manager,
                                   _job_id=self.identifier)

    def tearDown(self):
        self.manager.report(self.identifier, 'cancelled', {})

    def lost_stop(self, preserve=False):
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
        self.assertEqual(len(clears), 0)
        exec(compile(ast.Module(body=clears, type_ignores=[]), str(source), 'exec'),
             {'중단_event': self.stop})
        if not preserve:
            # 읽기 전용 관측 시험의 소실된 Event 입력은 계속 유지한다.
            self.stop.clear()

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


class LifecycleSymptomsTests(LifecycleFixture):
    def test_stop_survives_pipeline_initialization(self):
        self.lost_stop(preserve=True)
        self.assertTrue(self.stop.is_set())
        self.assertTrue(self.manager.status(self.identifier)['cancel_requested'])
        legacy_event(self.manager, self.identifier, ('finished', 1, 0))
        self.assertEqual(self.manager.status(self.identifier)['state'], 'cancelled')

    def test_finished_dead_worker_allows_restart_despite_running_flag(self):
        self.dead_worker()
        legacy_event(self.manager, self.identifier, ('finished', 1, 0))
        prepare_legacy(self.gui)
        self.assertFalse(self.gui.running)
        self.assertEqual(job_phase(self.gui, self.manager), 'finished')
        second = self.manager.begin('spacing')
        self.manager.report(second, 'done', {})

    def test_new_job_clears_previous_stop_but_rejected_start_preserves_stop(self):
        self.manager.cancel(self.identifier)
        with self.assertRaises(ServiceError):
            self.manager.begin('spacing', stop_event=self.stop)
        self.assertTrue(self.stop.is_set())
        legacy_event(self.manager, self.identifier, ('stopped',))
        second = self.manager.begin('spacing', stop_event=self.stop)
        self.assertFalse(self.stop.is_set())
        self.manager.cancel(second)
        self.assertTrue(self.stop.is_set())
        self.manager.report(second, 'cancelled', {})

    def test_current_finished_worker_still_blocks_restart(self):
        self.cleanup_window()
        self.assertEqual(self.manager.status(self.identifier)['state'], 'done')
        with self.assertRaises(ServiceError) as caught:
            self.manager.begin('spacing')
        self.assertEqual(caught.exception.code, 'JOB_RUNNING')

    def test_dead_worker_is_failed_and_unlocks_gui(self):
        self.dead_worker()
        prepare_legacy(self.gui)
        self.assertFalse(self.gui.running)
        self.assertEqual(self.manager.status(self.identifier)['state'], 'failed')
        self.assertEqual(job_phase(self.gui, self.manager), 'failed')
        self.assertEqual(self.manager.job_status(self.identifier)['error']['code'], 'JOB_INTERRUPTED')


class LifecyclePhaseTests(LifecycleFixture):
    def test_phase_lost_stop_is_visible_and_observation_is_read_only(self):
        self.lost_stop()
        before = self.manager.status(self.identifier)
        events = self.manager.events()
        self.assertEqual(job_phase(self.gui, self.manager), 'stop_requested')
        self.assertEqual(self.manager.status(self.identifier), before)
        self.assertEqual(self.manager.events(), events)
        self.assertFalse(self.stop.is_set())

    def test_phase_cleanup_window(self):
        self.cleanup_window()
        self.assertEqual(job_phase(self.gui, self.manager), 'finishing')
        self.assertFalse(self.gui.running)
        self.assertEqual(self.manager.status(self.identifier)['state'], 'done')

    def test_phase_dead_worker(self):
        self.dead_worker()
        self.assertEqual(job_phase(self.gui, self.manager), 'dead')
        self.assertTrue(self.gui.running)

    def test_remaining_phases_and_light_job_does_not_hide_document(self):
        manager = JobManager()
        gui = SimpleNamespace(running=False)
        self.assertEqual(job_phase(gui, manager), 'idle')
        identifier = manager.begin('spacing')
        self.assertEqual(job_phase(gui, manager), 'starting')
        manager.report(identifier, 'started', {})
        gui.running = True
        light = manager.begin('preview')
        self.assertEqual(job_phase(gui, manager), 'running')
        gui.closing = True
        self.assertEqual(job_phase(gui, manager), 'closing')
        gui.closing = False
        gui.running = False
        manager.report(identifier, 'failed', {'code': 'TEST'})
        self.assertEqual(job_phase(gui, manager), 'failed')
        manager.report(light, 'done', {})
        second = manager.begin('spacing')
        manager.report(second, 'done', {})
        self.assertEqual(job_phase(gui, manager), 'finished')

    def test_get_state_adds_field_inside_existing_tk_snapshot(self):
        from tests.test_desktop_browser_api import _FakeGui, _bridge
        gui = _FakeGui()
        gui._job_manager = self.manager
        gui._job_id = self.identifier
        bridge = _bridge(gui)
        calls = []
        def tk(callback):
            calls.append('snapshot')
            return callback()
        bridge._tk = tk
        state = bridge.get_state()
        self.assertEqual(state['job_phase'], 'running')
        self.assertEqual(state['job'], self.manager.latest())
        self.assertFalse(state['running'])
        self.assertEqual(calls, ['snapshot'])
