"""1쪽 맞춤이 성공하지 못하면 당김이 바꾼 장평·조정 글자색도 되돌리는지 확인. COM은 실행하지 않는다.

가짜 hwp는 글자마다 자간 7칸·장평 7칸·글자색을 상태로 들고, 자간×장평 폭으로 줄바꿈을 계산한다.
서식 보관·적용·당김·장평 보조·색 적용은 실제 함수를 그대로 돌린다. 모든 반복은 글자 수 안에서 끝난다.
"""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from docfit_engine.edit_failures import RequiredEditError

ROOT = Path(__file__).resolve().parents[1]
SLOTS = ('Hangul', 'Latin', 'Hanja', 'Japanese', 'Other', 'Symbol', 'User')
SPACING = tuple('Spacing' + name for name in SLOTS)
RATIO = tuple('Ratio' + name for name in SLOTS)
KEYS = SPACING + RATIO + ('TextColor',)
WIDTH = 1000
MARK = 0x123456      # 조정 글자색


class Boom(RuntimeError):
    pass


# 자간으로 당겨지는 문단(자간 0 근처).
PLAIN = '가나다라 마바사아자차'
# 이미 자간 -40이라 장평으로만 당겨지는 문단.
NARROW = '가나다라마바사 아자차카 타파하거'
# 첫 당김만 되고 원본 1줄에는 못 미치는 문단.
PARTIAL = '카타파하거너더러머버서어 가나다라 마바사아자차'
# 같은 줄을 두 번 당긴다.
TWICE = '가나다라 마바사 아자 차카'
TEXTS = (PLAIN, NARROW, PARTIAL)


def mixed(text):
    base = -40 if text == NARROW else 0
    chars = []
    for i in range(len(text)):
        values = {key: base + (1 if slot and (i + slot) % 4 == 0 else 0) for slot, key in enumerate(SPACING)}
        values.update({key: 100 + (i + slot) % 3 if slot else 100 for slot, key in enumerate(RATIO)})
        values['TextColor'] = (0, 0xFF, 0xFF00)[i // 3 % 3]
        chars.append(values)
    return chars


class PSet:
    """HCharShape 대신: 글자색을 몇 번 읽었는지 센다."""

    def __init__(self):
        self.__dict__.update(HSet=object(), color_reads=0, values={})

    def __getattr__(self, key):
        if key not in KEYS:
            raise AttributeError(key)
        if key == 'TextColor':
            self.__dict__['color_reads'] += 1
        return self.values[key]

    def __setattr__(self, key, value):
        self.values[key] = value


class CharHwp:
    def __init__(self, texts):
        self.texts = list(texts)
        self.chars = [mixed(text) for text in texts]
        self.pos = (0, 0, 0)
        self.selection = None
        self.reads = 0
        self.writes = []                 # 쓰기마다 'spacing' | 'ratio' | 'color'
        # fault(종류, 문단, 시작, 끝) -> None | 'raise'(앞 절반만 적용 뒤 예외) | 'false' | 'ignore'
        self.fault = lambda kind, para, start, end: None
        self.on_read = lambda: None
        self.pset = PSet()
        self.HParameterSet = SimpleNamespace(HCharShape=self.pset)
        self.HAction = SimpleNamespace(GetDefault=self.get_default, Execute=self.execute_action)

    def get_default(self, name, hset):
        self.reads += 1
        para, start, _ = self.selection
        self.pset.values.clear()
        self.pset.values.update(self.chars[para][start])
        self.on_read()
        return True

    def execute_action(self, name, hset):      # 색상_적용_현재선택 경로
        return self.write({'TextColor': self.pset.values['TextColor']})

    def RGBColor(self, r, g, b):
        return r | g << 8 | b << 16

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
        para, start, end = self.selection
        first = next(iter(data))
        kind = 'color' if first == 'TextColor' else 'ratio' if first in RATIO else 'spacing'
        self.writes.append(kind)
        fault = self.fault(kind, para, start, end)
        if fault == 'ignore':
            return True
        stop = start + max(1, (end - start) // 2) if fault == 'raise' else end
        for i in range(start, stop):
            self.chars[para][i].update(data)
        if fault == 'raise':
            raise Boom('일부 적용 뒤 예외')
        return fault != 'false'

    def line_starts(self, para):
        text, starts, used, index = self.texts[para], [0], 0, 0

        def width(i):
            values = self.chars[para][i]
            return (100 + values['SpacingHangul']) * values['RatioHangul'] / 100

        while index < len(text):
            end = text.find(' ', index)
            end = len(text) if end < 0 else end
            word = sum(width(i) for i in range(index, end))
            if used and used + word > WIDTH:
                starts.append(index)
                used = 0
            used += word
            if end < len(text):
                used += width(end)
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

    def snapshot(self, keys=KEYS):
        return [[tuple(values[key] for key in keys) for values in chars] for chars in self.chars]


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))

    def setUp(self):
        self.use(TEXTS)
        self.cancel = False
        self.ratio_on = True
        self.color = (0x56, 0x34, 0x12)

    def use(self, texts):
        self.doc = CharHwp(texts)
        self.original = self.doc.snapshot()

    def values(self, **extra):
        values = {
            'hwp': self.doc, 'hwp_run': self.doc.run, '중단_요청됨': lambda: self.cancel,
            '로그': Mock(), '진단로그': Mock(), '작업_모드': 'format',
            '단어모드_줄범위': self.doc.line_range, '단어모드_한글자': self.doc.char,
            '단어모드_분리정보': lambda anchor: None, '자간조정_본문범위': lambda a, b: (a, b),
            '_문단_줄시작들': lambda n: [(0, n, s) for s in self.doc.line_starts(n)],
            '색상_설정': self.color, '장평_줄이기_사용': self.ratio_on, '자간_변경문단': set(),
            '자간_최대시도_본문': 40, '다음단어_당김_자간_최대_퍼센트': 10,
        }
        values.update(extra)
        return values

    def run_fit(self, measure=lambda end: (2, 9), **extra):
        fn = '보고서_1쪽_맞춤'
        count = len(self.doc.texts)
        values = self.values(**{
            '쪽범위_사용중': lambda: False, '쪽맞춤_대상_조사': lambda *a, **k: {'문단': {}, '셀': {}},
            '쪽맞춤_레벨_적용': Mock(), '쪽맞춤_보고서_끝쪽': measure, '쪽맞춤_원복': Mock(),
            '문단_화면줄수': lambda n: len(self.doc.line_starts(n)),
        })
        values.update(extra)
        report = {'start': 1, 'end': 2, '문단범위': range(count), '끝': None}
        with patch.dict(self.ns[fn].__globals__, values):
            return self.ns[fn](report, 원본줄자간=[(1, None)] * count)

    def pull_only(self, **extra):
        """1쪽 맞춤 밖(기록 닫힘)에서 같은 당김만 돌린다."""
        fn = '문단_줄수_원본맞춤'
        with patch.dict(self.ns[fn].__globals__, self.values(**extra)):
            for para in range(len(self.doc.texts)):
                self.ns[fn](para, 1)

    def succeed_after_pulls(self):
        calls = []

        def measure(end):
            calls.append(end)
            return (2, 9) if len(calls) == 1 else (1, 0)
        return measure

    def after_pulls(self, action):
        """당김이 끝난 뒤의 측정에서 action을 실행한다."""
        calls = []

        def measure(end):
            calls.append(end)
            if len(calls) == 2:
                action()
            return 2, 9
        return measure

    def assertOriginal(self):
        self.assertEqual(self.doc.snapshot(), self.original)

    def differs(self, keys):
        before = [[tuple(v[KEYS.index(k)] for k in keys) for v in para] for para in self.original]
        return [p for p, (a, b) in enumerate(zip(self.doc.snapshot(keys), before)) if a != b]


