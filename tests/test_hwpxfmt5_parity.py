"""구조 서식 하네스가 텍스트·참조·폭 차이를 숨기지 않는지 확인."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('hwpxfmt5_parity', ROOT / 'scripts' / 'hwpxfmt5_parity.py')
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class ComparatorTests(unittest.TestCase):
    def test_normalized_bytes_keep_text_spaces_ids_and_width(self):
        xml = '<hp:p paraPrIDRef="1"><hp:run charPrIDRef="1"><hp:t>구 분</hp:t></hp:run><hp:sz width="100"/></hp:p>'
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            left = path / 'left.hwpx'
            right = path / 'right.hwpx'
            left.write_bytes(harness.base.one(xml))
            expected = harness.normalized_bytes(left)
            for old, new in [('구 분', '구분'), ('charPrIDRef="1"', 'charPrIDRef="2"'), ('width="100"', 'width="101"')]:
                with self.subTest(new=new):
                    right.write_bytes(harness.base.one(xml.replace(old, new)))
                    self.assertNotEqual(expected, harness.normalized_bytes(right))

    def test_normalized_bytes_ignore_prefix_names(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            left = path / 'left.hwpx'; right = path / 'right.hwpx'
            left.write_bytes(harness.base.hwpx_bytes({'Contents/section0.xml': '<a:sec xmlns:a="urn:test"><a:t>글</a:t></a:sec>'}))
            right.write_bytes(harness.base.hwpx_bytes({'Contents/section0.xml': '<b:sec xmlns:b="urn:test"><b:t>글</b:t></b:sec>'}))
            self.assertEqual(harness.normalized_bytes(left), harness.normalized_bytes(right))


@unittest.skipUnless(harness.base.find_bin(None), 'HWPX_FMT_BIN을 지정하면 구조 처리기 차이를 대조합니다')
class NativeParityTests(unittest.TestCase):
    def test_structure_processors_preserve_python_results(self):
        with tempfile.TemporaryDirectory(prefix='hwpxfmt5_parity_test_') as folder:
            result = harness.run(Path(harness.base.find_bin(None)), Path(folder))
        self.assertEqual(result['failed'], 0, [r for r in result['results'] if r['ok'] is False])
        self.assertEqual(result['unsupported_failed'], 0, result['unsupported'])
        self.assertGreaterEqual(result['passed'], 233)
        self.assertGreaterEqual(result['changed'], 84)
        for name in ('attachment', 'box', 'title', 'midtitle'):
            changed = sum(bool(r.get('count')) for r in result['results'] if r['processor'] == name)
            self.assertGreaterEqual(changed, 15, name)


if __name__ == '__main__':
    unittest.main()
