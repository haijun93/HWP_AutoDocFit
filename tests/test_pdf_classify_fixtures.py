"""PDFWIN2: Rust pdf_classify 대조용 기대값(expected.json)이 현재 Python _classify 와 일치하는지 확인.

기대값이 낡아 Rust 대조 시험이 엉뚱한 기준을 쓰는 일을 막는다.
"""
import json
import unittest
from pathlib import Path

from docfit_app.api_contract import ServiceError
from docfit_app.pdf_edit_service import PdfEditService

FIXTURES = Path(__file__).resolve().parents[1] / 'pdf_edit_core' / 'tests' / 'fixtures' / 'classify'


class ClassifyFixtureTests(unittest.TestCase):
    def test_expected_json_matches_python_classify(self):
        cases = json.loads((FIXTURES / 'expected.json').read_text(encoding='utf-8'))['cases']
        self.assertGreaterEqual(len(cases), 25)
        for name, want in cases.items():
            with self.subTest(name=name):
                try:
                    PdfEditService._classify(FIXTURES / f'{name}.pdf')
                    got = 'OK'
                except ServiceError as exc:
                    got = exc.code
                self.assertEqual(got, want)


if __name__ == '__main__':
    unittest.main()
