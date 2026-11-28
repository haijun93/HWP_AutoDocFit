"""OUTENGINE1: real child pipes, failure injection, and optional Rust integration."""
import concurrent.futures
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import tests  # isolate all profile/settings access before importing services
from docfit_app.api_contract import ServiceError
from docfit_app.outliner_engine import OutlinerEngine
from docfit_app.outliner_service import OutlinerService


class TransportTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.exe = self.root / 'outliner.exe'
        self.exe.write_bytes(b'synthetic engine marker')
        self.log = self.root / 'requests.jsonl'
        self.processes = []
        self.launch_kwargs = []
        self.singles = 0

    def engine(self, modes=('good',), timeout=5):
        modes = iter(modes)

        def command(mode, ready=False):
            return [sys.executable, '-u', str(Path(__file__).resolve().parent / 'fixtures' /
                                           'outliner_transport_child.py'), mode, str(self.log)] + (['ready'] if ready else [])

        def popen(args, **kwargs):
            self.launch_kwargs.append((args, kwargs))
            mode = next(modes, 'good')
            if mode == 'start_fail':
                raise OSError('synthetic start failure')
            process = subprocess.Popen(command(mode, ready=True), **kwargs)
            self.processes.append(process)
            # Python/antivirus startup is test preparation. Inject failures only
            # after the synthetic peer is ready, so a short response deadline
            # actually tests a stalled request rather than interpreter startup.
            greeting = queue.Queue()
            threading.Thread(target=lambda: greeting.put(process.stdout.readline()), daemon=True).start()
            self.assertEqual(greeting.get(timeout=15), 'READY\n')
            return process

        def run(args, **kwargs):
            self.singles += 1
            return subprocess.run(command('good'), **kwargs)

        engine = OutlinerEngine(self.exe, popen=popen, runner=run, timeout=timeout,
                                runtime_parent=self.root)
        self.addCleanup(engine.close)
        return engine

    def requests(self):
        return [json.loads(line) for line in self.log.read_text(encoding='utf-8').splitlines()]

    def test_reuses_one_hidden_process_and_checks_each_concurrent_response(self):
        engine = self.engine()
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            values = list(pool.map(lambda n: engine.request(
                {'op': 'parse', 'document': f'문서 {n}'})['result']['document'], range(30)))
        self.assertEqual(values, [f'문서 {n}' for n in range(30)])
        self.assertEqual(len(self.processes), 1)
        self.assertEqual(len({r['id'] for r in self.requests()}), 30)
        args, kwargs = self.launch_kwargs[0]
        self.assertNotEqual(Path(args[0]), self.exe)
        self.assertEqual(kwargs['creationflags'], getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        self.assertTrue(kwargs['close_fds'])
        self.assertEqual(engine.state, 'ready')

    def test_mid_request_exit_broken_response_and_wrong_id_restart_once(self):
        for mode in ('exit', 'broken', 'mismatch', 'envelope'):
            with self.subTest(mode=mode):
                engine = self.engine((mode, 'good'))
                self.assertEqual(engine.request({'op': 'parse', 'document': mode})['result']['document'], mode)
                self.assertEqual(engine.state, 'ready')
                engine.close()
        self.assertEqual(len(self.processes), 8)
        self.assertEqual(self.singles, 0)

    def test_two_failures_use_single_shot_and_stay_in_fallback(self):
        engine = self.engine(('broken', 'mismatch'))
        self.assertEqual(engine.request({'op': 'parse', 'document': 'fallback'})['result']['document'], 'fallback')
        self.assertEqual(engine.state, 'fallback')
        engine.request({'op': 'parse'})
        self.assertEqual(len(self.processes), 2)
        self.assertEqual(self.singles, 2)

    def test_timeout_covers_blocked_input_write_and_discards_old_reply(self):
        engine = self.engine(('timeout', 'good'), timeout=3)
        started = time.monotonic()
        text = '가' * (512 * 1024)
        self.assertEqual(engine.request({'op': 'parse', 'document': text})['result']['document'], text)
        self.assertLess(time.monotonic() - started, 20)
        self.assertIsNotNone(self.processes[0].poll())
        self.assertEqual(len(self.processes), 2)

    def test_timeout_when_child_does_not_read_stdin(self):
        engine = self.engine(('good',), timeout=3)
        # A child that never reads: filling stdin must also have a deadline.
        original = engine.popen

        def no_read(args, **kwargs):
            if not self.processes:
                process = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'], **kwargs)
                self.processes.append(process)
                return process
            return original(args, **kwargs)

        engine.popen = no_read
        text = 'x' * (512 * 1024)
        self.assertEqual(engine.request({'op': 'parse', 'document': text})['result']['document'], text)
        self.assertIsNotNone(self.processes[0].poll())

    def test_dead_idle_child_is_replaced_before_a_write_is_dispatched(self):
        engine = self.engine()
        engine.request({'op': 'parse'})
        self.processes[0].kill()
        self.processes[0].wait()
        engine.request({'op': 'edit', 'document': 'new'})
        self.assertEqual(len(self.processes), 2)
        self.assertEqual([r['op'] for r in self.requests()], ['parse', 'edit'])

    def test_duplicate_response_cannot_be_returned_to_another_request(self):
        engine = self.engine(('double', 'good'))
        engine.request({'op': 'parse', 'document': 'first'})
        result = engine.request({'op': 'parse', 'document': 'second'})
        self.assertEqual(result['result']['document'], 'second')
        self.assertEqual(len(self.processes), 2)

    def test_engine_application_error_is_not_retried(self):
        engine = self.engine(('error',))
        self.assertFalse(engine.request({'op': 'search'})['ok'])
        self.assertEqual(len(self.processes), 1)
        self.assertEqual(self.singles, 0)

    def test_ambiguous_write_is_applied_once_then_only_probe_is_sent(self):
        for mode in ('write_exit', 'timeout', 'broken', 'mismatch'):
            with self.subTest(mode=mode):
                self.log.unlink(missing_ok=True)
                engine = self.engine((mode, 'good'), timeout=3)
                target = self.root / 'synthetic.md'
                with self.assertRaises(ServiceError) as raised:
                    engine.request({'op': 'save', 'args': {'path': str(target), 'markdown': '- saved\n'}})
                self.assertEqual(raised.exception.code, 'OUTLINER_ENGINE_UNCERTAIN')
                ops = [r['op'] for r in self.requests()]
                self.assertEqual(ops, ['save', 'parse'])
                if mode != 'timeout':
                    self.assertEqual(target.read_text(encoding='utf-8'), '- saved\n')
                self.assertEqual(self.singles, 0)
                engine.close()

    def test_failed_write_and_failed_probe_degrade_without_replaying_write(self):
        engine = self.engine(('write_exit', 'broken'))
        with self.assertRaises(ServiceError):
            engine.request({'op': 'rename_page'})
        self.assertEqual(engine.state, 'fallback')
        self.assertEqual([r['op'] for r in self.requests()], ['rename_page', 'parse'])
        self.assertEqual(self.singles, 0)
        engine.request({'op': 'parse'})
        self.assertEqual(self.singles, 1)

    def test_start_failure_can_fallback_without_duplicate_writes(self):
        engine = self.engine(('start_fail', 'start_fail'))
        engine.request({'op': 'edit'})
        self.assertEqual(self.singles, 1)
        self.assertEqual([r['op'] for r in self.requests()], ['edit'])

    def test_close_reaps_child_removes_private_copy_and_is_terminal(self):
        engine = self.engine()
        engine.request({'op': 'parse'})
        private = Path(self.launch_kwargs[0][0][0])
        engine.close()
        engine.close()
        self.assertFalse(private.exists())
        self.assertIsNotNone(self.processes[0].poll())
        self.assertEqual(engine.state, 'closed')
        with self.assertRaises(ServiceError):
            engine.request({'op': 'parse'})

    def test_close_interrupts_pending_request_without_restarting(self):
        engine = self.engine(('timeout',), timeout=10)
        with concurrent.futures.ThreadPoolExecutor() as pool:
            pending = pool.submit(engine.request, {'op': 'parse'})
            deadline = time.monotonic() + 5
            while not self.log.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            engine.close()
            with self.assertRaises(ServiceError) as raised:
                pending.result(timeout=3)
        self.assertEqual(raised.exception.code, 'OUTLINER_ENGINE_CLOSED')
        self.assertEqual(len(self.processes), 1)

    def test_uncertain_rename_does_not_roll_back_the_page_name(self):
        service = OutlinerService(self.root, executable=self.exe, persistent=False)
        source = service.root / 'Old.md'
        source.write_text('- old\n', encoding='utf-8')
        with mock.patch.object(service, '_engine', side_effect=ServiceError('OUTLINER_ENGINE_UNCERTAIN', 'uncertain')):
            with self.assertRaises(ServiceError):
                service.execute({'action': 'rename', 'page': 'Old', 'new_name': 'New'})
        self.assertFalse(source.exists())
        self.assertTrue((service.root / 'New.md').exists())

    def test_closed_service_rejects_file_changes_before_starting_another_request(self):
        service = OutlinerService(self.root, executable=self.exe)
        service.close()
        with self.assertRaises(ServiceError):
            service.execute({'action': 'create', 'page': 'AfterClose'})
        self.assertFalse((service.root / 'AfterClose.md').exists())


@unittest.skipUnless(os.environ.get('OUTLINER_TEST_EXE'), 'explicit synthetic release engine required')
class RustTransportTest(unittest.TestCase):
    def test_parent_interpreter_exit_reaps_engine_before_deleting_private_executable(self):
        with tempfile.TemporaryDirectory() as folder:
            code = (
                'import json,sys; from docfit_app.outliner_service import OutlinerService; '
                's=OutlinerService(sys.argv[1],executable=sys.argv[2],runtime_parent=sys.argv[1]); '
                's.execute({"action":"parse","page":"Home","markdown":"- synthetic\\n"}); '
                'print(json.dumps({"private":s._transport._resources["directory"].name}))'
            )
            result = subprocess.run([sys.executable, '-c', code, folder, os.environ['OUTLINER_TEST_EXE']],
                                    capture_output=True, text=True, encoding='utf-8', timeout=30,
                                    creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr, '')
            self.assertFalse(Path(json.loads(result.stdout)['private']).exists())

    def test_save_load_edit_backlinks_equal_single_shot_and_original_is_replaceable(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            packaged = root / 'outliner.exe'
            import shutil
            shutil.copy2(os.environ['OUTLINER_TEST_EXE'], packaged)
            service = OutlinerService(root / 'data', executable=packaged, runtime_parent=root)
            single = OutlinerService(root / 'single', executable=packaged, persistent=False)
            try:
                for action in ('parse', 'edit', 'backlinks'):
                    request = {'action': action, 'page': 'Home', 'markdown': '- 한글 [[Home]]\n',
                               'operation': 'set_text', 'block_id': 1, 'text': '수정 [[Home]]', 'target': 'Home'}
                    self.assertEqual(service.execute(request), single.execute(request))
                loaded = service.execute({'action': 'load', 'page': 'Home'})
                service.execute({'action': 'save', 'page': 'Home', 'version': loaded['version'], 'markdown': '- 저장\n'})
                self.assertEqual(service.execute({'action': 'load', 'page': 'Home'})['markdown'], '- 저장\n')
                # Windows would refuse this if the packaged EXE itself were in use.
                packaged.rename(root / 'replaced.exe')
                shutil.copy2(root / 'replaced.exe', packaged)
                self.assertEqual(service.execute({'action': 'parse', 'page': 'Home', 'markdown': '- 교체\n'})['blocks'][0]['text'], '교체')
                process = service._transport._resources['process']
                private = Path(service._transport._resources['directory'].name)
            finally:
                service.close()
                single.close()
            self.assertIsNotNone(process.poll())
            self.assertFalse(private.exists())


if __name__ == '__main__':
    unittest.main()
