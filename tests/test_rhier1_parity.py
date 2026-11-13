"""Search schema parity and compatibility response regressions."""
import os
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core.timemachine.content_search import (
    ParagraphRecord, classify_paragraph_levels, convert_engine_levels_to_records,
)


class SearchResponseTests(unittest.TestCase):
    def test_engine_stays_disabled_by_default(self):
        from docfit_app import hierarchy_bridge
        from docfit_app.settings_schema import DEFAULTS
        self.assertFalse(DEFAULTS['hierarchy_engine'])
        with patch.object(hierarchy_bridge, 'run_executable') as run:
            self.assertIsNone(hierarchy_bridge.analyze_levels([], settings=DEFAULTS))
            run.assert_not_called()

    def test_legacy_roles_fall_back_without_losing_paragraphs(self):
        rows = [ParagraphRecord(0, '# 제목', 'body', 8, False)]
        with patch('docfit_app.hierarchy_bridge.analyze_levels', return_value={'role_sequence': ['제목']}):
            self.assertEqual(classify_paragraph_levels('x.md', rows)[0].level, 'document_title')

    def test_legacy_integer_contract_is_preserved(self):
        rows = [ParagraphRecord(0, 'ordinary', 'body', 8, False)]
        with patch('docfit_app.hierarchy_bridge.analyze_levels', return_value={'levels': [5]}):
            self.assertEqual(classify_paragraph_levels('x.txt', rows)[0].level, 'circle')
        cells = [ParagraphRecord(0, 'table', 'body', 8, True)]
        with patch('docfit_app.hierarchy_bridge.analyze_levels', return_value={'levels': [0]}):
            self.assertEqual(classify_paragraph_levels('x.docx', cells)[0].level, 'table_cell')

    def test_search_levels_preserve_title_cell_and_metadata(self):
        rows = [ParagraphRecord(0, '제목 칸', 'body', 8, True, table_cell_id=3, table_id=1)]
        result = convert_engine_levels_to_records(rows, {'search_levels': ['document_title']})
        self.assertEqual((result[0].level, result[0].table_cell_id), ('document_title', 3))

    def test_invalid_search_levels_fall_back(self):
        rows = [ParagraphRecord(0, '# 제목', 'body', 8, False)]
        for levels in ([], ['unknown'], ['body', 'body'], [None], [{}]):
            with self.subTest(levels=levels), patch('docfit_app.hierarchy_bridge.analyze_levels', return_value={'search_levels': levels}):
                self.assertEqual(classify_paragraph_levels('x.md', rows)[0].level, 'document_title')


class NativeParityTests(unittest.TestCase):
    def test_enabled_bridge_uses_native_search_contract(self):
        from docfit_app import hierarchy_bridge as hb
        root = Path(__file__).resolve().parents[1]
        exe = Path(os.environ.get('HIERARCHY_EXE', root / 'hierarchy/target/debug/hierarchy.exe'))
        if not exe.is_file():
            self.skipTest('build hierarchy or set HIERARCHY_EXE')
        rows = [ParagraphRecord(i, text, 'body', 8, False)
                for i, text in enumerate(['# 문서', '## 장', '네모: 항목', '원: 내용', '| 표 |', '|', '||'])]
        with patch.object(hb, '_read_settings', return_value={'hierarchy_engine': True}), \
                patch.object(hb, 'find_library', return_value=None), \
                patch.object(hb, 'find_executable', return_value=exe):
            result = classify_paragraph_levels('bridge.md', rows)
        self.assertEqual([p.level for p in result],
                         ['document_title', 'chapter', 'square', 'circle', 'table_cell', 'dash', 'table_cell'])

    def test_synthetic_corpus(self):
        from scripts.rhier1_diff import compare
        root = Path(__file__).resolve().parents[1]
        exe = Path(os.environ.get('HIERARCHY_EXE', root / 'hierarchy/target/debug/hierarchy.exe'))
        if not exe.is_file():
            self.skipTest('build hierarchy or set HIERARCHY_EXE')
        result = compare(exe)
        self.assertEqual(result['differences'], [])
        self.assertEqual(result['engine_differences'], [])
