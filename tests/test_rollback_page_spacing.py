"""쪽 간격 시도의 취소·예외·부분 적용·복원 실패 원복(E07) 회귀 시험. COM은 실행하지 않는다.

가짜 hwp는 문단 모양과 셀 여백을 상태로 들고 있어, 시험은 호출 횟수가 아니라 남은 값을 본다.
"""
from pathlib import Path
import copy
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from docfit_engine.edit_failures import RequiredEditError

ROOT = Path(__file__).resolve().parents[1]
FIELDS = ('LineSpacingType', 'LineSpacing', 'PrevSpacing', 'NextSpacing', 'PagebreakBefore')


class Boom(RuntimeError):
    pass


class StatefulHwp:
    """본문 문단(리스트 0)의 문단 모양과 표 칸의 세로 여백을 보관한다."""

    def __init__(self, count=4):
        self.paras = {i: {'LineSpacingType': 0, 'LineSpacing': 160, 'PrevSpacing': 2000,
                          'NextSpacing': 1000, 'PagebreakBefore': 0} for i in range(count)}
        self.cells = {2: (0, 141, 141), 3: (1, 200, 100)}
        self.pos = (0, 0, 0)
        # fault(문단, 바뀌는 값) -> None(정상) | 'raise'(적용 뒤 예외) | 'false'(적용 뒤 False) | 'ignore'(적용 안 함)
        self.fault = lambda para, changes: None
        shape = SimpleNamespace(HSet=object())
        self.HParameterSet = SimpleNamespace(HParaShape=shape)
        owner = self

        def get_default(name, hset):
            for key, value in owner.paras[owner.pos[1]].items():
                setattr(shape, key, value)
            return True

        def execute(name, hset):
            return owner.write({k: getattr(shape, k) for k in FIELDS})

        self.HAction = SimpleNamespace(GetDefault=get_default, Execute=execute)
        self.ParaShape = SimpleNamespace(Item=lambda key: owner.paras[owner.pos[1]][key])

    def write(self, values):
        para = self.pos[1]
        changes = {k: v for k, v in values.items() if self.paras[para][k] != v}
        kind = self.fault(para, changes)
        if kind == 'ignore':
            return True
        self.paras[para].update(changes)
        if kind == 'raise':
            raise Boom('적용 중 예외')
        return kind != 'false'

    def CreateAction(self, name):
        owner = self
        data = {}

        class Set:
            def SetItem(self, key, value):
                data[key] = value

            def Item(self, key):
                return data.get(key, owner.paras[owner.pos[1]][key])

        class Action:
            def CreateSet(self):
                return Set()

            def GetDefault(self, pset):
                return True

            def Execute(self, pset):
                return owner.write(data)

        return Action()

    def GetPos(self):
        return self.pos

    def SetPos(self, *pos):
        if pos[0] == 0 and pos[1] not in self.paras:
            return False
        self.pos = (pos[0], pos[1] if len(pos) > 1 else 0, pos[2] if len(pos) > 2 else 0)
        return True

    def LineSpacingMethod(self, name):
        return 0

    def PointToHwpUnit(self, pt):
        return int(pt * 100)      # 실제 한/글처럼 소수를 버린다(0.29pt -> 28)

    def snapshot(self):
        return copy.deepcopy(self.paras), dict(self.cells)


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))

    def setUp(self):
        self.doc = StatefulHwp()
        self.original = self.doc.snapshot()
        self.cancel = False

    def read_cell(self):
        return self.doc.cells.get(self.doc.pos[0])

    def write_cell(self, top_hwpunit=None, bottom_hwpunit=None, 원래값=None):
        area = self.doc.pos[0]
        if 원래값 is not None:
            self.doc.cells[area] = tuple(원래값)
        else:
            has, top, bottom = self.doc.cells[area]
            self.doc.cells[area] = (1, top if top_hwpunit is None else top_hwpunit,
                                    bottom if bottom_hwpunit is None else bottom_hwpunit)
        return True

    def patched(self, fn, **extra):
        values = {
            'hwp': self.doc, 'hwp_run': lambda command: True, '중단_요청됨': lambda: self.cancel,
            '로그': Mock(), '진단로그': Mock(), '현재_한칸표인가': lambda: False,
            '셀_세로여백_읽기': self.read_cell, '셀_세로여백_현재선택': self.write_cell,
        }
        values.update(extra)
        return patch.dict(self.ns[fn].__globals__, values)

    def assertOriginal(self):
        self.assertEqual(self.doc.snapshot(), self.original)


