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


if __name__ == '__main__':
    unittest.main()
