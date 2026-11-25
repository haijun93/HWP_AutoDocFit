"""선행 단계의 backend 선택과 실패 시 기존 선택 보존 계약."""
import json
import os
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

from docfit_core.hwpx_fmt_backend import configured_processor


class BackendTests(unittest.TestCase):
    def setUp(self):
        self.calls = []

        def processor(source, target=None, selections=None):
            self.calls.append((source, target, selections))
            return {'Contents/section0.xml': [[0, 2]]} if target is None else 1

        processor.__name__ = '붙임_hwpx_처리'
        self.processor = processor

    def test_default_returns_original_function(self):
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '0'}):
            self.assertIs(configured_processor(self.processor), self.processor)

    def test_unrelated_processor_keeps_existing_path(self):
        self.processor.__name__ = '기본표서식_hwpx_처리'
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '1'}):
            self.assertIs(configured_processor(self.processor), self.processor)

    def test_missing_binary_falls_back_once_and_keeps_subset(self):
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '1', 'HWPX_FMT_BIN': ''}), patch('shutil.which', return_value=None):
            messages = []
            fn = configured_processor(self.processor, messages.append)
            fn('source')
            selected = {'Contents/section0.xml': []}
            self.assertEqual(fn('source', 'target', selected), 1)
            self.assertIs(self.calls[-1][2], selected)
            self.assertEqual(len(messages), 1)
            self.assertIn('python_fallback', messages[0])

    def test_unsupported_survey_stays_python_for_apply(self):
        completed = subprocess.CompletedProcess([], 4, '', '미지원 입력: 구조 변환')
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '1', 'HWPX_FMT_BIN': 'test.exe'}), patch('subprocess.run', return_value=completed) as invoke:
            fn = configured_processor(self.processor)
            selected = fn('source')
            fn('source', 'target', selected)
            self.assertEqual(invoke.call_count, 1)
            self.assertEqual(len(self.calls), 2)

    def test_native_success_keeps_python_out_of_processing(self):
        def invoke(command, **kwargs):
            self.assertIn('--config', command)
            config = json.loads(Path(command[-1]).read_text(encoding='utf-8'))
            self.assertIn('format_samples', config)
            if command[1] == 'survey':
                return subprocess.CompletedProcess(command, 0, '{"s":[[0,2]]}', '')
            Path(command[3]).write_bytes(b'complete result')
            return subprocess.CompletedProcess(command, 0, '{"count":1}', '')
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '1', 'HWPX_FMT_BIN': 'test.exe'}), patch('subprocess.run', side_effect=invoke), patch('shutil.copyfile') as copy:
            messages = []
            fn = configured_processor(self.processor, messages.append)
            selected = fn('source')
            self.assertEqual(fn('source', 'target', selected), 1)
            self.assertFalse(self.calls)
            copy.assert_called_once()
            self.assertTrue(any('rust_applied' in s for s in messages))

    def test_live_run_context_settings_are_forwarded(self):
        namespace = {'제목_담당자_글': '현재 실행의 담당자', '표준서식_설정': {'문두기호_역할': {'□': ''}}}
        def invoke(command, **kwargs):
            config = json.loads(Path(command[-1]).read_text(encoding='utf-8'))
            self.assertEqual(config['title_owner_text'], '현재 실행의 담당자')
            self.assertEqual(config['marker_roles'], {'□': ''})
            return subprocess.CompletedProcess(command, 0, '{}', '')
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '1', 'HWPX_FMT_BIN': 'test.exe'}), patch('subprocess.run', side_effect=invoke):
            fn = configured_processor(self.processor, namespace=namespace)
            self.assertEqual(fn('source'), {})

    def test_failed_apply_preserves_existing_selection(self):
        completed = subprocess.CompletedProcess([], 2, '', '처리 실패')
        selected = {'s': [[3, 1]]}
        with patch.dict(os.environ, {'DOCFIT_HWPXFMT5': '1', 'HWPX_FMT_BIN': 'test.exe'}), patch('subprocess.run', return_value=completed):
            fn = configured_processor(self.processor)
            self.assertEqual(fn('source', 'target', selected), 1)
            self.assertIs(self.calls[-1][2], selected)


if __name__ == '__main__':
    unittest.main()
