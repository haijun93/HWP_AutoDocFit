"""장평 후보 제한과 실패/취소 시 혼합 서식 복원 회귀 시험. COM은 실행하지 않는다."""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class StatefulShape:
    def __init__(self):
        self.values = {}

    def CreateAction(self, name):
        owner = self

        class Action:
            def CreateSet(self):
                data = {}
                return SimpleNamespace(SetItem=data.__setitem__, data=data)

            def Execute(self, params):
                owner.values.update(params.data)
                return True

        return Action()


class RatioRestoreTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def apply_ratio(self, values, delta):
        fn = self.ns['단어모드_장평적용']
        fake = StatefulShape()
        fields = self.ns['_단어모드_장평필드']
        with patch.dict(fn.__globals__, {
            'hwp': fake, 'hwp_run': lambda _: True,
            '단어모드_범위선택': lambda *args: None,
        }):
            fn([((0, 0, 0), (0, 0, 1), values)], delta)
        return tuple(fake.values[key] for key in fields)

    def test_original_80_and_160_are_restored_in_all_language_fields(self):
        values = (80, 160, 90, 100, 110, 120, 130)
        self.assertEqual(self.apply_ratio(values, 0), values)

    def test_new_candidate_keeps_existing_limits(self):
        self.assertEqual(self.apply_ratio((80, 160, 100, 90, 95, 120, 130), -10),
                         (85, 150, 90, 85, 85, 110, 120))


class SearchRollbackTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def search(self, outcome, partial_spacing=False, restore_error=False):
        fn = self.ns['단어_장평_추가축소_시도']
        start, end = (0, 0, 0), (0, 0, 2)
        spacing = [(start, end, (0,) * 7)]
        ratios = [(start, end, (100, 160, 110, 120, 130, 140, 150))]
        state = {'spacing': 0, 'ratio': 0}

        def apply_spacing(runs, delta):
            state['spacing'] = delta
            if partial_spacing and delta:
                raise RuntimeError('자간 일부 적용 후 예외')

        def apply_ratio(runs, delta):
            if restore_error and delta == 0:
                raise RuntimeError('장평 복원 실패')
            state['ratio'] = delta

        def find(maximum, check, **kwargs):
            check(5)
            if outcome == 'cancel':
                raise self.ns['_단계탐색_중단']()
            if outcome == 'exception':
                raise RuntimeError('측정 예외')
            return 5 if outcome == 'success' else None

        error = None
        result = None
        with patch.dict(fn.__globals__, {
            '장평_줄이기_사용': True, '단어_장평_추가축소_최대_단계': 10,
            '단어모드_자간적용': apply_spacing, '단어모드_장평적용': apply_ratio,
            '단어모드_줄범위': lambda _: (start, end),
            '최소_성공단계_탐색': find, '진단로그': lambda _: None,
        }):
            try:
                result = fn(start, end, start, end, 4, 보관=(spacing, ratios))
            except RuntimeError as exc:
                error = exc
        return state, result, error

    def test_cancel_restores_spacing_and_ratio(self):
        state, result, error = self.search('cancel')
        self.assertIsNone(error)
        self.assertFalse(result)
        self.assertEqual(state, {'spacing': 0, 'ratio': 0})

    def test_measurement_exception_restores_both_and_propagates(self):
        state, _, error = self.search('exception')
        self.assertIsNotNone(error)
        self.assertEqual(state, {'spacing': 0, 'ratio': 0})

    def test_partial_spacing_exception_also_restores(self):
        state, _, error = self.search('exception', partial_spacing=True)
        self.assertIsNotNone(error)
        self.assertEqual(state, {'spacing': 0, 'ratio': 0})

    def test_ratio_restore_failure_still_attempts_spacing_restore(self):
        state, _, error = self.search('cancel', restore_error=True)
        self.assertIsNotNone(error)
        self.assertEqual(state['spacing'], 0)

    def test_unsuccessful_search_restores_both(self):
        state, result, error = self.search('failure')
        self.assertIsNone(error)
        self.assertFalse(result)
        self.assertEqual(state, {'spacing': 0, 'ratio': 0})

    def test_success_keeps_candidate(self):
        state, result, error = self.search('success')
        self.assertIsNone(error)
        self.assertTrue(result)
        self.assertEqual(state, {'spacing': -4, 'ratio': -5})


if __name__ == '__main__':
    unittest.main()
