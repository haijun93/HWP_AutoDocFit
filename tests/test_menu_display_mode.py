import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from docfit_app.application_service import ApplicationService


class MenuDisplayModeTests(unittest.TestCase):
    def test_missing_and_invalid_values_resolve_to_sidebar(self):
        with TemporaryDirectory() as tmp:
            service = ApplicationService(Path(tmp) / 'settings.json')
            self.assertEqual(service.state()['menu_display_mode'], 'sidebar')
            Path(tmp, 'settings.json').write_text(json.dumps({'menu_display_mode': 'unknown'}), encoding='utf-8')
            self.assertEqual(service.state()['menu_display_mode'], 'sidebar')
            service.jobs.shutdown()

    def test_valid_modes_persist_and_invalid_requests_are_rejected(self):
        with TemporaryDirectory() as tmp:
            path = Path(tmp) / 'settings.json'
            service = ApplicationService(path)
            self.assertEqual(service.dispatch('set_menu_display_mode', ['dock']), {'menu_display_mode': 'dock'})
            self.assertEqual(service.state()['menu_display_mode'], 'dock')
            self.assertEqual(json.loads(path.read_text(encoding='utf-8'))['menu_display_mode'], 'dock')
            with self.assertRaises(Exception):
                service.dispatch('set_menu_display_mode', ['bogus'])
            service.jobs.shutdown()


if __name__ == '__main__':
    unittest.main()
