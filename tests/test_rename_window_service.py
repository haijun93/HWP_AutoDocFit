import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from docfit_app.api_contract import ServiceError
from docfit_app.rename_window_service import RenameWindowService


class FakeSettings:
    def __init__(self): self.values = {}
    def read(self): return dict(self.values)
    def update(self, change): change(self.values)


class RenameWindowServiceTest(unittest.TestCase):
    def test_mode_defaults_to_flutter_and_only_accepts_flutter_or_native(self):
        settings = FakeSettings()
        service = RenameWindowService(settings)
        self.assertEqual(service.execute({'action': 'ui_mode'}), {'mode': 'flutter'})
        self.assertEqual(service.execute({'action': 'ui_mode', 'value': 'native'}), {'mode': 'native'})
        self.assertEqual(settings.values['rename_ui'], 'native')
        with self.assertRaises(ServiceError): service.execute({'action': 'ui_mode', 'value': 'tk'})

    def test_golden_default_matches_application_schema(self):
        from docfit_app.settings_schema import DEFAULTS
        root = Path(__file__).resolve().parents[1]
        golden = json.loads((root / 'docs' / 'settings_defaults.json').read_text(encoding='utf-8'))
        self.assertEqual(DEFAULTS['rename_ui'], 'flutter')
        self.assertEqual(golden['rename_ui'], DEFAULTS['rename_ui'])

    def test_native_window_open_is_explicit_and_does_not_change_files(self):
        with tempfile.TemporaryDirectory() as folder:
            exe = Path(folder) / 'rename_win.exe'; exe.touch()
            launcher = mock.Mock()
            service = RenameWindowService(FakeSettings(), executable=exe, launcher=launcher)
            self.assertEqual(service.execute({'action': 'open_if_native'}), {'mode': 'flutter', 'opened': False})
            launcher.assert_not_called()
            self.assertEqual(service.execute({'action': 'open_native'}), {'mode': 'flutter', 'opened': True})
            launcher.assert_called_once()


if __name__ == '__main__': unittest.main()
