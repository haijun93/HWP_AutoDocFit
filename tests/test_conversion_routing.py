"""라우팅 상태·1회 폴백·게시 경계. 실제 엔진은 합성 결과 대역만 사용한다."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_app.settings_schema import DEFAULTS
from docfit_core import conversion_routing as route
from docfit_core import kordoc_bridge as kb
from docfit_core.conversion_fixtures import build_docx
from docfit_core.markdown_hwpx import generate_hwpx


class ConversionRoutingTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'source.docx'
        build_docx([('h', 1, '제목'), ('p', '본문'), ('table', [['키', '값'], ['가', '2']])], self.source)
        self.target = self.root / 'result.hwpx'

    @staticmethod
    def kordoc(source, target, **kwargs):
        generate_hwpx('# 대체 결과\n\n본문', target)
        return target

    def test_settings_default_and_bridge_facade(self):
        self.assertEqual(DEFAULTS['docx_engine'], 'kordoc')
        self.assertEqual(route.configured_engine({}), 'kordoc')
        self.assertEqual(route.configured_engine({'docx_engine': 'auto'}), 'auto')
        with self.assertRaises(ValueError):
            route.configured_engine({'docx_engine': 'typo'})
        result = kb.convert_to_hwpx_routed(self.source, '보고서', 'native', target=self.target)
        self.assertEqual(result.engine, 'native')
        self.assertEqual(result.states, ('prepare', 'native_attempt', 'native_validation', 'publish', 'complete'))
        self.assertTrue(result.quality['verdict']['pass'])

    def test_new_internal_key_does_not_change_existing_settings_form(self):
        from docfit_app.configuration_service import fields
        self.assertNotIn('docx_engine', {field['key'] for field in fields()})

    def test_kordoc_setting_skips_native(self):
        with patch.object(kb, 'convert_to_hwpx', side_effect=self.kordoc) as fallback, patch.object(route, '_native') as native:
            result = route.convert_to_hwpx_routed(self.source, engine=route.configured_engine({}), target=self.target)
        native.assert_not_called()
        fallback.assert_called_once()
        self.assertEqual(result.states, ('prepare', 'kordoc_attempt', 'publish', 'complete'))
        self.assertIsNone(result.fallback_reason)

    def test_auto_native_success_does_not_call_kordoc(self):
        with patch.object(kb, 'convert_to_hwpx') as fallback:
            result = route.convert_to_hwpx_routed(self.source, target=self.target)
        fallback.assert_not_called()
        self.assertEqual(result.engine, 'native')

    def test_native_failure_falls_back_once_with_reason(self):
        with patch.object(route, '_native', side_effect=route.NativeQualityError('그림 누락')) as native, patch.object(kb, 'convert_to_hwpx', side_effect=self.kordoc) as fallback:
            result = route.convert_to_hwpx_routed(self.source, target=self.target)
        native.assert_called_once()
        fallback.assert_called_once()
        self.assertEqual(result.engine, 'kordoc')
        self.assertIn('그림 누락', result.fallback_reason)
        self.assertEqual(result.states, ('prepare', 'native_attempt', 'fallback', 'kordoc_attempt', 'publish', 'complete'))
        self.assertEqual(list(self.root.glob('.docfit-route-*')), [])

    def test_explicit_native_failure_does_not_fallback_or_publish(self):
        with patch.object(route, '_native', side_effect=route.NativeQualityError('손실')), patch.object(kb, 'convert_to_hwpx') as fallback:
            with self.assertRaises(route.NativeQualityError):
                route.convert_to_hwpx_routed(self.source, engine='native', target=self.target)
        fallback.assert_not_called()
        self.assertFalse(self.target.exists())

    def test_unsupported_preset_in_auto_uses_kordoc(self):
        with patch.object(kb, 'convert_to_hwpx', side_effect=self.kordoc):
            result = route.convert_to_hwpx_routed(self.source, '계획서', target=self.target)
        self.assertEqual(result.engine, 'kordoc')
        self.assertIn('프리셋', result.fallback_reason)

    def test_warning_and_serialized_quality_failure_are_not_published(self):
        original = self.source.read_bytes()
        with patch.object(route, 'generate_hwpx', side_effect=lambda md, p, **kw: generate_hwpx('빠진 결과', p)), patch.object(kb, 'convert_to_hwpx', side_effect=self.kordoc):
            result = route.convert_to_hwpx_routed(self.source, target=self.target)
        self.assertEqual(result.engine, 'kordoc')
        self.assertIn('품질 미달', result.fallback_reason)
        self.assertEqual(original, self.source.read_bytes())

    def test_existing_target_and_source_overwrite_never_try_an_engine(self):
        self.target.write_bytes(b'existing')
        with patch.object(route, '_native') as native, patch.object(kb, 'convert_to_hwpx') as fallback:
            with self.assertRaises(FileExistsError):
                route.convert_to_hwpx_routed(self.source, target=self.target)
            with self.assertRaises(ValueError):
                route.convert_to_hwpx_routed(self.source, target=self.source)
        native.assert_not_called()
        fallback.assert_not_called()
        self.assertEqual(self.target.read_bytes(), b'existing')

    def test_publish_race_preserves_other_file_and_does_not_fallback(self):
        link = route.os.link

        def race(source, target):
            target.write_bytes(b'other writer')
            link(source, target)

        with patch.object(route.os, 'link', side_effect=race), patch.object(kb, 'convert_to_hwpx') as fallback:
            with self.assertRaises(FileExistsError):
                route.convert_to_hwpx_routed(self.source, target=self.target)
        fallback.assert_not_called()
        self.assertEqual(self.target.read_bytes(), b'other writer')

    def test_permission_error_and_fallback_failure_are_propagated(self):
        with patch.object(route, '_native', side_effect=PermissionError('access')), patch.object(kb, 'convert_to_hwpx') as fallback:
            with self.assertRaises(PermissionError):
                route.convert_to_hwpx_routed(self.source, target=self.target)
        fallback.assert_not_called()
        with patch.object(route, '_native', side_effect=route.NativeQualityError('loss')), patch.object(kb, 'convert_to_hwpx', side_effect=RuntimeError('kordoc failed')) as fallback:
            with self.assertRaisesRegex(RuntimeError, 'kordoc failed'):
                route.convert_to_hwpx_routed(self.source, target=self.target)
        fallback.assert_called_once()
        self.assertFalse(self.target.exists())

    def test_loss_warnings_force_fallback(self):
        source = self.root / 'loss.html'
        source.write_text('<h1>제목</h1><p style="color:red">본문</p>', encoding='utf-8')
        with patch.object(route, '_kordoc', side_effect=lambda s, t, w, p: generate_hwpx('대체', t)):
            result = route.convert_to_hwpx_routed(source, target=self.target)
        self.assertEqual(result.engine, 'kordoc')
        self.assertIn('CSS', result.fallback_reason)

    def test_custom_style_is_not_silently_flattened(self):
        from zipfile import ZipFile, ZIP_DEFLATED
        with ZipFile(self.source) as archive:
            parts = {n: archive.read(n) for n in archive.namelist()}
        parts['word/document.xml'] = parts['word/document.xml'].replace(b'Heading1', b'CustomHeading')
        with ZipFile(self.source, 'w', ZIP_DEFLATED) as archive:
            for name, value in parts.items():
                archive.writestr(name, value)
        with patch.object(kb, 'convert_to_hwpx', side_effect=self.kordoc):
            result = route.convert_to_hwpx_routed(self.source, target=self.target)
        self.assertIn('미검증 문단 스타일', result.fallback_reason)
        self.assertEqual(result.engine, 'kordoc')

    def test_literal_angle_brackets_and_table_escapes_survive_quality_gate(self):
        build_docx([('h', 1, '한글 <제목>'), ('table', [['키', '값'], ['A | B', '특수 & < >']])], self.source)
        result = route.convert_to_hwpx_routed(self.source, engine='native', target=self.target)
        self.assertTrue(result.quality['verdict']['pass'])

    def test_malformed_xml_uses_single_fallback(self):
        from zipfile import ZipFile
        with ZipFile(self.source, 'w') as archive:
            archive.writestr('word/document.xml', '<broken>')
        with patch.object(kb, 'convert_to_hwpx', side_effect=self.kordoc) as fallback:
            result = route.convert_to_hwpx_routed(self.source, target=self.target)
        fallback.assert_called_once()
        self.assertEqual(result.engine, 'kordoc')


if __name__ == '__main__':
    unittest.main()
