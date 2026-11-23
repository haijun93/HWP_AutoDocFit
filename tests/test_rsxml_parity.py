"""RSXML1~RSXML4: 파이썬 처리기와 Rust CLI(hwpx_fmt)의 결과가 같은지(차이 0) 시험한다.

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

    def test_rsxml2_cases_cover_each_processor_30_times_and_are_valid_hwpx(self):
        import io
        import zipfile
        cases = parity.rsxml2_cases_module().build(parity)
        for name in ('overview_lead', 'page_number', 'spacing_reset'):
            self.assertGreaterEqual(sum(1 for _, jobs in cases.values() if any(j[0] == name for j in jobs)), 30, name)
        for cname, (data, jobs) in cases.items():
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                self.assertIn('Contents/header.xml', z.namelist(), cname)
            for _, cfg in jobs:
                self.assertTrue(cfg is None or cfg in parity.rsxml2_cases_module().CONFIGS, cname)

    def test_python_settings_are_applied_and_restored(self):
        g = {'표준서식_설정': {'keep': 1}, '제목_담당자_글': 'x'}

        def fn():
            pass
        fn.__globals__.update(g)
        restore = parity.apply_py_settings(fn, {'page_number': {'pos': 'A'}, 'title_owner_text': '담당'})
        self.assertEqual(fn.__globals__['표준서식_설정'], {'쪽번호': {'pos': 'A'}})
        self.assertEqual(fn.__globals__['제목_담당자_글'], '담당')
        restore()
        self.assertEqual(fn.__globals__['표준서식_설정'], {'keep': 1})
        self.assertEqual(fn.__globals__['제목_담당자_글'], 'x')
        del fn.__globals__['표준서식_설정'], fn.__globals__['제목_담당자_글']


class Rsxml3CorpusTest(unittest.TestCase):
    NEW = ('asterisk_superscript', 'attachment_font', 'colon_label', 'blank_lines', 'emphasis')

    @classmethod
    def setUpClass(cls):
        parity.load_python_processors()   # 학습 함수(서식표_종류판별 등) 이름공간 준비
        import rsxml3_cases
        cls.mod = rsxml3_cases
        cls.cases = rsxml3_cases.build(parity, parity.PY_NS)

    def test_each_new_processor_has_30_plus_valid_synthetic_documents(self):
        import io
        import zipfile
        for name in self.NEW:
            n = sum(1 for _, jobs in self.cases.values() if any(j[0] == name for j in jobs))
            self.assertGreaterEqual(n, 30, name)
        for cname, (data, jobs) in self.cases.items():
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                self.assertIn('Contents/header.xml', z.namelist(), cname)
            for _, cfg in jobs:
                self.assertTrue(cfg is None or isinstance(cfg, dict) or cfg in self.mod.CONFIGS, cname)

    def test_learned_profiles_come_from_python_learners(self):
        # 프로필은 손으로 쓴 값이 아니라 파이썬 학습 함수가 만든 것이어야 한다(슬롯·강조 항목이 실제로 있다).
        blank = [jobs[0][1]['blank_line_profile'] for n, (_, jobs) in self.cases.items() if n.startswith('blk_example_')]
        self.assertEqual(len(blank), 4)
        self.assertTrue(all(p.get('slots') for p in blank))
        emp = self.cases['emp_example'][1][0][1]['emphasis_words']
        self.assertTrue(any(e.get('bold') for e in emp) and any(e.get('kind') == 'color' for e in emp))

    def test_python_settings_cover_new_keys_and_restore(self):
        def fn():
            pass
        before = dict(fn.__globals__)
        restore = parity.apply_py_settings(fn, {'std_symbols': False, 'symbol_rules': [['ㅇ', 0, '글꼴', 12]], 'colon_label_enabled': False,
                                                'ratio_reduce': True, 'blank_line_profile': {'slots': {'a': 1}}, 'emphasis_words': [{'x': 1}]})
        g = fn.__globals__
        self.assertEqual(g['표준서식_설정'], {'기호_규칙': [['ㅇ', 0, '글꼴', 12]], '빈줄_서식': {'slots': {'a': 1}}, '강조_낱말': [{'x': 1}]})
        self.assertEqual((g['표준서식_기호_사용'], g['알파_콜론라벨_정렬_사용'], g['장평_줄이기_사용']), (False, False, True))
        restore()
        for k in ('표준서식_설정', '표준서식_기호_사용', '알파_콜론라벨_정렬_사용', '장평_줄이기_사용', '제목_담당자_글', '활성_항목기호_체계'):
            self.assertEqual(g.get(k), before.get(k), k)


@unittest.skipUnless(parity.find_bin(None), 'hwpx_fmt 바이너리가 없습니다(cargo build --release 후 HWPX_FMT_BIN 지정)')
class Rsxml3ParityTest(unittest.TestCase):
    NEW = Rsxml3CorpusTest.NEW

    def test_new_processors_identical_and_changing(self):
        work = Path(tempfile.mkdtemp(prefix='rsxml3_parity_test_'))
        try:
            procs = {k: v for k, v in parity.load_python_processors().items() if k in self.NEW}
            old_only = parity.ONLY
            parity.ONLY = set(self.NEW)
            try:
                results, failures = parity.run_parity(parity.find_bin(None), work, procs)
            finally:
                parity.ONLY = old_only
        finally:
            shutil.rmtree(work, ignore_errors=True)
        bad = [(r['doc'], r['processor'], r['diffs']) for r in results if not r['ok']]
        self.assertEqual(failures, 0, bad)
        for name in self.NEW:
            own = [r for r in results if r['processor'] == name and r['kind'] == 'rsxml3']
            self.assertGreaterEqual(len(own), 30, name)
            self.assertGreaterEqual(sum(1 for r in own if r['py_count']), 10, name)

    def test_comparator_catches_deliberately_broken_output(self):
        work = Path(tempfile.mkdtemp(prefix='rsxml3_sabotage_test_'))
        try:
            procs = parity.load_python_processors()
            rows, bad = parity.run_sabotage(parity.find_bin(None), work, procs)
        finally:
            shutil.rmtree(work, ignore_errors=True)
        self.assertEqual(bad, 0, rows)
        self.assertGreaterEqual(sum(len(r['detected']) for r in rows), 20)


class Rsxml4CorpusTest(unittest.TestCase):
    NEW = ('table_width', 'table_style', 'table_style_align', 'connector_tab')

    @classmethod
    def setUpClass(cls):
        parity.load_python_processors()
        import rsxml4_cases
        cls.mod = rsxml4_cases
        cls.cases = rsxml4_cases.build(parity, parity.PY_NS)

    def test_each_processor_has_30_plus_valid_synthetic_documents(self):
        import io
        import zipfile
        for name in self.NEW:
            n = sum(1 for _, jobs in self.cases.values() if any(j[0] == name for j in jobs))
            self.assertGreaterEqual(n, 30, name)
        for cname, (data, jobs) in self.cases.items():
            with zipfile.ZipFile(io.BytesIO(data)) as z:
                self.assertIn('Contents/header.xml', z.namelist(), cname)
            for _, cfg in jobs:
                self.assertTrue(cfg is None or isinstance(cfg, dict) or cfg in self.mod.CONFIGS, cname)

    def test_style_profiles_come_from_python_learner_and_default(self):
        from docfit_core.table_style import valid_style
        for key in ('ts_default', 'ts_learned', 'ts_learned_2head', 'ts_learned_ns0'):
            style = self.mod.CONFIGS[key]['table_style']
            self.assertTrue(valid_style(style), key)
            self.assertGreaterEqual(len(style['borders']), 2, key)
        self.assertFalse(valid_style(self.mod.CONFIGS['ts_invalid_version']['table_style']))
        self.assertIn('ns0:', next(iter(self.mod.CONFIGS['ts_learned_ns0']['table_style']['borders'].values())))
        self.assertTrue(any(c.get('bold') for c in self.mod.CONFIGS['ts_learned']['table_style']['chars'].values()))

    def test_rounding_tie_and_chain_documents_exist(self):
        self.assertGreaterEqual(sum(1 for n in self.cases if n.startswith('w_tie_round_')), 8)
        self.assertGreaterEqual(sum(1 for n in self.cases if n.startswith('a_after_width_')), 20)
        self.assertGreaterEqual(sum(1 for n in self.cases if n.startswith('a_after_style_')), 10)

    def test_python_settings_cover_new_keys_and_restore(self):
        def fn():
            pass
        before = dict(fn.__globals__)
        restore = parity.apply_py_settings(fn, {'table_style': {'version': 1}, 'connector_tab_enabled': False})
        g = fn.__globals__
        self.assertEqual(g['활성_표서식_프로필'], {'version': 1})
        self.assertFalse(g['알파_연결부호_탭정렬_사용'])
        restore()
        for k in ('활성_표서식_프로필', '알파_연결부호_탭정렬_사용'):
            self.assertEqual(g.get(k), before.get(k), k)


@unittest.skipUnless(parity.find_bin(None), 'hwpx_fmt 바이너리가 없습니다(cargo build --release 후 HWPX_FMT_BIN 지정)')
class Rsxml4ParityTest(unittest.TestCase):
    NEW = Rsxml4CorpusTest.NEW

    def test_new_processors_identical_and_changing(self):
        work = Path(tempfile.mkdtemp(prefix='rsxml4_parity_test_'))
        try:
            procs = {k: v for k, v in parity.load_python_processors().items() if k in self.NEW}
            old_only = parity.ONLY
            parity.ONLY = set(self.NEW)
            try:
                results, failures = parity.run_parity(parity.find_bin(None), work, procs)
            finally:
                parity.ONLY = old_only
        finally:
            shutil.rmtree(work, ignore_errors=True)
        bad = [(r['doc'], r['processor'], r['diffs']) for r in results if not r['ok']]
        self.assertEqual(failures, 0, bad)
        for name in self.NEW:
            own = [r for r in results if r['processor'] == name and r['kind'] == 'rsxml4']
            self.assertGreaterEqual(len(own), 30, name)
            self.assertGreaterEqual(sum(1 for r in own if r['py_count']), 15, name)

    def test_sabotage_pairs_cover_new_processors(self):
        covered = {p for _, p, _ in parity.SABOTAGE_PAIRS}
        self.assertTrue(set(self.NEW) <= covered)


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
        # RSXML2 처리기마다 전용 합성 사례 30건 이상, 그중 실제로 바뀐 것이 있어야 한다
        for name in ('overview_lead', 'page_number', 'spacing_reset'):
            own = [r for r in results if r['processor'] == name and r['kind'] == 'rsxml2']
            self.assertGreaterEqual(len(own), 30, name)
            self.assertGreaterEqual(sum(1 for r in own if r['py_count']), 15, name)

    def test_unsupported_settings_are_rejected_not_ignored(self):
        work = Path(tempfile.mkdtemp(prefix='rsxml_unsupported_test_'))
        try:
            results, failures = parity.run_unsupported(parity.find_bin(None), work)
        finally:
            shutil.rmtree(work, ignore_errors=True)
        self.assertEqual(failures, 0, [r for r in results if not r['ok']])
        self.assertGreaterEqual(len(results), 10)


if __name__ == '__main__':
    unittest.main()