class RatioColorTests(Base):
    def test_success_keeps_spacing_ratio_and_color(self):
        self.assertTrue(self.run_fit(self.succeed_after_pulls()))
        self.assertEqual(self.differs(SPACING), [0, 2])
        self.assertEqual(self.differs(RATIO), [1])
        self.assertEqual(self.differs(('TextColor',)), [0, 1, 2])

    def test_success_result_matches_pulls_without_record(self):
        self.assertTrue(self.run_fit(self.succeed_after_pulls()))
        inside, writes = self.doc.snapshot(), self.doc.writes
        self.use(TEXTS)
        self.pull_only()
        self.assertEqual(self.doc.snapshot(), inside)
        self.assertEqual(self.doc.writes, writes)

    def test_failure_restores_ratio_and_color(self):
        self.assertFalse(self.run_fit())
        self.assertIn('ratio', self.doc.writes)
        self.assertIn('color', self.doc.writes)
        self.assertOriginal()

    def test_failure_with_only_ratio_on(self):
        self.color = None
        self.assertFalse(self.run_fit())
        self.assertIn('ratio', self.doc.writes)
        self.assertNotIn('color', self.doc.writes)
        self.assertEqual(self.doc.pset.color_reads, 0)
        self.assertOriginal()

    def test_failure_with_only_color_on(self):
        self.ratio_on = False
        self.assertFalse(self.run_fit())
        self.assertNotIn('ratio', self.doc.writes)
        self.assertIn('color', self.doc.writes)
        self.assertOriginal()

    def test_settings_off_add_no_reads_or_writes(self):
        self.ratio_on, self.color = False, None
        self.assertTrue(self.run_fit(self.succeed_after_pulls()))
        counts = self.doc.reads, self.doc.writes, self.doc.pset.color_reads
        result = self.doc.snapshot()
        self.use(TEXTS)
        self.pull_only()
        self.assertEqual((self.doc.reads, self.doc.writes, self.doc.pset.color_reads), counts)
        self.assertEqual(counts[2], 0)
        self.assertEqual(self.doc.snapshot(), result)

    def test_color_outside_the_body_range_is_restored(self):
        """색은 줄 처음부터 칠한다: 자간 보관에서 빠지는 문두(항목기호)도 되돌린다."""
        def body(start, end):
            return (start[0], start[1], max(start[2], 2)), end
        self.assertFalse(self.run_fit(자간조정_본문범위=body))
        self.assertIn('color', self.doc.writes)
        self.assertOriginal()

    def test_repeated_pulls_return_to_first_color(self):
        self.use([TWICE])
        self.assertFalse(self.run_fit())
        self.assertGreaterEqual(self.doc.writes.count('color'), 2)
        self.assertOriginal()

    def test_cancel_during_ratio_write_restores(self):
        def fault(kind, *a):
            if kind == 'ratio':
                self.cancel = True
        self.doc.fault = fault
        self.assertFalse(self.run_fit())
        self.assertIn('ratio', self.doc.writes)
        self.assertOriginal()

    def test_cancel_after_color_write_restores(self):
        def fault(kind, *a):
            if kind == 'color':
                self.cancel = True
        self.doc.fault = fault
        self.assertFalse(self.run_fit())
        self.assertIn('color', self.doc.writes)
        self.assertOriginal()

    def test_cancel_while_keeping_color_does_not_paint(self):
        self.ratio_on = False
        self.use([PLAIN])

        def on_read():
            if 'spacing' in self.doc.writes:      # 당김 뒤의 첫 읽기 = 색 보관
                self.cancel = True
        self.doc.on_read = on_read
        self.assertFalse(self.run_fit())
        self.assertNotIn('color', self.doc.writes)
        self.assertOriginal()

    def test_exception_after_pulls_restores_and_propagates(self):
        def boom():
            raise Boom('측정')
        with self.assertRaises(Boom):
            self.run_fit(self.after_pulls(boom))
        self.assertOriginal()

    def test_partially_applied_ratio_write_restores(self):
        seen = []

        def fault(kind, *a):
            if kind == 'ratio' and not seen:
                seen.append(kind)
                return 'raise'
        self.doc.fault = fault
        # 장평 보조는 자기 복원 뒤 예외를 그대로 올린다.
        with self.assertRaises(Boom):
            self.run_fit()
        self.assertTrue(seen)
        self.assertOriginal()

    def test_partially_applied_color_write_restores(self):
        seen = []

        def fault(kind, *a):
            if kind == 'color' and not seen:
                seen.append(kind)
                return 'raise'          # 색 적용 오류는 기록만 하고 계속된다
        self.doc.fault = fault
        self.assertFalse(self.run_fit())
        self.assertTrue(seen)
        self.assertOriginal()

    def test_restore_exception_that_still_applied_is_confirmed(self):
        def arm():
            self.doc.fault = lambda *a: 'false'
        self.assertFalse(self.run_fit(self.after_pulls(arm)))
        self.assertOriginal()

    def unconfirmed(self, broken):
        def arm():
            self.doc.fault = lambda kind, *a: 'ignore' if kind == broken else None
        with self.assertRaises(RequiredEditError):
            self.run_fit(self.after_pulls(arm))

    def test_unconfirmed_ratio_restore_blocks_and_restores_the_rest(self):
        self.unconfirmed('ratio')
        self.assertEqual(self.differs(RATIO), [1])
        self.assertEqual(self.differs(SPACING), [])
        self.assertEqual(self.differs(('TextColor',)), [])

    def test_unconfirmed_color_restore_blocks_and_restores_the_rest(self):
        self.unconfirmed('color')
        self.assertEqual(self.differs(('TextColor',)), [0, 1, 2])
        self.assertEqual(self.differs(SPACING + RATIO), [])

    def test_unconfirmed_spacing_restore_still_restores_ratio_and_color(self):
        self.unconfirmed('spacing')
        self.assertEqual(self.differs(RATIO), [])
        self.assertEqual(self.differs(('TextColor',)), [])

    def test_records_are_closed_after_the_fit(self):
        namespace = self.ns['보고서_1쪽_맞춤'].__globals__
        self.run_fit()
        for name in ('쪽맞춤_자간기록', '쪽맞춤_장평기록', '쪽맞춤_색기록'):
            self.assertIsNone(namespace.get(name), name)


if __name__ == '__main__':
    unittest.main()
