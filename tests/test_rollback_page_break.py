"""맞춤이 취소·예외 없이 '실패'해도 풀었던 쪽 나눔을 되살리는지(ROLLBACK3). COM은 실행하지 않는다.

가짜 hwp는 문단 모양을 상태로 들고 있어, 시험은 호출 횟수가 아니라 남은 값을 본다.
"""
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock, patch

from docfit_engine.edit_failures import RequiredEditError
from tests.test_rollback_page_spacing import StatefulHwp

ROOT = Path(__file__).resolve().parents[1]


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))

    def setUp(self):
        self.cancel = False

    def make_doc(self, count, breaks):
        self.doc = StatefulHwp(count)
        for para in breaks:
            self.doc.paras[para]['PagebreakBefore'] = 1
        self.original = self.doc.snapshot()

    def patched(self, fn, **extra):
        values = {'hwp': self.doc, 'hwp_run': lambda command: True, '중단_요청됨': lambda: self.cancel,
                  '로그': Mock(), '진단로그': Mock()}
        values.update(extra)
        return patch.dict(self.ns[fn].__globals__, values)

    def breaks(self):
        return [i for i, p in self.doc.paras.items() if p['PagebreakBefore']]


class ReportFitFailureTests(Base):
    """_문서유형별_쪽맞춤_본체: 보고서 안 쪽 나눔을 푼 뒤 담지 못한 경우."""

    def setUp(self):
        super().setUp()
        self.make_doc(8, (0, 2, 3, 6))
        self.issues = Mock()

    def run_body(self, fit, reports=None, pages_after_release=2,
                 entry='_문서유형별_쪽맞춤_본체', **extra):
        """reports: [(문단범위, 원래 쪽 수)]. 쪽 나눔을 모두 풀어도 pages_after_release쪽으로 남는다."""
        reports = reports or [range(0, 8)]

        def listing(제목문단=None, 대상=None):
            items = [{'문단범위': r, 'start': 1, 'end': pages_after_release if self.released(r) else 3,
                      '끝': (None, None), 'overflow': 3} for r in reports]
            return items, [r.start for r in reports]

        fn = '_문서유형별_쪽맞춤_본체'
        values = {
            '쪽맞춤_원본_보고서': None, '쪽맞춤_보고서_목록': listing,
            '쪽맞춤_문서유형_판정': lambda found, n: ('1쪽 보고서', list(range(len(reports)))),
            '붙임쪽_경계_목록': lambda 표문단=None: [], '붙임쪽_보고서_쪽나눔수': lambda item, bounds: 0,
            '보고서_1쪽_맞춤': fit, '검수_문제_기록': self.issues, '현재_처리파일': 'x',
        }
        values.update(extra)
        with self.patched(fn, **values):
            result = self.ns[entry]()
            self.adjusted = self.ns[fn].__globals__['쪽맞춤_보고서조정됨']
            self.record = self.ns[fn].__globals__['쪽맞춤_해제기록']
            return result

    def released(self, paragraphs):
        return not any(self.doc.paras[i]['PagebreakBefore'] for i in list(paragraphs)[1:])

    def test_normal_fit_failure_restores_breaks(self):
        seen = []

        def fit(*args, **kwargs):
            seen.append(self.breaks())
            return False
        self.assertTrue(self.run_body(fit))
        self.assertEqual(seen, [[0]])               # 맞춤은 지금처럼 쪽 나눔을 푼 상태에서 시도한다
        self.assertEqual(self.doc.snapshot(), self.original)
        self.assertIsNone(self.record)
        self.issues.assert_called_once()
        self.assertTrue(self.adjusted)              # 저장 단계·재확인의 판단은 기존 그대로

    def test_impossible_attachment_skip_restores_breaks(self):
        fit = Mock()
        self.run_body(fit, 붙임쪽_보고서_쪽나눔수=lambda item, bounds: 1)
        fit.assert_not_called()
        self.assertEqual(self.doc.snapshot(), self.original)

    def test_successful_fit_keeps_breaks_released(self):
        writes = []
        self.doc.fault = lambda para, changes: writes.append((para, dict(changes))) and None
        self.run_body(Mock(return_value=True))
        self.assertEqual(self.breaks(), [0])
        self.assertEqual(writes, [(i, {'PagebreakBefore': 0}) for i in (2, 3, 6)])

    def test_release_alone_is_enough_keeps_breaks_released(self):
        fit = Mock()
        self.run_body(fit, pages_after_release=1)
        fit.assert_not_called()
        self.assertEqual(self.breaks(), [0])

    def test_each_report_is_judged_separately(self):
        reports = [range(0, 4), range(4, 8)]
        self.doc.paras[4]['PagebreakBefore'] = 1
        self.run_body(lambda item, number, *a, **k: number == 2, reports=reports)
        self.assertEqual(self.breaks(), [0, 2, 3, 4])    # 1번(실패)은 되살리고 2번(성공)은 푼 채

    def test_unconfirmed_restore_blocks_after_trying_all(self):
        def fit(*args, **kwargs):
            self.doc.fault = lambda para, changes: 'ignore' if para == 2 else None
            return False
        with self.assertRaises(RequiredEditError):
            self.run_body(fit)
        self.assertEqual(self.breaks(), [0, 3, 6])

    def test_restore_exception_after_apply_is_confirmed_by_reading(self):
        def fit(*args, **kwargs):
            self.doc.fault = lambda para, changes: 'raise' if para == 2 else None
            return False
        self.run_body(fit)
        self.assertEqual(self.doc.snapshot(), self.original)

    def test_whole_stage_propagates_unconfirmed_restore(self):
        def fit(*args, **kwargs):
            self.doc.fault = lambda para, changes: 'ignore'
            return False
        fallback = Mock()
        with self.assertRaises(RequiredEditError):
            self.run_body(fit, entry='보고서_페이지수_맞춤_전체_적용', 페이지맞춤_문단간격_사용=True,
                          쪽맞춤_유형판정_사용=True, 마지막쪽_화면줄수=fallback)
        fallback.assert_not_called()


class MidTitleFailureTests(Base):
    """중제목_외톨이_정리: 뒤 □의 쪽 나눔을 풀고도 모으지 못한 경우."""

    def setUp(self):
        super().setUp()
        self.make_doc(4, (2,))

    def run_midtitle(self, break_fault):
        def move(*args, **kwargs):
            self.doc.fault = break_fault
            return False
        fn = '중제목_외톨이_정리'
        with self.patched(fn, **{
            '중제목_묶음_문단': lambda pos: [((0, 2, 0), (0, 3, 1), '본문')],
            '중제목_묶음_쪽': lambda *a: ({2: 2}, (1, 1)), '쪽범위_사용중': lambda: False,
            '세트문장_통계': {'대상': 0, '성공': 0, '실패': 0}, '_묶음_같은쪽_이동': lambda *a, **k: '실패',
            '_묶음_쪽나눔_이동': move, '검수_문제_기록': Mock(), '현재_처리파일': 'x',
        }):
            return self.ns[fn]({'위치': (0, 1, 0), '다음': (0, 2, 0), '표키': 'k', '요약': 's'})

    def test_normal_failure_puts_break_back(self):
        self.assertFalse(self.run_midtitle(lambda para, changes: None))
        self.assertEqual(self.doc.snapshot(), self.original)

    def test_normal_failure_with_ignored_write_blocks(self):
        with self.assertRaises(RequiredEditError):
            self.run_midtitle(lambda para, changes: 'ignore')


if __name__ == '__main__':
    unittest.main()
