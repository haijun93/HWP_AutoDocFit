"""1쪽 맞춤이 성공하지 못하면 이번 맞춤이 바꾼 글자 자간을 되돌리는지 확인. COM은 실행하지 않는다.

가짜 hwp는 글자마다 자간 7칸을 상태로 들고 있고, 그 자간으로 줄바꿈을 계산한다. 자간 보관·적용·당김은
실제 함수를 그대로 돌린다. 모든 반복은 글자 수 안에서 끝난다.
"""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from docfit_engine.edit_failures import RequiredEditError

ROOT = Path(__file__).resolve().parents[1]
FIELDS = tuple('Spacing' + name for name in
               ('Hangul', 'Latin', 'Hanja', 'Japanese', 'Other', 'Symbol', 'User'))
WIDTH = 1000      # 한 줄 폭. 글자 폭은 100 + 자간


class Boom(RuntimeError):
    pass


def mixed(text):
    """앞 단계 조정과 혼합 서식: 글자마다 -0~-2, 슬롯마다 다른 값."""
    return [tuple(-(i % 3) + (1 if slot == 1 and i % 4 == 0 else 0) for slot in range(7))
            for i in range(len(text))]


class CharHwp:
    """본문 문단(리스트 0)의 글자별 자간을 보관한다."""

    def __init__(self, texts):
        self.texts = list(texts)
        self.chars = [mixed(text) for text in texts]
        self.pos = (0, 0, 0)
        self.selection = None
        self.reads = self.writes = 0
        # fault(문단, 시작, 끝, 쓰기번호) -> None | 'raise'(앞 절반만 적용 뒤 예외) | 'false' | 'ignore'
        self.fault = lambda para, start, end, number: None
        pset = SimpleNamespace(HSet=object())
        self.HParameterSet = SimpleNamespace(HCharShape=pset)

        def get_default(name, hset):
            self.reads += 1
            para, start, _ = self.selection
            for key, value in zip(FIELDS, self.chars[para][start]):
                setattr(pset, key, value)
            return True

        self.HAction = SimpleNamespace(GetDefault=get_default)

    def GetPos(self):
        return self.pos

    def SetPos(self, list_id, para, pos):
        self.pos = (list_id, para, pos)
        return True

    def SelectText(self, spara, spos, epara, epos):
        self.selection = (spara, spos, epos)
        return True

    def run(self, command):
        if command == 'Cancel':
            self.selection = None
        elif command == 'MoveNextChar':
            list_id, para, pos = self.pos
            self.pos = (list_id, para, min(len(self.texts[para]), pos + 1))
        else:
            raise AssertionError(command)
        return True

    def CreateAction(self, name):
        owner = self
        data = {}

        class Action:
            def CreateSet(self):
                return SimpleNamespace(SetItem=data.__setitem__)

            def Execute(self, pset):
                return owner.write(data)

        return Action()

    def write(self, data):
        self.writes += 1
        para, start, end = self.selection
        kind = self.fault(para, start, end, self.writes)
        if kind == 'ignore':
            return True
        stop = start + max(1, (end - start) // 2) if kind == 'raise' else end
        for i in range(start, stop):
            self.chars[para][i] = tuple(data.get(key, old) for key, old in zip(FIELDS, self.chars[para][i]))
        if kind == 'raise':
            raise Boom('일부 적용 뒤 예외')
        return kind != 'false'

    def line_starts(self, para):
        """어절 단위 줄바꿈. 줄 끝 공백은 폭에 넣지 않는다."""
        text, starts, used, index = self.texts[para], [0], 0, 0
        while index < len(text):
            end = text.find(' ', index)
            end = len(text) if end < 0 else end
            word = sum(100 + self.chars[para][i][0] for i in range(index, end))
            if used and used + word > WIDTH:
                starts.append(index)
                used = 0
            used += word
            if end < len(text):
                used += 100 + self.chars[para][end][0]
            index = end + 1
        return starts

    def line_range(self, pos):
        starts = self.line_starts(pos[1]) + [len(self.texts[pos[1]])]
        for start, end in zip(starts, starts[1:]):
            if start <= pos[2] < end:
                return (0, pos[1], start), (0, pos[1], end)
        return (0, pos[1], starts[-2]), (0, pos[1], starts[-1])

    def char(self, pos):
        if pos[2] >= len(self.texts[pos[1]]):
            return None
        return pos, (0, pos[1], pos[2] + 1), self.texts[pos[1]][pos[2]]

    def snapshot(self):
        return [list(chars) for chars in self.chars]


# 문단 0: 두 줄 → 당김 한 번에 한 줄(원본 줄 수 도달).
# 문단 1: 세 줄(원본 1줄) → 첫 당김은 되지만 둘째는 안 돼 '일부만 바뀐' 채 False.
PULL = '가나다라 마바사아자차'
PARTIAL = '카타파하거너더러머버서어 가나다라 마바사아자차'
TWICE = '가나다라 마바사 아자 차카'      # 같은 줄을 두 번 당긴다


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))

    def setUp(self):
        self.use([PULL, PARTIAL])
        self.cancel = False

    def use(self, texts):
        self.doc = CharHwp(texts)
        self.original = self.doc.snapshot()

    def run_fit(self, measure=lambda end: (2, 9), original_lines=None, **extra):
        fn = '보고서_1쪽_맞춤'
        count = len(self.doc.texts)
        values = {
            'hwp': self.doc, 'hwp_run': self.doc.run, '중단_요청됨': lambda: self.cancel,
            '로그': Mock(), '진단로그': Mock(), '작업_모드': 'format', '쪽범위_사용중': lambda: False,
            '쪽맞춤_대상_조사': lambda *a, **k: {'문단': {}, '셀': {}}, '쪽맞춤_레벨_적용': Mock(),
            '쪽맞춤_보고서_끝쪽': measure, '쪽맞춤_원복': Mock(),
            '단어모드_줄범위': self.doc.line_range, '단어모드_한글자': self.doc.char,
            '단어모드_분리정보': lambda anchor: None, '자간조정_본문범위': lambda a, b: (a, b),
            '_문단_줄시작들': lambda n: [(0, n, s) for s in self.doc.line_starts(n)],
            '문단_화면줄수': lambda n: len(self.doc.line_starts(n)),
            '색상_설정': None, '장평_줄이기_사용': False, '자간_변경문단': set(),
        }
        values.update(extra)
        report = {'start': 1, 'end': 2, '문단범위': range(count), '끝': None}
        lines = original_lines or [(1, None)] * count
        with patch.dict(self.ns[fn].__globals__, values):
            return self.ns[fn](report, 원본줄자간=lines)

    def assertOriginal(self):
        self.assertEqual(self.doc.snapshot(), self.original)

    def changed(self):
        return [p for p in range(len(self.doc.chars)) if self.doc.chars[p] != self.original[p]]


