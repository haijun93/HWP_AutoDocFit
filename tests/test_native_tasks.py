"""Rust 작업 프로세스: 합성 자료만 사용하며 설치본/사용자 설정/서비스를 변경하지 않는다."""
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

from docfit_app.native_tasks import Session, serve, MAX_LINE


class NativeTasksTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.session = Session(self.root / 'settings')
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(self.session.close)

    def call(self, action, **args):
        return self.session.dispatch(dict(op='rename', payload=dict(action=action, args=args)))

    def wait(self, identifier):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            status = self.session.dispatch(dict(op='job_status', payload=dict(job_id=identifier)))
            if status['state'] in {'done', 'failed', 'cancelled'}:
                worker = self.session.jobs.thread(identifier)
                worker.join(2)
                return status
            time.sleep(.01)
        self.fail('작업이 끝나지 않았습니다.')

    def test_advanced_rename_preview_apply_history_undo(self):
        a, b = self.root / '보고서 9.txt', self.root / '보고서 10.txt'
        a.write_text('합성 A', encoding='utf-8')
        b.write_text('합성 B', encoding='utf-8')
        self.assertEqual(len(self.call('add_paths', paths=[str(a), str(b)])['entries']), 2)
        rules = [dict(kind='number_shift', delta=2, which='last'), dict(kind='cleanup', underscore='from_space')]
        preview = self.call('preview', rules=rules)
        self.assertEqual([r['new_name'] for r in preview['items']], ['보고서_11.txt', '보고서_12.txt'])
        job = self.call('apply', plan_id=preview['plan_id'])
        self.assertEqual(self.wait(job['job_id'])['state'], 'done')
        self.assertEqual((self.root / '보고서_11.txt').read_text(encoding='utf-8'), '합성 A')
        history = self.call('history')['runs']
        self.assertEqual(len(history), 1)
        self.assertEqual(self.wait(self.call('undo', run_id=history[0]['id'])['job_id'])['state'], 'done')
        self.assertEqual(a.read_text(encoding='utf-8'), '합성 A')
        self.assertEqual(b.read_text(encoding='utf-8'), '합성 B')

    def test_stale_preview_is_rejected(self):
        source = self.root / 'a.txt'
        source.write_text('합성', encoding='utf-8')
        self.call('add_paths', paths=[str(source)])
        preview = self.call('preview', rules=[dict(kind='add', text='시험_')])
        self.call('clear')
        with self.assertRaisesRegex(ValueError, '미리보기'):
            self.call('apply', plan_id=preview['plan_id'])
        self.assertTrue(source.exists())

    def test_rulesets_roundtrip_and_disabled_rule(self):
        rules = [dict(kind='case', mode='upper'), dict(kind='clear', enabled=False)]
        self.assertTrue(self.call('ruleset_save', name='합성', rules=rules)['ok'])
        exported = self.call('ruleset_export', name='합성')
        self.call('ruleset_delete', name='합성')
        self.assertTrue(self.call('ruleset_import', json=exported['json'])['ok'])
        saved = next(s for s in self.call('ruleset_list')['rulesets'] if s['name'] == '합성')
        self.assertEqual(saved['rules'], rules)

    def test_controlled_services_only_run_when_requested(self):
        self.session.kvf = Mock()
        self.session.kvf.command.return_value = dict(ok=True, running=False)
        self.session.idioms = Mock()
        self.session.idioms.command.return_value = dict(text=[], body=[])
        self.session.dispatch(dict(op='about'))
        self.session.kvf.command.assert_not_called()
        self.session.idioms.command.assert_not_called()
        self.session.dispatch(dict(op='kvf', payload=dict(action='status')))
        self.session.kvf.command.assert_called_once_with('status', {})
        self.session.dispatch(dict(op='idioms', payload=dict(action='get')))
        self.session.idioms.command.assert_called_once_with('get', {})
        with self.assertRaises(ValueError):
            self.session.dispatch(dict(op='kvf', payload=dict(action='install')))

    def test_runtime_settings_apply_service_only_for_changed_search_and_keep_other_keys(self):
        store = self.root / 'settings' / 'settings.json'
        store.parent.mkdir(parents=True)
        store.write_text('{"other":42,"mongttang_search":false}', encoding='utf-8')
        self.session.ktm = Mock()
        self.session.ktm.apply_setting.return_value = dict(ok=True)
        initial = self.session.dispatch(dict(op='runtime_settings'))['values']
        self.session.ktm.apply_setting.assert_not_called()
        wanted = dict(initial, always_on_top=False, mongttang_search=True)
        response = self.session.dispatch(dict(op='runtime_settings', payload=dict(action='set', values=wanted)))
        self.assertEqual(response['values'], wanted)
        self.session.ktm.apply_setting.assert_called_once_with(True)
        self.assertEqual(json.loads(store.read_text(encoding='utf-8'))['other'], 42)
        # 다른 창이 저장한 뒤에는 오래된 초안으로 서비스 상태를 바꾸지 않는다.
        store.write_text('{"mongttang_search":false,"always_on_top":false}', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, '다른 창'):
            self.session.dispatch(dict(op='runtime_settings', payload=dict(action='set', values=wanted)))
        self.session.ktm.apply_setting.assert_called_once()

    def test_runtime_service_failure_does_not_save_desired_setting(self):
        self.session.ktm = Mock()
        self.session.ktm.apply_setting.return_value = dict(ok=False, error='합성 실패')
        initial = self.session.dispatch(dict(op='runtime_settings'))['values']
        with self.assertRaisesRegex(ValueError, '합성 실패'):
            self.session.dispatch(dict(op='runtime_settings', payload=dict(action='set', values=dict(initial, mongttang_search=False))))
        self.assertFalse((self.root / 'settings' / 'settings.json').exists())

    def test_pdf_target_size_reaches_compression_engine(self):
        from docfit_app.conversion_runner import run_conversion
        source = self.root / '합성.pdf'
        source.write_bytes(b'synthetic-pdf-engine-mock')
        compressed = Mock(after=100, step=1, method='preserve_text', text_preserved=True, reached_target=True)
        compressed.summary.return_value = '압축'
        with patch('docfit_core.pdf_compress.compress_pdf', return_value=compressed) as compress:
            result = run_conversion([source], 'pdfz', pdf_target_bytes=3 * 1024 * 1024)
        self.assertEqual(compress.call_args.kwargs['target_bytes'], 3 * 1024 * 1024)
        self.assertEqual(result[0]['status'], 'success')

    def test_conversion_returns_actual_noncolliding_outputs_without_com(self):
        from pypdf import PdfWriter
        source = self.root / '합성.pdf'
        writer = PdfWriter()
        writer.add_blank_page(width=100, height=100)
        with source.open('wb') as stream:
            writer.write(stream)
        digest = source.read_bytes()
        # 같은 형식은 성공을 꾸며 내지 않고 skipped 사유를 전달한다.
        job = self.session.dispatch(dict(op='convert', payload=dict(paths=[str(source)], target='pdf')))
        done = self.wait(job['job_id'])
        self.assertEqual(done['result'][0]['status'], 'skipped')
        self.assertIn('이미', done['result'][0]['error'])
        self.assertEqual(source.read_bytes(), digest)

    def test_pdf_merge_keeps_input_order_and_actual_output(self):
        from pypdf import PdfReader, PdfWriter
        paths = []
        for index, width in enumerate([123, 234]):
            source = self.root / f'{index}.pdf'
            writer = PdfWriter()
            writer.add_blank_page(width=width, height=100)
            with source.open('wb') as stream:
                writer.write(stream)
            paths.append(str(source))
        job = self.session.dispatch(dict(op='convert', payload=dict(paths=paths, target='pdfm')))
        done = self.wait(job['job_id'])
        self.assertEqual([row['source'] for row in done['result']], paths)
        output = done['result'][0]['output']
        self.assertNotIn(output, paths)
        self.assertEqual([int(p.mediabox.width) for p in PdfReader(output).pages], [123, 234])
        self.assertEqual(done['result'][1]['output'], output)

    def test_cancel_is_cooperative_and_does_not_allow_overlapping_edits(self):
        started = __import__('threading').Event()
        def fake_convert(*args, stop_event=None, **kwargs):
            started.set()
            while not stop_event.is_set():
                time.sleep(.002)
            return [dict(source=None, status='cancelled', output=None, error='중단')]
        source = self.root / '합성.txt'
        source.write_text('합성', encoding='utf-8')
        with patch('docfit_app.conversion_runner.run_conversion', side_effect=fake_convert):
            job = self.session.dispatch(dict(op='convert', payload=dict(paths=[str(source)], target='md')))
            self.assertTrue(started.wait(2))
            with self.assertRaisesRegex(ValueError, '진행 중'):
                self.call('clear')
            self.session.dispatch(dict(op='cancel', payload=dict(job_id=job['job_id'])))
            done = self.wait(job['job_id'])
            self.assertEqual(done['state'], 'cancelled')
            self.assertEqual(done['result'][0]['status'], 'cancelled')
        self.assertEqual(source.read_text(encoding='utf-8'), '합성')

    def test_protocol_recovers_from_bad_json_and_rejects_unlisted_commands(self):
        writer = io.StringIO()
        reader = io.StringIO('bad json\n' + json.dumps(dict(op='arbitrary_shell')) + '\n' + json.dumps(dict(op='about')) + '\n')
        serve(reader, writer, self.session)
        replies = [json.loads(line) for line in writer.getvalue().splitlines()]
        self.assertEqual([r['ok'] for r in replies], [False, False, True])

    def test_oversize_request_does_not_execute_remaining_fragments(self):
        writer = io.StringIO()
        serve(io.StringIO(' ' * (MAX_LINE + 1) + '\n{"op":"about"}\n'), writer, self.session)
        self.assertEqual(len(writer.getvalue().splitlines()), 1)

    def test_early_bootstrap_runs_without_tk_or_com(self):
        env = dict(os.environ, PYTHONIOENCODING='utf-8')
        result = subprocess.run([sys.executable, 'hwp-auto-docfit.py', '--native-tasks', '--data-dir', str(self.root)],
            input='{"op":"about"}\n', capture_output=True, text=True, encoding='utf-8', env=env, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['ui'], 'Rust + Win32')


if __name__ == '__main__':
    unittest.main()
