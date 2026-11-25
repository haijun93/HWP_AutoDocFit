import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.api_contract import ServiceError
from docfit_app.outliner_service import OutlinerService


class OutlinerServiceTest(unittest.TestCase):
    def test_paths_are_confined_to_outliner_data_folder(self):
        with tempfile.TemporaryDirectory() as folder:
            service = OutlinerService(folder, executable=Path(folder) / 'engine.exe')
            with self.assertRaises(ServiceError):
                service.execute({'action': 'load', 'page': '../settings'})

    def test_save_rejects_changed_source(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            executable = root / 'engine.exe'
            executable.touch()
            page = root / 'Outliner' / 'Notes.md'
            page.parent.mkdir()
            page.write_text('- initial\n', encoding='utf-8')
            service = OutlinerService(root, executable=executable)
            original = service._digest(page)
            page.write_text('- changed elsewhere\n', encoding='utf-8')
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'save', 'page': 'Notes', 'version': original, 'markdown': '- mine\n'})
            self.assertEqual(raised.exception.code, 'OUTLINER_CONFLICT')

    def test_cli_contract_and_application_registration(self):
        with tempfile.TemporaryDirectory() as folder:
            executable = Path(folder) / 'engine.exe'
            executable.touch()
            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {'markdown': '- Hi\n', 'document': {'blocks': []}}})
            runner = unittest.mock.Mock(return_value=Completed())
            with patch('docfit_app.outliner_service.subprocess.run', return_value=Completed()):
                service = OutlinerService(folder, executable=executable, runner=runner)
                result = service.execute({'action': 'load', 'page': 'Home'})
                self.assertEqual(result['markdown'], '- Hi\n')
                self.assertEqual(json.loads(runner.call_args.kwargs['input'])['op'], 'load')

    def test_edit_operations_are_sent_to_the_engine_as_op_edit_with_args_action(self):
        """set_text·insert·delete 는 op='edit' 아래 args.action 으로 보낸다('지원하지 않는 명령: set_text' 사고, v2.2.83)."""
        with tempfile.TemporaryDirectory() as folder:
            executable = Path(folder) / 'engine.exe'
            executable.touch()
            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {'markdown': '- x\n', 'document': {'blocks': []}}})
            runner = unittest.mock.Mock(return_value=Completed())
            service = OutlinerService(folder, executable=executable, runner=runner)
            for operation in ('set_text', 'insert', 'delete'):
                service.execute({'action': 'edit', 'operation': operation, 'page': 'Home',
                                 'markdown': '- a\n', 'block_id': 1, 'text': 'x'})
                sent = json.loads(runner.call_args.kwargs['input'])
                self.assertEqual(sent['op'], 'edit')
                self.assertEqual(sent['args']['action'], operation)
                self.assertEqual(sent['args']['block_id'], 1)
                self.assertEqual(sent['args']['text'], 'x')
                self.assertNotIn('operation', sent['args'])


class FakeSettings:
    def __init__(self):
        self.values = {}

    def read(self):
        return dict(self.values)

    def update(self, change):
        change(self.values)


class OutlinerNativeWindowTest(unittest.TestCase):
    """설정 outliner_ui('flutter' 기본 | 'native')와 Win32 창(outliner_win.exe) 실행 — 앱 쪽에는 켜기만 있다."""

    def _service(self, folder, **kw):
        executable = Path(folder) / 'engine.exe'
        executable.touch()
        return OutlinerService(folder, executable=executable, **kw)

    def test_history_requests_map_operation_to_engine_args_action_and_keep_page(self):
        """실행 취소(OUTLINERUNDO1): history 는 op='history', args.action=operation, page 유지."""
        with tempfile.TemporaryDirectory() as folder:
            executable = Path(folder) / 'engine.exe'
            executable.touch()
            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {'markdown': '- a\n', 'can_undo': False}})
            runner = unittest.mock.Mock(return_value=Completed())
            service = OutlinerService(folder, executable=executable, runner=runner)
            result = service.execute({'action': 'history', 'operation': 'undo', 'page': 'Home',
                                      'markdown': '- b\n', 'version': 'v1', 'history': None})
            sent = json.loads(runner.call_args.kwargs['input'])
            self.assertEqual(sent['op'], 'history')
            self.assertEqual(sent['document'], '- b\n')
            self.assertEqual(sent['args']['action'], 'undo')
            self.assertEqual(sent['args']['page'], 'Home')
            self.assertEqual(result['markdown'], '- a\n')

    def test_ui_mode_defaults_to_flutter_saves_native_and_rejects_other_values(self):
        with tempfile.TemporaryDirectory() as folder:
            settings = FakeSettings()
            service = self._service(folder, settings=settings)
            self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})
            self.assertEqual(service.execute({'action': 'ui_mode', 'value': 'native'}), {'mode': 'native'})
            self.assertEqual(settings.values['outliner_ui'], 'native')
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'ui_mode', 'value': 'tk'})
            self.assertEqual(raised.exception.code, 'INVALID_INPUT')
            self.assertEqual(settings.values['outliner_ui'], 'native')
            settings.values['outliner_ui'] = 'garbage'
            self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})

    def test_ui_mode_without_settings_store_is_a_service_error(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(ServiceError):
                self._service(folder).execute({'action': 'ui_mode'})

    def test_open_native_starts_the_window_with_the_outliner_data_folder(self):
        with tempfile.TemporaryDirectory() as folder:
            exe = Path(folder) / 'outliner_win.exe'
            exe.touch()
            launcher = unittest.mock.Mock()
            service = self._service(folder, native_executable=exe, launcher=launcher)
            self.assertEqual(service.execute({'action': 'open_native'}), {'opened': True})
            args = launcher.call_args.args[0]
            self.assertEqual(args[0], str(exe))
            self.assertEqual(args[1], '--data-dir')
            self.assertEqual(Path(args[2]), Path(folder).resolve() / 'Outliner')

    def test_open_native_reports_missing_program_and_launch_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            service = self._service(folder, native_executable=Path(folder) / 'none.exe', launcher=unittest.mock.Mock())
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'open_native'})
            self.assertEqual(raised.exception.code, 'OUTLINER_WINDOW_MISSING')
            exe = Path(folder) / 'outliner_win.exe'
            exe.touch()
            service = self._service(folder, native_executable=exe, launcher=unittest.mock.Mock(side_effect=OSError('x')))
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'open_native'})
            self.assertEqual(raised.exception.code, 'OUTLINER_WINDOW_FAILED')

    def test_setting_is_registered_with_a_flutter_default_and_excluded_from_exports(self):
        from docfit_app.configuration_service import EXCLUDED
        from docfit_app.settings_schema import DEFAULTS
        self.assertEqual(DEFAULTS['outliner_ui'], 'flutter')
        self.assertIn('outliner_ui', EXCLUDED)


