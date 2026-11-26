"""CTXEXT1: 합성 파일만 쓰는 추가 통로·변환 입력 보안 회귀 시험."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
from zipfile import ZipFile

from defusedxml.common import DefusedXmlException

from docfit_app.api_contract import ServiceError
from docfit_app.native_file_dialog_service import ADDABLE, NativeFileDialogService
from docfit_core import kordoc_bridge as kb
from docfit_core.file_conversion import CONVERSION_INPUTS


class ContextInputTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.service = NativeFileDialogService()
        self.addCleanup(self.service.close)

    def files(self, extensions):
        paths = []
        for ext in extensions:
            path = self.root / ('synthetic' + ext)
            path.write_bytes(b'synthetic')
            paths.append(str(path))
        return paths

    def test_card_inputs_share_add_and_conversion_policy(self):
        for ext in ('.xls', '.pptx', '.hml'):
            with self.subTest(ext=ext):
                self.assertIn(ext, ADDABLE)
                self.assertIn(ext, CONVERSION_INPUTS)
                paths = self.files([ext.upper()])
                selected = self.service.register_selection('add_files', paths)
                self.assertEqual(self.service.resolve_selection(selected['selection_id'], 'add_files'), paths)
        self.assertNotIn('.ppt', ADDABLE)
        self.assertNotIn('.ppt', CONVERSION_INPUTS)

    def test_folder_collects_supported_and_skips_ppt(self):
        paths = self.files(['.xls', '.PPTX', '.hml'])
        self.files(['.ppt'])
        self.assertEqual(set(self.service.directory_files([str(self.root)])), set(paths))

    def test_document_view_lists_hml_and_passes_it_to_rust_scanner(self):
        from docfit_app.doc_grid_service import DocGridService
        source = Path(self.files(['.hml'])[0])
        service = DocGridService(thumb_dir=self.root / 'thumbs')
        self.assertIn(str(source), [row['path'] for row in service.list_documents(self.root)])
        with patch('docfit_app.doc_grid_service._rust_scan', return_value=[]) as scan:
            with patch.dict('os.environ', {'DOCFIT_SCAN_ENGINE': 'rust'}):
                service.list_documents(self.root, recursive=True)
            self.assertIn('.hml', scan.call_args.args[1])

    def test_mixed_ppt_selection_is_rejected_as_a_whole(self):
        with self.assertRaises(ServiceError):
            self.service.register_selection('add_files', self.files(['.xls', '.ppt']))

    def test_other_purpose_does_not_accept_new_inputs(self):
        for path in self.files(['.xls', '.pptx', '.hml']):
            with self.subTest(path=path), self.assertRaises(ServiceError):
                self.service.register_selection('add_format_file', [path])

    def test_desktop_add_paths_reaches_existing_list_without_starting_work(self):
        from desktop_web_ui import DesktopWebBridge
        paths = self.files(['.xls', '.pptx', '.hml'])
        gui = Mock(running=False, files=[])
        def add(path):
            gui.files.append(path)
            return True
        gui.파일추가.side_effect = add
        bridge = DesktopWebBridge(gui)
        self.addCleanup(bridge._file_dialogs.close)
        bridge._tk = lambda fn: fn()
        bridge.get_state = lambda: {'files': list(gui.files)}
        bridge._update_document_hints = Mock()
        self.assertEqual(bridge.add_paths(paths)['files'], paths)
        gui.작업시작.assert_not_called()
        before = list(gui.files)
        self.assertEqual(bridge.add_paths(self.files(['.ppt']))['files'], before)

    def test_hml_dtd_and_entities_are_rejected_before_engine(self):
        source = self.root / 'unsafe.hml'
        payloads = [
            '<!DOCTYPE HWPML><HWPML/>',
            '<!DOCTYPE HWPML [<!ENTITY x "expanded">]><HWPML>&x;</HWPML>',
            '<!DOCTYPE HWPML SYSTEM "https://invalid.example/no.dtd"><HWPML/>',
            '<!DOCTYPE HWPML [<!ENTITY x SYSTEM "file:///missing">]><HWPML>&x;</HWPML>',
        ]
        with patch.object(kb, 'run_kordoc') as run:
            for xml in payloads:
                for encoding in ('utf-8', 'utf-16'):
                    with self.subTest(xml=xml, encoding=encoding):
                        source.write_text(xml, encoding=encoding)
                        with self.assertRaises(DefusedXmlException):
                            kb.parse_document(source, self.root / 'out.md')
            run.assert_not_called()

    def test_pptx_xml_and_relationship_dtd_rejected_before_engine(self):
        source = self.root / 'unsafe.pptx'
        for part in ('ppt/slides/slide1.xml', 'ppt/_rels/presentation.xml.rels'):
            with self.subTest(part=part):
                with ZipFile(source, 'w') as archive:
                    archive.writestr(part, '<!DOCTYPE root><root/>')
                with patch.object(kb, 'run_kordoc') as run:
                    with self.assertRaises(DefusedXmlException):
                        kb.parse_document(source, self.root / 'out.md')
                    run.assert_not_called()

    def test_safe_hml_and_pptx_reach_kordoc(self):
        hml = self.root / 'safe.hml'
        hml.write_text('<HWPML><BODY/></HWPML>', encoding='utf-8')
        pptx = self.root / 'safe.pptx'
        with ZipFile(pptx, 'w') as archive:
            archive.writestr('ppt/presentation.xml', '<presentation/>')
        for source in (hml, pptx):
            with self.subTest(source=source), patch.object(kb, 'run_kordoc') as run:
                kb.parse_document(source, self.root / 'out.md')
                self.assertEqual(run.call_args.args[0][0], str(source))

    def test_editor_preconversion_uses_existing_kordoc_path(self):
        from docfit_engine.formatter_text import _install
        convert = _install(_docfit_namespace={}, _docfit_symbol='외부문서_hwpx로_변환',
                           bind_function=lambda function, namespace: function)
        for source in self.files(['.xls', '.pptx', '.hml']):
            with self.subTest(source=source), patch.object(kb, 'convert_to_hwpx') as run:
                target = self.root / 'output.hwpx'
                self.assertFalse(convert(Path(source), Path(source).suffix, target))
                run.assert_called_once_with(Path(source), target)


if __name__ == '__main__':
    unittest.main()