class LinearFitTests(Base):
    """_보고서_페이지수_맞춤_시도_본체 (E07의 직접 대상)."""

    def adjust(self, delta, 최소쪽=None, 위간격=False, 원래값=None):
        key = 'PrevSpacing' if 위간격 else 'NextSpacing'
        for para in self.doc.paras:
            if 원래값 is not None:
                원래값.setdefault((0, para), self.doc.paras[para][key] / 200)
            self.doc.paras[para][key] = max(0, self.doc.paras[para][key] + int(delta * 200))
        return len(self.doc.paras)

    def adjust_cells(self, step, 최소쪽=None, 원래값=None):
        for area, (has, top, bottom) in list(self.doc.cells.items()):
            if 원래값 is not None:
                원래값.setdefault(area, (has, top, bottom))
            self.doc.cells[area] = (1, top // 2, bottom // 2)
        return len(self.doc.cells)

    def run_fit(self, pages, split=lambda 최소쪽=None: False, adjust=None):
        fn = '_보고서_페이지수_맞춤_시도_본체'
        with self.patched(fn, **{
            '구조문단_간격_일괄조정': adjust or self.adjust, '표_셀_세로여백_일괄조정': self.adjust_cells,
            '마지막쪽_화면줄수': pages, '쪽맞춤_묶음분리_있음': split,
            '페이지맞춤_최대_pt': 3.0, '페이지맞춤_스텝_pt': 1.0, '페이지맞춤_표셀세로여백_사용': True,
            '페이지맞춤_뒤쪽범위_쪽수': 1, '페이지맞춤_묶음확인_추가단계': 2,
        }):
            return self.ns[fn](3)

    def test_cancel_between_steps_restores(self):
        def pages():
            self.cancel = True
            return 4, 1
        self.assertFalse(self.run_fit(pages))
        self.assertOriginal()

    def test_measurement_exception_restores_and_propagates(self):
        def pages():
            raise Boom('측정 예외')
        with self.assertRaises(Boom):
            self.run_fit(pages)
        self.assertOriginal()

    def test_partial_adjust_exception_restores(self):
        def adjust(delta, 최소쪽=None, 위간격=False, 원래값=None):
            원래값.setdefault((0, 0), self.doc.paras[0]['NextSpacing'] / 200)
            self.doc.paras[0]['NextSpacing'] -= 200
            raise Boom('두 번째 문단에서 예외')
        with self.assertRaises(Boom):
            self.run_fit(lambda: (4, 1), adjust=adjust)
        self.assertOriginal()

    def test_cancel_during_bundle_check_is_not_success(self):
        def split(최소쪽=None):
            self.cancel = True       # 실제 함수는 취소 때 '걸림 없음'(False)을 돌려준다
            return False
        self.assertFalse(self.run_fit(lambda: (3, 1), split=split))
        self.assertOriginal()

    def test_cancel_during_reapply_restores(self):
        calls = []

        def adjust(delta, 최소쪽=None, 위간격=False, 원래값=None):
            calls.append(위간격)
            count = self.adjust(delta, 최소쪽, 위간격, 원래값)
            if len(calls) == 4:      # 3단계 탐색 뒤 첫 재적용
                self.cancel = True
            return count
        self.assertFalse(self.run_fit(lambda: (3, 1), split=lambda 최소쪽=None: True, adjust=adjust))
        self.assertOriginal()

    def test_ignored_restore_write_blocks_save(self):
        def pages():
            self.doc.fault = lambda para, changes: 'ignore' if para == 1 else None
            self.cancel = True
            return 4, 1
        with self.assertRaises(RequiredEditError):
            self.run_fit(pages)
        for para in (0, 2, 3):       # 한 문단이 실패해도 나머지와 표 셀은 되돌린다
            self.assertEqual(self.doc.paras[para], self.original[0][para])
        self.assertEqual(self.doc.cells, self.original[1])

    def test_success_keeps_candidate(self):
        self.assertTrue(self.run_fit(lambda: (3, 1)))
        self.assertEqual(self.doc.paras[0]['NextSpacing'], 800)


class RestoreHelperTests(Base):
    def test_cell_restore_failure_is_reported_after_trying_all(self):
        values = {2: self.doc.cells[2], 3: self.doc.cells[3]}
        self.doc.cells = {2: (1, 10, 10), 3: (1, 10, 10)}

        def write(원래값=None):
            if self.doc.pos[0] == 2:
                return False
            return self.write_cell(원래값=원래값)
        with self.patched('표_셀_세로여백_복원', **{'셀_세로여백_현재선택': write}):
            with self.assertRaises(RequiredEditError):
                self.ns['표_셀_세로여백_복원'](values)
        self.assertEqual(self.doc.cells[3], self.original[1][3])

    def test_missing_paragraph_is_not_confirmed(self):
        with self.patched('구조문단_간격_복원'):
            with self.assertRaises(RequiredEditError):
                self.ns['구조문단_간격_복원']({(0, 99): 5.0})


class ExactRestoreTests(Base):
    """pt를 거치면 1 HWPUNIT이 잘리는 값(실측: 아래 간격 29 -> 28)도 원래 값으로 돌아와야 한다."""

    def setUp(self):
        super().setUp()
        for para in self.doc.paras.values():
            para['NextSpacing'] = 58
            para['PrevSpacing'] = 58
        self.original = self.doc.snapshot()

    def test_paragraph_restore_is_exact(self):
        for 위간격, key in ((False, 'NextSpacing'), (True, 'PrevSpacing')):
            self.doc.paras[1][key] = 0
            with self.patched('구조문단_간격_복원'):
                self.ns['구조문단_간격_복원']({(0, 1): 0.29}, 위간격=위간격)
        self.assertOriginal()

    def test_above_pull_restore_is_exact(self):
        fn = '_묶음_위간격_축소_당김'
        with self.patched(fn, **{'보고서_묶음_쪽별줄수': lambda p: None, '세트문장_통계': {}}):
            self.ns[fn]([((0, i, 0), 0, 160) for i in range(4)], [], 1, None, 's')
        self.assertOriginal()

    def test_fast_restore_is_exact(self):
        fn = '쪽맞춤_원복'
        survey = {'문단': {(0, 1): [0.29, 0.29]}, '셀': {}, '문단현재': {(0, 1): [0.0, 0.0]}, '셀현재': {},
                  '쓴문단': {((0, 1), 0), ((0, 1), 1)}}
        self.doc.paras[1].update(NextSpacing=0, PrevSpacing=0)
        with self.patched(fn, **{'페이지맞춤_최대_pt': 2.0, '페이지맞춤_스텝_pt': 1.0,
                                 '페이지맞춤_표셀세로여백_최소_pt': 0.0}):
            self.ns[fn](survey)
        self.assertOriginal()


class FastFitTests(Base):
    def survey(self):
        paras = {(0, i): [5.0, 10.0] for i in self.doc.paras}
        return {'문단': paras, '셀': dict(self.doc.cells),
                '문단현재': {k: list(v) for k, v in paras.items()}, '셀현재': dict(self.doc.cells)}

    def run_fast(self, pages, split=lambda 최소쪽=None: False, entry='_보고서_페이지수_맞춤_고속', **extra):
        with self.patched(entry, **{
            '쪽맞춤_대상_조사': lambda *a, **k: self.survey(), '마지막쪽_화면줄수': pages,
            '쪽맞춤_묶음분리_있음': split, '페이지맞춤_최대_pt': 2.0, '페이지맞춤_스텝_pt': 1.0,
            '페이지맞춤_표셀세로여백_최소_pt': 0.0, '페이지맞춤_뒤쪽범위_쪽수': 1,
            '페이지맞춤_묶음확인_추가단계': 1, **extra,
        }):
            return self.ns[entry](3)

    def test_cancel_after_level_restores(self):
        def pages():
            self.cancel = True
            return 3, 1
        self.assertFalse(self.run_fast(pages))
        self.assertOriginal()

    def test_partial_level_exception_restores(self):
        writes = []

        def hwp_run(command):
            if command == 'Cancel':
                writes.append(command)
                if len(writes) == 3:       # 쓰기는 됐지만 기록 전에 예외
                    raise Boom('Cancel 예외')
        with self.assertRaises(Boom):
            self.run_fast(lambda: (3, 1), hwp_run=hwp_run)
        self.assertOriginal()

    def test_restore_failure_blocks_and_does_not_fall_back(self):
        def pages():
            self.doc.fault = lambda para, changes: 'ignore' if para == 0 else None
            return 5, 1                    # 최대 레벨로도 못 닿음 → 원복
        fallback = Mock()
        with self.assertRaises(RequiredEditError):
            self.run_fast(pages, entry='보고서_페이지수_맞춤_시도',
                          _보고서_페이지수_맞춤_시도_본체=fallback, 페이지맞춤_고속_사용=True)
        fallback.assert_not_called()
        self.assertEqual(self.doc.paras[1], self.original[0][1])

    def test_whole_stage_does_not_swallow_restore_failure(self):
        fn = '보고서_페이지수_맞춤_전체_적용'
        fallback = Mock(return_value=(3, 9))
        with self.patched(fn, **{
            '페이지맞춤_문단간격_사용': True, '쪽맞춤_유형판정_사용': True,
            '_문서유형별_쪽맞춤': Mock(side_effect=RequiredEditError('복원 실패')), '마지막쪽_화면줄수': fallback,
        }):
            with self.assertRaises(RequiredEditError):
                self.ns[fn]()
        fallback.assert_not_called()


class BundleSpacingTests(Base):
    GROUP = [((0, 2, 0), (0, 3, 5), '본문')]

    def pull(self, counts):
        fn = '_묶음_위간격_축소_당김'
        backup = [((0, i, 0), 0, 160) for i in range(4)]
        with self.patched(fn, **{'보고서_묶음_쪽별줄수': counts, '세트문장_통계': {}, '표_쪽범위': lambda key: None}):
            return self.ns[fn](backup, self.GROUP, 1, None, 's')

    def test_above_spacing_restored_on_measurement_exception(self):
        with self.assertRaises(Boom):
            self.pull(Mock(side_effect=Boom('측정')))
        self.assertOriginal()

    def test_above_spacing_restored_on_partial_write_exception(self):
        def run(command):
            if command == 'Cancel' and self.doc.pos[1] == 2 and self.doc.paras[2]['PrevSpacing'] != 2000:
                raise Boom('일부 적용 뒤 예외')
        fn = '_묶음_위간격_축소_당김'
        with self.patched(fn, **{'hwp_run': run, '보고서_묶음_쪽별줄수': Mock(), '세트문장_통계': {}}):
            with self.assertRaises(Boom):
                self.ns[fn]([((0, i, 0), 0, 160) for i in range(4)], self.GROUP, 1, None, 's')
        self.assertOriginal()

    def test_above_spacing_restore_failure_blocks(self):
        def counts(paragraphs):
            self.doc.fault = lambda para, changes: 'ignore' if para == 0 else None
            return None                   # 중단·측정 실패
        with self.assertRaises(RequiredEditError):
            self.pull(counts)
        self.assertEqual(self.doc.paras[1:2] if False else self.doc.paras[1], self.original[0][1])

    def move(self, counts, **extra):
        fn = '_묶음_같은쪽_이동'
        with self.patched(fn, **{
            '보고서_묶음_쪽별줄수': counts, '쪽범위_사용중': lambda: False, '세트문장_통계': {},
            '보고서_줄간격_보관': lambda a, b: [((0, i, 0), 0, 160) for i in range(4)],
            '세트문장_최소줄간격_퍼센트': 140, '세트문장_최대줄간격_퍼센트': 180,
            '세트문장_페이지줄간격_최대시도': 2, '_묶음뒤_같은쪽_줄간격_맞춤': Mock(),
            '_묶음_위간격_축소_당김': lambda *a: False, **extra,
        }):
            return self.ns[fn]((0, 3, 0), self.GROUP, {1: 3, 2: 1}, False, 's', 한방향=True)

    def test_partial_line_spacing_apply_restores(self):
        self.doc.fault = lambda para, changes: 'false' if para == 2 and changes.get('LineSpacing') == 150 else None
        with self.assertRaises(RuntimeError):
            self.move(Mock(return_value={1: 3, 2: 1}))
        self.assertOriginal()

    def test_cancel_restores_line_spacing(self):
        def counts(paragraphs):
            self.cancel = True
            return None
        self.assertIsNone(self.move(counts))
        self.assertOriginal()

    def test_line_restore_failure_still_restores_others_and_blocks(self):
        def counts(paragraphs):
            self.doc.fault = lambda para, changes: 'false' if para == 0 else None
            return None

        def fault_false_without_apply(para, changes):
            return 'ignore' if para == 0 else None

        def counts_ignore(paragraphs):
            self.doc.fault = fault_false_without_apply
            return None
        with self.assertRaises(RequiredEditError):
            self.move(counts_ignore)
        for para in (1, 2, 3):
            self.assertEqual(self.doc.paras[para], self.original[0][para])

    def test_success_keeps_line_spacing(self):
        self.assertEqual(self.move(Mock(return_value={1: 4})), '성공')
        self.assertEqual(self.doc.paras[0]['LineSpacing'], 150)

    def test_tail_restore_failure_still_restores_others_and_blocks(self):
        fn = '_묶음뒤_같은쪽_줄간격_맞춤'
        self.doc.paras = {i: dict(self.doc.paras[0], LineSpacing=140 if i == 0 else 170) for i in range(4)}
        self.original = self.doc.snapshot()

        def run(command):
            self.doc.pos = (0, min(3, self.doc.pos[1] + 1), 0)

        def counts(paragraphs):
            self.doc.fault = lambda para, changes: 'false' if para == 1 else None

            def stuck(para, changes):
                return 'ignore' if para == 1 else None
            self.doc.fault = stuck
            return {3: 1}
        with self.patched(fn, **{'hwp_run': run, '본문_컨트롤_문단번호': lambda: set(),
                                 '문단_첫줄_쪽': lambda pos: 2, '보고서_묶음_쪽별줄수': counts}):
            with self.assertRaises(RequiredEditError):
                self.ns[fn]([((0, 0, 0), (0, 0, 3), '본문')], 2)
        for para in (2, 3):
            self.assertEqual(self.doc.paras[para], self.original[0][para])


class PageBreakTests(Base):
    def move(self, counts, **extra):
        fn = '_묶음_쪽나눔_이동'
        with self.patched(fn, **{'쪽나누기_사용': True, '쪽_본문줄수': lambda pos: 30,
                                 '보고서_묶음_쪽별줄수': counts, '쪽배치_쪽나눔문단': set(), **extra}):
            return self.ns[fn]((0, 2, 0), [((0, 2, 0), (0, 3, 1), '본문')], {1: 3, 2: 9}, 's')

    def test_exception_after_break_restores(self):
        with self.assertRaises(Boom):
            self.move(Mock(side_effect=Boom('측정')))
        self.assertOriginal()

    def test_partially_applied_break_restores(self):
        self.doc.fault = lambda para, changes: 'false' if changes.get('PagebreakBefore') == 1 else None
        with self.assertRaises(RuntimeError):
            self.move(Mock())
        self.assertOriginal()

    def test_existing_break_is_kept_when_move_fails(self):
        self.doc.paras[2]['PagebreakBefore'] = 1
        self.original = self.doc.snapshot()
        self.assertFalse(self.move(lambda p: {1: 1, 2: 1}))
        self.assertOriginal()

    def test_break_restore_failure_blocks(self):
        def counts(paragraphs):
            self.doc.fault = lambda para, changes: 'ignore'
            return None
        with self.assertRaises(RequiredEditError):
            self.move(counts)

    def midtitle(self, counts, **extra):
        fn = '중제목_외톨이_정리'
        self.doc.paras[2]['PagebreakBefore'] = 1
        self.original = self.doc.snapshot()
        with self.patched(fn, **{
            '중제목_묶음_문단': lambda pos: [((0, 2, 0), (0, 3, 1), '본문')], '중제목_묶음_쪽': counts,
            '쪽범위_사용중': lambda: False, '세트문장_통계': {'대상': 0, '성공': 0, '실패': 0}, **extra,
        }):
            return self.ns[fn]({'위치': (0, 1, 0), '다음': (0, 2, 0), '표키': 'k', '요약': 's'})

    def test_midtitle_measurement_failure_puts_break_back(self):
        measures = iter([({1: 2}, (2, 2)), (None, None)])
        self.assertFalse(self.midtitle(lambda *a: next(measures)))
        self.assertOriginal()

    def test_midtitle_exception_puts_break_back(self):
        measures = iter([({1: 2}, (2, 2))])

        def counts(*args):
            try:
                return next(measures)
            except StopIteration:
                raise Boom('측정')
        with self.assertRaises(Boom):
            self.midtitle(counts)
        self.assertOriginal()

    def report_body(self, fit):
        fn = '_문서유형별_쪽맞춤_본체'
        self.doc.paras[2]['PagebreakBefore'] = 1
        self.original = self.doc.snapshot()
        report = {'문단범위': range(0, 4), 'start': 1, 'end': 2, '끝': (None, None), 'overflow': 3}
        with self.patched(fn, **{
            '쪽맞춤_원본_보고서': None, '쪽맞춤_보고서_목록': lambda 제목문단=None, 대상=None: ([dict(report)], [0]),
            '쪽맞춤_문서유형_판정': lambda reports, n: ('1쪽 보고서', [0]), '붙임쪽_경계_목록': lambda 표문단=None: [],
            '붙임쪽_보고서_쪽나눔수': lambda item, bounds: 0, '보고서_1쪽_맞춤': fit,
            '검수_문제_기록': Mock(), '현재_처리파일': 'x',
        }):
            return self.ns[fn]()

    def test_removed_report_breaks_return_on_exception(self):
        with self.assertRaises(Boom):
            self.report_body(Mock(side_effect=Boom('맞춤')))
        self.assertOriginal()

    def test_removed_report_breaks_return_on_cancel(self):
        def fit(*args, **kwargs):
            self.cancel = True
            return False
        self.report_body(fit)
        self.assertOriginal()


class OnePageFitTests(Base):
    def run_fit(self, apply_line, measure_end):
        fn = '보고서_1쪽_맞춤'

        def level(survey, value, 위간격_0=False):
            self.doc.paras[0]['NextSpacing'] = 1000 if not value else 0
        with self.patched(fn, **{
            '쪽맞춤_대상_조사': lambda *a, **k: {'문단': {}, '셀': {}}, '쪽맞춤_레벨_적용': level,
            '쪽맞춤_보고서_끝쪽': measure_end, '작업_모드': 'format', '쪽범위_사용중': lambda: False,
            '보고서_줄간격_보관': lambda a, b: [((0, i, 0), 0, 160) for i in range(4)],
            '보고서_줄간격_적용': apply_line,
        }):
            return self.ns[fn]({'start': 1, 'end': 2, '문단범위': range(0, 4), '끝': None})

    def test_line_restore_failure_still_restores_level_and_blocks(self):
        def apply_line(backup, step, 확대=False):
            for pos, kind, value in backup:
                self.doc.paras[pos[1]]['LineSpacing'] = value - 10 * step
            self.doc.fault = lambda para, changes: 'raise' if para == 0 else None
            self.cancel = True
            return True
        with self.assertRaises(RequiredEditError):
            self.doc.fault = lambda para, changes: None

            def stuck(backup, step, 확대=False):
                apply_line(backup, step)
                self.doc.fault = lambda para, changes: 'ignore' if para == 0 else None
                return True
            self.run_fit(stuck, lambda end: (2, 1))
        for para in (1, 2, 3):
            self.assertEqual(self.doc.paras[para], self.original[0][para])
        self.assertEqual(self.doc.paras[0]['NextSpacing'], 1000)


if __name__ == '__main__':
    unittest.main()
