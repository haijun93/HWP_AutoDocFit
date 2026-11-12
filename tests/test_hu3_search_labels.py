"""HU3 범정부 라벨 색인 회귀 시험."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core.timemachine.content_search import (
    ContentSearchIndex, ParagraphRecord, classify_paragraph_levels,
)
from docfit_core.timemachine import hierarchy_path


class SearchLabelTests(unittest.TestCase):
    def test_labels_with_and_without_engine(self):
        lines = ['네모: 배경', '원: 내용', '바: 상세', '점: 부연',
                 '당구: 참고', '주석: 하나', '주석1: 둘', '주석2: 셋', '개요: 요약']
        records = [ParagraphRecord(i, t, 'body', 8, False) for i, t in enumerate(lines)]
        for engine in (None, {'levels': [8] * len(lines)}):
            with self.subTest(engine=engine), patch(
                    'docfit_app.hierarchy_bridge.analyze_levels', return_value=engine):
                result = classify_paragraph_levels('sample.txt', records)
                self.assertEqual(['square', 'circle', 'dash'] + ['asterisk'] * 5 + ['body'],
                                 [p.level for p in result])
                self.assertEqual('□ 배경', result[0].text)

    def test_fixture_index_and_parent_path(self):
        with tempfile.TemporaryDirectory() as folder:
            index = ContentSearchIndex(Path(folder) / 'index.sqlite')
            try:
                fixture = Path(__file__).parent / 'fixtures' / 'beomgov_markdown.txt'
                self.assertTrue(index.index_file(str(fixture))['indexed'])
                rows = [dict(row) for row in index.db.execute(
                    'SELECT paragraph_index, body, level, level_rank FROM content_paragraph_fts ORDER BY CAST(paragraph_index AS INTEGER)')]
                by_text = {r['body']: r for r in rows}
                for text, level in [('□ 추진 배경', 'square'),
                                    ('ㅇ 노후 시설의 안전점검 결과 반영', 'circle'),
                                    ('- 금액 2,970,000원과 일정 14:00 유지', 'dash'),
                                    ('* 예산은 확정 후 반영', 'asterisk'),
                                    ('** 일정은 별도 협의', 'asterisk')]:
                    self.assertEqual(level, by_text[text]['level'])
                trail = hierarchy_path.breadcrumb(rows, by_text['ㅇ 노후 시설의 안전점검 결과 반영']['paragraph_index'])
                self.assertIn('□ 추진 배경', [step['text'] for step in trail])
                index.db.execute('UPDATE content_meta SET hierarchy_schema_version=3')
                index.db.commit()
                self.assertTrue(index.index_file(str(fixture))['indexed'])
                self.assertEqual(4, index.db.execute(
                    'SELECT hierarchy_schema_version FROM content_meta').fetchone()[0])
            finally:
                index.close()