class OutlinerPageManagementTest(unittest.TestCase):
    """OUTLINERREF1: 페이지 관리(이름 바꾸기·삭제·복원) 및 블록 참조 연동."""

    def test_rename_page_and_undo_rename(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            executable = root / 'engine.exe'
            executable.touch()
            outliner_dir = root / 'Outliner'
            outliner_dir.mkdir(parents=True)
            p1 = outliner_dir / 'OldPage.md'
            p1.write_text('- content\n', encoding='utf-8')
            p2 = outliner_dir / 'OtherPage.md'
            p2.write_text('- [[OldPage]] link\n', encoding='utf-8')

            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {'markdown': '- content\n', 'document': {'blocks': []}, 'count': 1}})

            runner = unittest.mock.Mock(return_value=Completed())
            service = OutlinerService(root, executable=executable, runner=runner)
            res = service.execute({'action': 'rename', 'page': 'OldPage', 'new_name': 'NewPage'})
            self.assertEqual(res['page'], 'NewPage')
            self.assertTrue((outliner_dir / 'NewPage.md').exists())
            self.assertFalse((outliner_dir / 'OldPage.md').exists())

            # undo_rename
            res_undo = service.execute({'action': 'undo_rename'})
            self.assertEqual(res_undo['page'], 'OldPage')
            self.assertTrue((outliner_dir / 'OldPage.md').exists())
            self.assertFalse((outliner_dir / 'NewPage.md').exists())

    def test_delete_page_trash_and_restore(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            executable = root / 'engine.exe'
            executable.touch()
            outliner_dir = root / 'Outliner'
            outliner_dir.mkdir(parents=True)
            p = outliner_dir / 'MyPage.md'
            p.write_text('- delete target\n', encoding='utf-8')

            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {'markdown': '- delete target\n', 'document': {'blocks': []}}})

            runner = unittest.mock.Mock(return_value=Completed())
            service = OutlinerService(root, executable=executable, runner=runner)
            res = service.execute({'action': 'delete', 'page': 'MyPage'})
            self.assertTrue(res['deleted'])
            self.assertFalse(p.exists())
            trash_files = list((outliner_dir / '.trash').glob('MyPage.*.md'))
            self.assertEqual(len(trash_files), 1)

            # restore
            res_res = service.execute({'action': 'restore'})
            self.assertEqual(res_res['page'], 'MyPage')
            self.assertTrue(p.exists())

    def test_check_delete_page_references(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            executable = root / 'engine.exe'
            executable.touch()
            outliner_dir = root / 'Outliner'
            outliner_dir.mkdir(parents=True)
            (outliner_dir / 'Target.md').write_text('- target\n', encoding='utf-8')
            (outliner_dir / 'Ref1.md').write_text('- [[Target]] ref\n', encoding='utf-8')
            (outliner_dir / 'Ref2.md').write_text('- [[Target|alias]] ref\n', encoding='utf-8')
            (outliner_dir / 'Unrelated.md').write_text('- nothing\n', encoding='utf-8')

            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {
                    'referring_files': [str(outliner_dir / 'Ref1.md'), str(outliner_dir / 'Ref2.md')],
                    'referring_blocks': [],
                }})

            runner = unittest.mock.Mock(return_value=Completed())
            service = OutlinerService(root, executable=executable, runner=runner)
            res = service.execute({'action': 'check_delete', 'page': 'Target'})
            self.assertTrue(res['has_references'])
            self.assertEqual(res['count'], 2)
            self.assertIn('Ref1', res['referring_pages'])
            self.assertIn('Ref2', res['referring_pages'])
            sent = json.loads(runner.call_args.kwargs['input'])
            self.assertEqual(sent['op'], 'check_page_references')

    def test_block_ref_and_backlinks_forwarding(self):
        with tempfile.TemporaryDirectory() as folder:
            executable = Path(folder) / 'engine.exe'
            executable.touch()
            class Completed:
                returncode = 0
                stdout = json.dumps({'ok': True, 'result': {'found': True, 'block_id': 1, 'backlinks': [2]}})
            runner = unittest.mock.Mock(return_value=Completed())
            service = OutlinerService(folder, executable=executable, runner=runner)
            res = service.execute({
                'action': 'resolve_block_ref',
                'page': 'MyPage',
                'markdown': '- text\n',
                'uuid': 'blk-1'
            })
            self.assertTrue(res['found'])
            self.assertEqual(res['backlinks'], [2])
            sent = json.loads(runner.call_args.kwargs['input'])
            self.assertEqual(sent['op'], 'resolve_block_ref')
            self.assertEqual(sent['args']['uuid'], 'blk-1')


if __name__ == '__main__':
    unittest.main()
