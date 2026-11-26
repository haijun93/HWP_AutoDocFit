"""EFF4: 서식 보관의 한/글 호출 수와 원복 값. 상태형 가짜 hwp 를 쓰며 COM 은 실행하지 않는다."""
from collections import Counter
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class CountingHwp:
    """글자마다 자간·장평 7슬롯을 따로 가진 한 문단. 한/글 호출 이름별로 횟수를 센다.

    GetDefault 는 한/글 2020 실측처럼 선택 첫 글자의 값을 돌려준다(SPDMEMO5)."""

    def __init__(self, spacing, ratio, spacing_fields, ratio_fields):
        self.fields = tuple(spacing_fields) + tuple(ratio_fields)
        self.chars = [dict(zip(self.fields, tuple(s) + tuple(r))) for s, r in zip(spacing, ratio)]
        self.calls = Counter()
        self.pos = (0, 0, 0)
        self.selection = None
        owner = self

        class CharShape:
            HSet = None

            def __init__(self):
                self.snapshot = {}

            def __getattr__(self, name):
                if name not in owner.fields:
                    raise AttributeError(name)
                owner.calls['read_ratio' if name.startswith('Ratio') else 'read_spacing'] += 1
                return self.snapshot[name]

        shape = CharShape()

        class HAction:
            def GetDefault(self, action, hset):
                owner.calls['GetDefault'] += 1
                shape.snapshot = dict(owner.chars[owner.selection[0]])

        self.HParameterSet = SimpleNamespace(HCharShape=shape)
        self.HAction = HAction()

    def GetPos(self):
        self.calls['GetPos'] += 1
        return self.pos

    def SetPos(self, *pos):
        self.calls['SetPos'] += 1
        self.pos = tuple(pos)

    def SelectText(self, start_para, start, end_para, end):
        self.calls['SelectText'] += 1
        self.selection = (start, end)
        return True

    def run(self, command):
        self.calls['Run:' + command] += 1
        if command == 'MoveNextChar':
            self.pos = (0, 0, min(len(self.chars), self.pos[2] + 1))
        elif command == 'Cancel':
            self.selection = None
        return True

    def CreateAction(self, name):
        owner = self

        class Action:
            def CreateSet(self):
                data = {}
                return SimpleNamespace(SetItem=data.__setitem__, data=data)

            def Execute(self, params):
                owner.calls['Execute'] += 1
                for index in range(*owner.selection):
                    owner.chars[index].update(params.data)
                return True

        return Action()


# 혼합 서식: 장평 80·160, 슬롯마다 다른 값, 이웃 글자와 같은 구간 포함.
SPACING = [(0,) * 7, (0,) * 7, (-3, -2, -1, 0, 1, 2, 3), (5,) * 7, (0,) * 7, (0,) * 7]
RATIO = [(80,) * 7, (80,) * 7, (160,) * 7, (100,) * 7, (90, 100, 110, 120, 130, 140, 150), (100,) * 7]
LENGTH = len(SPACING)
START, END = (0, 0, 0), (0, 0, LENGTH)


class FormatReadCallTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def make(self):
        return CountingHwp(SPACING, RATIO, self.ns['_단어모드_자간필드'], self.ns['_단어모드_장평필드'])

    def patched(self, fake, ratio_on, **extra):
        return patch.dict(self.ns['단어모드_서식보관'].__globals__, {
            'hwp': fake, 'hwp_run': fake.run, '장평_줄이기_사용': ratio_on,
            '중단_요청됨': lambda: False, '자간조정_본문범위': lambda a, b: (a, b),
            '진단로그': lambda *_: None, **extra,
        })

    def expand(self, runs):
        return [values for start, end, values in runs for _ in range(start[2], end[2])]

    def snapshot(self, name, ratio_on):
        fake = self.make()
        with self.patched(fake, ratio_on):
            result = self.ns[name](START, END)
        return fake, result

    def assert_selection_calls_unchanged(self, fake):
        """장평을 빼도 글자별 선택·GetDefault 는 그대로다(혼합 서식 보존)."""
        self.assertEqual(fake.calls['GetDefault'], LENGTH)
        self.assertEqual(fake.calls['SelectText'], LENGTH)
        self.assertEqual(fake.calls['Run:MoveNextChar'], LENGTH)

    def test_ratio_off_does_not_read_ratio(self):
        fake, (spacing, ratio) = self.snapshot('단어모드_서식보관', False)
        self.assertEqual(fake.calls['read_ratio'], 0)
        self.assertEqual(fake.calls['read_spacing'], 7 * LENGTH)
        self.assert_selection_calls_unchanged(fake)
        self.assertEqual(self.expand(spacing), SPACING)
        # 읽지 않은 장평은 빈 목록이 아니라 None: 모르고 쓰면 조용히 지나가지 않는다.
        self.assertIsNone(ratio)

    def test_ratio_on_reads_every_character(self):
        fake, (spacing, ratio) = self.snapshot('단어모드_서식보관', True)
        self.assertEqual(fake.calls['read_ratio'], 7 * LENGTH)
        self.assertEqual(fake.calls['read_spacing'], 7 * LENGTH)
        self.assert_selection_calls_unchanged(fake)
        self.assertEqual(self.expand(spacing), SPACING)
        self.assertEqual(self.expand(ratio), RATIO)

    def test_spacing_snapshot_never_reads_ratio(self):
        for ratio_on in (False, True):
            with self.subTest(ratio_on=ratio_on):
                fake, spacing = self.snapshot('단어모드_자간보관', ratio_on)
                self.assertEqual(fake.calls['read_ratio'], 0)
                self.assert_selection_calls_unchanged(fake)
                self.assertEqual(self.expand(spacing), SPACING)

    def test_ratio_snapshot_reads_ratio_even_when_setting_off(self):
        fake, ratio = self.snapshot('단어모드_장평보관', False)
        self.assertEqual(fake.calls['read_ratio'], 7 * LENGTH)
        self.assertEqual(self.expand(ratio), RATIO)

    def test_spacing_apply_and_restore_keeps_mixed_values_when_ratio_off(self):
        fake = self.make()
        original = [dict(char) for char in fake.chars]
        with self.patched(fake, False):
            spacing, _ = self.ns['단어모드_서식보관'](START, END)
            self.ns['단어모드_자간적용'](spacing, -4)
            self.assertNotEqual(fake.chars, original)
            self.ns['단어모드_자간적용'](spacing, 0)
        self.assertEqual(fake.chars, original)

    def test_failed_ratio_search_restores_mixed_values(self):
        """ROLLBACK1 계약: 보관 값을 넘겨받은 장평 탐색이 실패하면 80·160 까지 원값으로."""
        fake = self.make()
        original = [dict(char) for char in fake.chars]
        changed = []

        def line_range(_):
            changed.append(fake.chars != original)
            return START, (0, 0, 0)          # 어느 단계에서도 성공하지 않음

        with self.patched(fake, True, 단어_장평_추가축소_최대_단계=30, 단어모드_줄범위=line_range):
            runs = self.ns['단어모드_서식보관'](START, END)
            reads = fake.calls['read_ratio']
            self.assertFalse(self.ns['단어_장평_추가축소_시도'](START, END, START, END, 4, 보관=runs))
        self.assertTrue(any(changed))
        self.assertEqual(fake.chars, original)
        self.assertEqual(fake.calls['read_ratio'], reads)   # 넘겨받은 값을 다시 읽지 않음

    def test_ratio_search_without_snapshot_reads_ratio_when_on(self):
        fake = self.make()
        with self.patched(fake, True, 단어_장평_추가축소_최대_단계=30,
                          단어모드_줄범위=lambda _: (START, (0, 0, 0))):
            self.ns['단어_장평_추가축소_시도'](START, END, START, END, 4)
        self.assertEqual(fake.calls['read_ratio'], 7 * LENGTH)


if __name__ == '__main__':
    unittest.main()
