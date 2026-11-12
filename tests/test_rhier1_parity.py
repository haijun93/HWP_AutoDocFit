"""Search schema parity and compatibility response regressions."""
import os
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core.timemachine.content_search import (
    ParagraphRecord, classify_paragraph_levels, convert_engine_levels_to_records,
)


class SearchResponseTests(unittest.TestCase):
    def test_legacy_roles_fall_back_without_losing_paragraphs(self):
        rows = [ParagraphRecord(0, '# 제목', 'body', 8, False)]
        with patch('docfit_app.hierarchy_bridge.analyze_levels', return_value={'role_sequence': ['제목']}):
            self.assertEqual(classify_paragraph_levels('x.md', rows)[0].level, 'document_title')

    def test_search_levels_preserve_title_cell_and_metadata(self):
        rows = [ParagraphRecord(0, '제목 칸', 'body', 8, True, table_cell_id=3, table_id=1)]
        result = convert_engine_levels_to_records(rows, {'search_levels': ['document_title']})
        self.assertEqual((result[0].level, result[0].table_cell_id), ('document_title', 3))

    def test_invalid_search_levels_fall_back(self):
        rows = [ParagraphRecord(0, '# 제목', 'body', 8, False)]
        for levels in ([], ['unknown'], ['body', 'body']):
            with self.subTest(levels=levels), patch('docfit_app.hierarchy_bridge.analyze_levels', return_value={'search_levels': levels}):
                self.assertEqual(classify_paragraph_levels('x.md', rows)[0].level, 'document_title')


class NativeParityTests(unittest.TestCase):
    def test_synthetic_corpus(self):
        from scripts.rhier1_diff import compare
        root = Path(__file__).resolve().parents[1]
        exe = Path(os.environ.get('HIERARCHY_EXE', root / 'hierarchy/target/debug/hierarchy.exe'))
        if not exe.is_file():
            self.skipTest('build hierarchy or set HIERARCHY_EXE')
        result = compare(exe)
        self.assertEqual(result['differences'], [])
