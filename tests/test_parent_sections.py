"""부모 절 묶음과 기존 키워드 검색 결과 보존 시험."""
import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core.timemachine import hierarchy_path as hp
from docfit_core.timemachine.content_search import ContentSearchIndex


class ParentSectionsTests(unittest.TestCase):
    def test_nearest_parent_document_order_and_no_mutation(self):
        rows = [dict(paragraph_index=i * 10, level_rank=rank, body=text)
                for i, (rank, text) in enumerate([
                    (8, 'preface'), (0, 'Title'), (1, '# Chapter'),
                    (2, 'Section'), (8, 'one'), (None, 'two'),
                    (2, 'Next'), (8, 'three'), (1, 'Other'), (8, 'four')])]
        before = copy.deepcopy(rows)
        result = hp.group_by_parent(list(reversed(rows)), [90, 50, 40, 40, 0, 70, 30, 999])
        self.assertEqual(result, [
            dict(parent_paragraph_index=None, parent_text='', breadcrumb_text='',
                 paragraph_indexes=[0], count=1),
            dict(parent_paragraph_index=20, parent_text='Chapter', breadcrumb_text='Title > Chapter',
                 paragraph_indexes=[30], count=1),
            dict(parent_paragraph_index=30, parent_text='Section', breadcrumb_text='Title > Chapter > Section',
                 paragraph_indexes=[40, 50], count=2),
            dict(parent_paragraph_index=60, parent_text='Next', breadcrumb_text='Title > Chapter > Next',
                 paragraph_indexes=[70], count=1),
            dict(parent_paragraph_index=80, parent_text='Other', breadcrumb_text='Title > Other',
                 paragraph_indexes=[90], count=1),
        ])
        self.assertEqual(rows, before)

    def test_empty_flat_and_display_title_is_not_parent(self):
        self.assertEqual(hp.group_by_parent([], [1]), [])
        rows = [dict(paragraph_index=i, level_rank=8, level=level, body=text)
                for i, (level, text) in enumerate([
                    ('table_cell', 'Display title'), ('body', 'one'), ('body', 'two')])]
        self.assertEqual(hp.group_by_parent(rows, []), [])
        self.assertEqual(hp.group_by_parent(rows, [2, 1]), [
            dict(parent_paragraph_index=None, parent_text='', breadcrumb_text='',
                 paragraph_indexes=[1, 2], count=2)])

    def test_keyword_sections_all_matches_selected_only_and_existing_fields(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            index = ContentSearchIndex(root / 'index.sqlite')
            try:
                for n in range(3):
                    source = root / f'doc{n}.md'
                    source.write_text('# Title\n\n## Section\n\n' +
                                      '\n\n'.join(f'needle body {i}' for i in range(5)), encoding='utf-8')
                    index.index_file(source)
                original_group = hp.group_by_parent
                with patch.object(index, '_breadcrumb_rows', wraps=index._breadcrumb_rows) as lookup, \
                     patch.object(hp, 'group_by_parent', wraps=original_group) as grouping:
                    results = index.keyword_search('needle', limit=1, offset=1)
                self.assertEqual(lookup.call_count, 1)
                self.assertEqual(grouping.call_count, 1)
                self.assertEqual(len(results), 1)
                self.assertEqual(len(results[0]['locations']), 3)
                self.assertEqual(results[0]['sections'], [dict(
                    parent_paragraph_index=1, parent_text='Section',
                    breadcrumb_text='Title > Section', paragraph_indexes=[2, 3, 4, 5, 6], count=5)])
                # 추가 필드 계산을 대체해도 모든 기존 필드 값과 순서는 같다.
                with patch.object(hp, 'group_by_parent', return_value=[]):
                    baseline = index.keyword_search('needle', limit=1, offset=1)
                self.assertEqual([{k: v for k, v in r.items() if k != 'sections'} for r in results],
                                 [{k: v for k, v in r.items() if k != 'sections'} for r in baseline])
                with patch.object(index, '_breadcrumb_rows', wraps=index._breadcrumb_rows) as lookup, \
                     patch.object(hp, 'group_by_parent', wraps=original_group) as grouping:
                    self.assertEqual(index.keyword_search('needle', offset=10), [])
                lookup.assert_not_called()
                grouping.assert_not_called()
            finally:
                index.close()
