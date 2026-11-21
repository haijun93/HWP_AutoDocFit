"""설정 `photoview_ui`('flutter' 기본 | 'native')와 Win32 사진뷰 창(photoview_win.exe) 실행 — 앱 쪽에는 켜기만 있다."""
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from docfit_app.api_contract import ServiceError
from docfit_app.photoview_window_service import PhotoViewWindowService


class FakeSettings:
    def __init__(self, path=None):
        self.values = {}
        self._path = path

    def read(self):
        return dict(self.values)

    def update(self, change):
        change(self.values)


class PhotoViewWindowServiceTest(unittest.TestCase):
    def _exe(self, folder):
        exe = Path(folder) / 'photoview_win.exe'
        exe.touch()
        return exe

    def test_ui_mode_defaults_to_flutter_saves_native_and_rejects_other_values(self):
        settings = FakeSettings()
        service = PhotoViewWindowService(settings)
        self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})
        self.assertEqual(service.execute({'action': 'ui_mode', 'value': 'native'}), {'mode': 'native'})
        self.assertEqual(settings.values['photoview_ui'], 'native')
        with self.assertRaises(ServiceError) as raised:
            service.execute({'action': 'ui_mode', 'value': 'tk'})
        self.assertEqual(raised.exception.code, 'INVALID_INPUT')
        self.assertEqual(settings.values['photoview_ui'], 'native')
        settings.values['photoview_ui'] = 'garbage'
        self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})

    def test_ui_mode_without_settings_store_is_a_service_error(self):
        with self.assertRaises(ServiceError):
            PhotoViewWindowService(None).execute({'action': 'ui_mode'})

    def test_open_if_native_does_nothing_for_the_default_flutter_mode(self):
        """기본값 'flutter' 에서는 창을 열지 않는다 = 사용자 눈에 보이는 변화 없음."""
        with tempfile.TemporaryDirectory() as folder:
            launcher = mock.Mock()
            service = PhotoViewWindowService(FakeSettings(), executable=self._exe(folder), launcher=launcher)
            self.assertEqual(service.execute({'action': 'open_if_native'}), {'mode': 'flutter', 'opened': False})
            launcher.assert_not_called()

    def test_open_if_native_starts_the_window_with_the_apps_settings_file_and_klib(self):
        with tempfile.TemporaryDirectory() as folder:
            exe = self._exe(folder)
            settings_file = Path(folder) / 'settings.json'
            settings = FakeSettings(settings_file)
            settings.values['photoview_ui'] = 'native'
            launcher = mock.Mock()
            klib = Path(folder) / 'klib.exe'
            service = PhotoViewWindowService(settings, executable=exe, launcher=launcher, klib_finder=lambda: klib)
            self.assertEqual(service.execute({'action': 'open_if_native'}), {'mode': 'native', 'opened': True})
            self.assertEqual(launcher.call_args.args[0], [str(exe), '--settings', str(settings_file), '--klib', str(klib)])

    def test_open_native_omits_klib_when_the_app_cannot_find_one(self):
        with tempfile.TemporaryDirectory() as folder:
            exe = self._exe(folder)
            launcher = mock.Mock()
            service = PhotoViewWindowService(FakeSettings(), executable=exe, launcher=launcher, klib_finder=lambda: None)
            service.execute({'action': 'open_native'})
            self.assertEqual(launcher.call_args.args[0], [str(exe)])

    def test_open_native_reports_missing_program_and_launch_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            service = PhotoViewWindowService(FakeSettings(), executable=Path(folder) / 'none.exe', launcher=mock.Mock())
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'open_native'})
            self.assertEqual(raised.exception.code, 'PHOTOVIEW_WINDOW_MISSING')
            service = PhotoViewWindowService(FakeSettings(), executable=self._exe(folder), launcher=mock.Mock(side_effect=OSError('x')), klib_finder=None)
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'open_native'})
            self.assertEqual(raised.exception.code, 'PHOTOVIEW_WINDOW_FAILED')

    def test_unknown_action_is_rejected(self):
        with self.assertRaises(ServiceError):
            PhotoViewWindowService(FakeSettings()).execute({'action': 'nope'})

    def test_setting_is_registered_with_a_flutter_default_and_excluded_from_exports(self):
        from docfit_app.configuration_service import EXCLUDED
        from docfit_app.settings_schema import DEFAULTS
        self.assertEqual(DEFAULTS['photoview_ui'], 'flutter')
        self.assertIn('photoview_ui', EXCLUDED)

    def test_golden_defaults_and_rust_hidden_keys_know_the_new_key(self):
        import json
        root = Path(__file__).resolve().parents[1]
        golden = json.loads((root / 'docs' / 'settings_defaults.json').read_text(encoding='utf-8'))
        self.assertEqual(golden.get('photoview_ui'), 'flutter')
        rust = (root / 'docfit_config' / 'src' / 'lib.rs').read_text(encoding='utf-8')
        self.assertIn('"photoview_ui"', rust, 'docfit_config 의 설정 창 숨김 목록에도 있어야 한다')


if __name__ == '__main__':
    unittest.main()