class FailureTests(Base):
    def test_fixture_changes_spacing(self):
        """성공 맞춤에서는 당김이 실제로 자간을 바꾸고 그 결과가 남는다."""
        calls = []

        def measure(end):
            calls.append(end)
            return (2, 9) if len(calls) == 1 else (1, 0)
        self.assertTrue(self.run_fit(measure))
        self.assertEqual(self.changed(), [0, 1])
        self.assertEqual(len(self.doc.line_starts(0)), 1)

    def test_failure_restores_each_character(self):
        self.assertFalse(self.run_fit())
        self.assertOriginal()

    def test_partial_pull_with_no_matched_paragraph_restores(self):
        self.use([PARTIAL])
        seen = []

        def measure(end):
            seen.append(self.doc.snapshot())
            return 2, 9
        self.assertFalse(self.run_fit(measure))
        self.assertEqual(len(seen), 1)      # 원본 줄 수에 맞춘 문단 없음
        self.assertGreater(self.doc.writes, 0)
        self.assertOriginal()

    def test_overlapping_pulls_return_to_first_value(self):
        self.use([TWICE])
        self.assertFalse(self.run_fit())
        self.assertGreaterEqual(self.doc.writes, 2)
        self.assertEqual(len(self.doc.line_starts(0)), len(CharHwp([TWICE]).line_starts(0)))
        self.assertOriginal()

    def test_cancel_during_pull_restores(self):
        def fault(para, start, end, number):
            if para == 1:
                self.cancel = True
        self.doc.fault = fault
        self.assertFalse(self.run_fit())
        self.assertOriginal()

    def test_cancel_after_success_measurement_restores(self):
        calls = []

        def measure(end):
            calls.append(end)
            if len(calls) == 1:
                return 2, 9
            self.cancel = True
            return 1, 0
        self.run_fit(measure)
        self.assertOriginal()

    def test_exception_after_pulls_restores_and_propagates(self):
        calls = []

        def measure(end):
            calls.append(end)
            if len(calls) == 2:
                raise Boom('측정')
            return 2, 9
        with self.assertRaises(Boom):
            self.run_fit(measure)
        self.assertOriginal()

    def test_partially_applied_write_restores(self):
        def fault(para, start, end, number):
            if number == self.target:
                return 'raise'
        for self.target in (1, 2, 5):
            with self.subTest(write=self.target):
                self.use([PULL, PARTIAL])
                self.doc.fault = fault
                with self.assertRaises(Boom):
                    self.run_fit()
                self.assertOriginal()

    def test_jangpyeong_helper_spacing_is_restored(self):
        """장평 추가 축소가 함께 줄인 자간도 되돌린다(장평 값은 이 시험의 대상이 아니다)."""
        def helper_apply(runs, delta):
            pass
        self.use([PARTIAL])
        for key in FIELDS:      # 원복 확인이 읽는 장평(이 가짜는 장평을 바꾸지 않는다)
            setattr(self.doc.HParameterSet.HCharShape, key.replace('Spacing', 'Ratio'), 100)
        self.assertFalse(self.run_fit(**{'장평_줄이기_사용': True, '단어모드_장평적용': helper_apply,
                                        '단어모드_서식보관': self.keep_with_ratio}))
        self.assertOriginal()

    def keep_with_ratio(self, start, end, 장평포함=None):
        runs = [((0, start[1], i), (0, start[1], i + 1), self.doc.chars[start[1]][i])
                for i in range(start[2], end[2])]
        return runs, [(start, end, (100,) * 7)]

    def test_unconfirmed_restore_blocks_and_restores_the_rest(self):
        calls = []

        def measure(end):
            calls.append(end)
            if len(calls) == 2:      # 당김이 끝난 뒤부터 문단 0의 쓰기가 먹지 않는다
                self.doc.fault = lambda para, *a: 'ignore' if para == 0 else None
            return 2, 9
        with self.assertRaises(RequiredEditError):
            self.run_fit(measure)
        self.assertEqual(self.doc.chars[1], self.original[1])
        self.assertNotEqual(self.doc.chars[0], self.original[0])

    def test_restore_exception_that_still_applied_is_confirmed(self):
        calls = []

        def measure(end):
            calls.append(end)
            if len(calls) == 2:
                self.doc.fault = lambda *a: 'false'      # 값은 적용되고 반환만 실패
            return 2, 9
        self.assertFalse(self.run_fit(measure))
        self.assertOriginal()

    def test_success_by_later_line_spacing_step_keeps_spacing(self):
        calls = []

        def measure(end):
            calls.append(end)
            return (1, 0) if len(calls) == 3 else (2, 2)
        self.assertTrue(self.run_fit(measure, **{
            '보고서_줄간격_보관': lambda a, b: [], '보고서_줄간격_적용': lambda *a: True}))
        self.assertEqual(self.changed(), [0, 1])

    def test_keeping_original_values_adds_no_reads_on_success(self):
        """1쪽 맞춤의 성공 경로는 당김만 따로 돌린 것과 글자 모양 읽기·쓰기 수가 같다."""
        calls = []

        def measure(end):
            calls.append(end)
            return (2, 9) if len(calls) == 1 else (1, 0)
        self.assertTrue(self.run_fit(measure))
        counts, result = (self.doc.reads, self.doc.writes), self.doc.snapshot()
        self.use([PULL, PARTIAL])
        fn = '문단_줄수_원본맞춤'
        with patch.dict(self.ns[fn].__globals__, {
            'hwp': self.doc, 'hwp_run': self.doc.run, '중단_요청됨': lambda: False, '작업_모드': 'format',
            '로그': Mock(), '진단로그': Mock(), '단어모드_줄범위': self.doc.line_range,
            '단어모드_한글자': self.doc.char, '단어모드_분리정보': lambda anchor: None,
            '자간조정_본문범위': lambda a, b: (a, b), '색상_설정': None, '장평_줄이기_사용': False,
            '자간_변경문단': set(),
            '_문단_줄시작들': lambda n: [(0, n, s) for s in self.doc.line_starts(n)],
        }):
            for para in range(2):
                self.ns[fn](para, 1)
        self.assertEqual((self.doc.reads, self.doc.writes), counts)
        self.assertEqual(self.doc.snapshot(), result)

    def test_record_is_closed_after_the_fit(self):
        namespace = self.ns['보고서_1쪽_맞춤'].__globals__
        self.run_fit()
        self.assertIsNone(namespace.get('쪽맞춤_자간기록'))


if __name__ == '__main__':
    unittest.main()
