import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

from docfit_app.api_contract import ServiceError
from docfit_app.pdf_edit_service import PdfEditService


def _pdf_bytes():
    from pypdf import PdfWriter
    import io
    writer = PdfWriter()
    writer.add_blank_page(width=100, height=100)
    data = io.BytesIO()
    writer.write(data)
    return data.getvalue()


PDF = _pdf_bytes()


class PdfEditServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.source = self.root / 'source.pdf'
        self.source.write_bytes(PDF)
        self.executable = self.root / 'pdf-edit.exe'
        self.executable.write_bytes(b'fake')
        self.runner = Mock(return_value=Mock(returncode=0, stderr=''))
        self.service = PdfEditService(self.executable, self.runner)

    def tearDown(self):
        self.temp.cleanup()

    def test_invokes_fake_engine_and_keeps_original(self):
        output = self.root / 'result.pdf'
        def run_delete(args, **kwargs):
            Path(args[3]).write_bytes(b'edited')
            return Mock(returncode=0, stderr='')
        self.runner.side_effect = run_delete
        self.service.execute('delete', self.source, output, pages=[2])
        self.assertEqual(self.source.read_bytes(), PDF)
        self.runner.assert_called_once()
        self.assertEqual(self.runner.call_args.args[0][1], 'delete')

    def test_split_and_merge_are_routed_to_engine(self):
        split_dir = self.root / 'parts'
        def split_runner(args, **kwargs):
            Path(args[3]).mkdir()
            (Path(args[3]) / 'part-1.pdf').write_bytes(b'part')
            return Mock(returncode=0, stderr='')
        self.runner.side_effect = split_runner
        self.service.execute('split', self.source, split_dir, pages=[1, 2])
        self.assertEqual(self.runner.call_args.args[0][1], 'split')
        self.assertEqual(self.runner.call_args.args[0][-1], '1-2')
        second = self.root / 'second.pdf'
        second.write_bytes(PDF)
        output = self.root / 'merged.pdf'
        self.runner.side_effect = lambda args, **kwargs: (Path(args[2]).write_bytes(b'merged') and Mock(returncode=0, stderr=''))
        self.service.execute('merge', None, output, sources=[self.source, second])
        self.assertEqual(self.runner.call_args.args[0][1], 'merge')
        self.assertEqual(self.source.read_bytes(), PDF)

    def test_permission_marker_without_encryption_is_not_misclassified(self):
        self.source.write_bytes(b'%PDF-1.4\n/P value\n%%EOF')
        with self.assertRaises(ServiceError) as caught:
            self.service.execute('delete', self.source, self.root / 'out.pdf', pages=[1])
        self.assertEqual(caught.exception.code, 'PDF_DAMAGED')

    def test_refuses_existing_output_and_same_source(self):
        with self.assertRaises(ServiceError):
            self.service.execute('delete', self.source, self.source, pages=[1])
        output = self.root / 'result.pdf'
        output.write_bytes(b'keep')
        with self.assertRaises(ServiceError):
            self.service.execute('delete', self.source, output, pages=[1])
        self.assertEqual(output.read_bytes(), b'keep')
        self.runner.assert_not_called()

    def test_classifies_encrypted_restricted_and_damaged_before_engine(self):
        for content, code in [
            (b'%PDF-1.4 /Encrypt true\n%%EOF', 'PDF_ENCRYPTED'),
            (b'not a pdf', 'PDF_DAMAGED'),
        ]:
            self.source.write_bytes(content)
            with self.assertRaises(ServiceError) as caught:
                self.service.execute('delete', self.source, self.root / 'result.pdf', pages=[1])
            self.assertEqual(caught.exception.code, code)
            self.runner.assert_not_called()
        from pypdf import PdfWriter
        from pypdf.constants import UserAccessPermissions as UAP
        writer = PdfWriter()
        writer.add_blank_page(width=100, height=100)
        writer.encrypt(user_password='', owner_password='owner', permissions_flag=UAP.PRINT)
        with self.source.open('wb') as stream:
            writer.write(stream)
        with self.assertRaises(ServiceError) as caught:
            self.service.execute('delete', self.source, self.root / 'result.pdf', pages=[1])
        self.assertEqual(caught.exception.code, 'PDF_RESTRICTED')

    def test_missing_engine_has_clear_error(self):
        self.service.executable = self.root / 'missing.exe'
        with self.assertRaises(ServiceError) as caught:
            self.service.execute('delete', self.source, self.root / 'result.pdf', pages=[1])
        self.assertEqual(caught.exception.code, 'PDF_ENGINE_MISSING')


if __name__ == '__main__':
    unittest.main()
