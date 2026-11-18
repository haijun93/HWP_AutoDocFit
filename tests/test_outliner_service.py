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


if __name__ == '__main__':
    unittest.main()
