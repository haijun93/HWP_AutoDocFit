"""ONECELL1: 칸이 하나인 표는 서식 단계와 컨트롤 단계를 합쳐 한 번만 방문한다. 가짜 hwp 를 쓰며 COM 은 실행하지 않는다.

'다음 단어 당김'은 한 줄에 한 번이라, 같은 줄을 두 번 방문하면 두 번 당겨질 수 있다."""
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock, patch


class CellHwp:
    """칸 목록마다 lines[area] 개 문단(길이 10). 칸의 끝에서 MoveNextChar 는 제자리에 머문다."""

    def __init__(self, lines):
        self.lines = lines
        self.pos = (0, 0, 0)

    def GetPos(self):
        return self.pos

    def SetPos(self, *pos):
        self.pos = tuple(pos) if pos[0] in self.lines else (0, 0, 0)
        return True

    def run(self, command):
        area, para, i = self.pos
        if command == 'MoveLineEnd':
            self.pos = (area, para, 10)
        elif command == 'MoveNextChar':
            if i < 10:
                self.pos = (area, para, i + 1)
            elif para + 1 < self.lines.get(area, 1):
                self.pos = (area, para + 1, 0)


class OneCellVisitTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def setUp(self):
        # 2: 한 칸 표(두 줄), 3·4: 여러 칸 표(또는 표 안 표·글상자·병합 칸 목록)
        self.doc = CellHwp({2: 2, 3: 1, 4: 1})
        self.visits = []
        self.single = {2}
        self.extra = {
            'hwp': self.doc, 'hwp_run': self.doc.run, '중단_요청됨': lambda: False,
            '한칸표_보호영역': set(self.single), '한칸표_영역_목록': lambda: set(self.single),
            '한칸표_처리영역': set(), '디자인표_보호영역': set(), '재검사_대상문단': None,
            '서식표_보호영역_조사': lambda: set(), '쪽범위_사용중': lambda: False, '쪽범위_컨트롤영역': set(),
            '표셀_자간_제외인가': lambda: False, '숫자_표칸인가': lambda: False,
            '괄호_안쪽_공백_정리_문단_처리': Mock(), '쉼표_공백_정리_문단_처리': Mock(),
            '콜론_공백_정리_문단_처리': Mock(), '작업_모드': 'all', '진단로그': Mock(),
            '자간_최대시도_본문': 30, '자간_최대시도_표': 5,
            '자간자동조정': self.adjust,
        }

    def adjust(self, 최대시도=None):
        self.visits.append((self.doc.pos[0], self.doc.pos[1], 최대시도))
        return True

    def run_stages(self, *names, **override):
        fn = self.ns['컨트롤_내부_자간조정']
        with patch.dict(fn.__globals__, {**self.extra, **override}):
            for name in names:
                self.assertTrue(self.ns[name]())

    SINGLE = [(2, 0, 30), (2, 1, 30)]
    OTHERS = [(3, 0, 5), (4, 0, 5)]

    def test_single_cell_lines_are_visited_once_across_both_stages(self):
        self.run_stages('한칸표_자간조정', '컨트롤_내부_자간조정')
        self.assertEqual(self.visits, self.SINGLE + self.OTHERS)

    def test_format_stage_alone_is_unchanged(self):
        self.run_stages('한칸표_자간조정')
        self.assertEqual(self.visits, self.SINGLE)

    def test_control_stage_alone_is_unchanged(self):
        self.run_stages('컨트롤_내부_자간조정')
        self.assertEqual(self.visits, self.SINGLE + self.OTHERS)

    def test_record_is_used_only_by_the_next_control_pass(self):
        self.run_stages('한칸표_자간조정', '컨트롤_내부_자간조정', '컨트롤_내부_자간조정')
        self.assertEqual(self.visits, self.SINGLE + self.OTHERS + self.SINGLE + self.OTHERS)

    def test_second_round_visits_once_again(self):
        self.run_stages(*(['한칸표_자간조정', '컨트롤_내부_자간조정'] * 2))
        self.assertEqual(self.visits, (self.SINGLE + self.OTHERS) * 2)

    def test_area_skipped_by_format_stage_is_not_recorded(self):
        # 서식 단계에서 쪽 범위 밖이라 건너뛴 칸은 컨트롤 단계의 판단에 맡긴다.
        fn = self.ns['컨트롤_내부_자간조정']
        scope = set()
        with patch.dict(fn.__globals__, {**self.extra, '쪽범위_사용중': lambda: True, '쪽범위_컨트롤영역': scope}):
            self.assertTrue(self.ns['한칸표_자간조정']())
            scope.update({2, 3, 4})
            self.assertTrue(fn())
        self.assertEqual(self.visits, self.SINGLE + self.OTHERS)

    def test_area_no_longer_single_cell_is_not_skipped(self):
        # 두 단계 사이에 목록 번호가 밀려 한 칸 표가 아니게 된 영역은 건너뛰지 않는다.
        self.run_stages('한칸표_자간조정', '컨트롤_내부_자간조정', 한칸표_영역_목록=lambda: set())
        self.assertEqual(self.visits, self.SINGLE + [(2, 0, 5), (2, 1, 5)] + self.OTHERS)

    def test_interrupted_format_stage_is_not_recorded(self):
        fn = self.ns['한칸표_자간조정']
        record = set()
        with patch.dict(fn.__globals__, {**self.extra, '한칸표_처리영역': record,
                                         '자간자동조정': lambda 최대시도=None: False}):
            self.assertFalse(fn())
        self.assertEqual(record, set())

    def test_recheck_pass_ignores_the_record(self):
        self.run_stages('컨트롤_내부_자간조정', 재검사_대상문단={(2, 0)}, 한칸표_처리영역={2},
                        재검사_영역인가=lambda area: area == 2)
        self.assertEqual(self.visits, self.SINGLE)


if __name__ == '__main__':
    unittest.main()
