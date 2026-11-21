"""설정 `conversion_ui`('flutter' 기본 | 'native')와 Win32 몽땅파일변환 창(convert_win.exe) 실행 — 앱 쪽에는 켜기만 있다."""
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from docfit_app.api_contract import ServiceError
from docfit_app.conversion_window_service import ConversionWindowService


class FakeSettings:
    def __init__(self, path=None):
        self.values = {}
        self._path = path

    def read(self):
        return dict(self.values)

    def update(self, change):
        change(self.values)


class ConversionWindowServiceTest(unittest.TestCase):
    def _exe(self, folder):
        exe = Path(folder) / 'convert_win.exe'
        exe.touch()
        return exe

    def test_ui_mode_defaults_to_flutter_saves_native_and_rejects_other_values(self):
        settings = FakeSettings()
        service = ConversionWindowService(settings)
        self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})
        self.assertEqual(service.execute({'action': 'ui_mode', 'value': 'native'}), {'mode': 'native'})
        self.assertEqual(settings.values['conversion_ui'], 'native')
        with self.assertRaises(ServiceError) as raised:
            service.execute({'action': 'ui_mode', 'value': 'tk'})
        self.assertEqual(raised.exception.code, 'INVALID_INPUT')
        self.assertEqual(settings.values['conversion_ui'], 'native')
        settings.values['conversion_ui'] = 'garbage'
        self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})

    def test_ui_mode_without_settings_store_is_a_service_error(self):
        with self.assertRaises(ServiceError):
            ConversionWindowService(None).execute({'action': 'ui_mode'})

    def test_open_if_native_does_nothing_for_the_default_flutter_mode(self):
        """기본값 'flutter' 에서는 창을 열지 않는다 = 사용자 눈에 보이는 변화 없음."""
        with tempfile.TemporaryDirectory() as folder:
            launcher = mock.Mock()
            service = ConversionWindowService(FakeSettings(), executable=self._exe(folder), launcher=launcher)
            self.assertEqual(service.execute({'action': 'open_if_native'}), {'mode': 'flutter', 'opened': False})
            launcher.assert_not_called()

    def test_open_if_native_starts_the_window_in_native_mode(self):
        with tempfile.TemporaryDirectory() as folder:
            exe = self._exe(folder)
            settings = FakeSettings()
            settings.values['conversion_ui'] = 'native'
            launcher = mock.Mock()
            service = ConversionWindowService(settings, executable=exe, launcher=launcher)
            self.assertEqual(service.execute({'action': 'open_if_native'}), {'mode': 'native', 'opened': True})
            self.assertEqual(launcher.call_args.args[0], [str(exe)])

    def test_open_native_starts_the_window_regardless_of_mode(self):
        with tempfile.TemporaryDirectory() as folder:
            exe = self._exe(folder)
            launcher = mock.Mock()
            service = ConversionWindowService(FakeSettings(), executable=exe, launcher=launcher)
            self.assertEqual(service.execute({'action': 'open_native'}), {'opened': True})
            launcher.assert_called_once()

    def test_open_native_reports_missing_program_and_launch_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            service = ConversionWindowService(FakeSettings(), executable=Path(folder) / 'none.exe', launcher=mock.Mock())
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'open_native'})
            self.assertEqual(raised.exception.code, 'CONVERSION_WINDOW_MISSING')
            service = ConversionWindowService(FakeSettings(), executable=self._exe(folder), launcher=mock.Mock(side_effect=OSError('x')))
            with self.assertRaises(ServiceError) as raised:
                service.execute({'action': 'open_native'})
            self.assertEqual(raised.exception.code, 'CONVERSION_WINDOW_FAILED')

    def test_unknown_action_is_rejected(self):
        with self.assertRaises(ServiceError):
            ConversionWindowService(FakeSettings()).execute({'action': 'nope'})

    def test_setting_is_registered_with_a_flutter_default_and_excluded_from_exports(self):
        from docfit_app.configuration_service import EXCLUDED
        from docfit_app.settings_schema import DEFAULTS
        self.assertEqual(DEFAULTS['conversion_ui'], 'flutter')
        self.assertIn('conversion_ui', EXCLUDED)

    def test_golden_defaults_and_rust_hidden_keys_know_the_new_key(self):
        import json
        root = Path(__file__).resolve().parents[1]
        golden = json.loads((root / 'docs' / 'settings_defaults.json').read_text(encoding='utf-8'))
        self.assertEqual(golden.get('conversion_ui'), 'flutter')
        rust = (root / 'docfit_config' / 'src' / 'lib.rs').read_text(encoding='utf-8')
        self.assertIn('"conversion_ui"', rust, 'docfit_config 의 설정 창 숨김 목록에도 있어야 한다')


if __name__ == '__main__':
    unittest.main()
