"""RSXML1: 파이썬 처리기와 Rust CLI(hwpx_fmt)의 결과가 같은지(차이 0) 시험한다.

Rust 바이너리가 없으면 건너뛴다(--bin/HWPX_FMT_BIN/CARGO_TARGET_DIR/release/hwpx_fmt). 한/글 COM 은 쓰지 않는다.
"""
import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('rsxml_parity', ROOT / 'scripts' / 'rsxml_parity.py')
parity = importlib.util.module_from_spec(_spec)
sys.modules['rsxml_parity'] = parity
_spec.loader.exec_module(parity)


class NormalizationTest(unittest.TestCase):
    """비교기 자체 시험: 접두사·속성 순서·공백뿐인 꼬리는 무시하고, 글·구조 차이는 잡아야 한다."""

    def norm(self, xml, strict=False):
        return parity.norm_tree(parity.ET.fromstring(xml), strict)

    def test_prefix_and_attribute_order_ignored(self):
        a = self.norm('<a:r xmlns:a="u" x="1" y="2"><a:t>가</a:t></a:r>')
        b = self.norm('<b:r xmlns:b="u" y="2" x="1"><b:t>가</b:t></b:r>')
        self.assertEqual(a, b)

    def test_whitespace_between_elements_ignored_but_text_kept(self):
        a = self.norm('<r xmlns="u">\n  <t>가</t>\n</r>')
        b = self.norm('<r xmlns="u"><t>가</t></r>')
        self.assertEqual(a, b)
        self.assertNotEqual(self.norm('<r xmlns="u"><t>구 분</t></r>'), self.norm('<r xmlns="u"><t>구분</t></r>'))
        self.assertNotEqual(self.norm('<r xmlns="u"><t> </t></r>'), self.norm('<r xmlns="u"><t></t></r>'))
        self.assertNotEqual(self.norm('<r xmlns="u"><t/></r>', True), self.norm('<r xmlns="u">  <t/></r>', True))

    def test_difference_is_reported(self):
        a = self.norm('<r><t a="1">x</t></r>')
        b = self.norm('<r><t a="2">x</t></r>')
        self.assertIn('속성', parity.first_diff(a, b))
        self.assertIsNone(parity.first_diff(a, a))

    def test_compare_outputs_detects_different_zip(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / 'a.hwpx').write_bytes(parity.one(parity.host(parity.grid([['구분', '내용']]))))
            (d / 'b.hwpx').write_bytes(parity.one(parity.host(parity.grid([['구 분', '내용']]))))
            same, diffs, _ = parity.compare_outputs(d / 'a.hwpx', d / 'b.hwpx')
            self.assertFalse(same)
            self.assertTrue(any('글' in x for x in diffs), diffs)


class CorpusTest(unittest.TestCase):
    def test_corpus_size(self):
        self.assertGreaterEqual(len(parity.synthetic_cases()) + len(parity.random_cases()), 30)


@unittest.skipUnless(parity.find_bin(None), 'hwpx_fmt 바이너리가 없습니다(cargo build --release 후 HWPX_FMT_BIN 지정)')
class ParityTest(unittest.TestCase):
    def test_python_and_rust_outputs_identical(self):
        work = Path(tempfile.mkdtemp(prefix='rsxml_parity_test_'))
        try:
            results, failures = parity.run_parity(parity.find_bin(None), work, parity.load_python_processors())
        finally:
            shutil.rmtree(work, ignore_errors=True)
        bad = [(r['doc'], r['processor'], r['diffs']) for r in results if not r['ok']]
        self.assertEqual(failures, 0, bad)
        self.assertGreaterEqual(len({r['doc'] for r in results}), 60)
        # 시험이 공허하지 않도록: 실제로 바뀐 쌍과 둘 다 거부한 쌍이 모두 있어야 한다
        self.assertGreater(sum(1 for r in results if r['py_count']), 30)
        self.assertGreater(sum(1 for r in results if r['py_error'] and r['rs_error']), 10)


if __name__ == '__main__':
    unittest.main()
