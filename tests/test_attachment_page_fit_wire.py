"""쪽 수 맞춤의 붙임 쪽 경계 연결: XML 경계 목록, 마지막 쪽 '불가능' 판정, 보고서 건너뛰기."""
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'tests' / 'fixtures'


class FakeHwp:
    """문단 번호 → 쪽 번호만 흉내 낸다."""

    def __init__(self, last_para, pages=None):
        self.pos = (0, 0, 0)
        self.last_para = last_para
        self.pages = pages or {}

    def GetPos(self):
        return self.pos

    def SetPos(self, *pos):
        self.pos = tuple(pos)

    def run(self, cmd):
        if cmd == 'MoveDocEnd':
            self.pos = (0, self.last_para, 0)

    def page(self):
        return self.pages.get(self.pos[1])


class AttachmentWireTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))

    def boundaries(self, fixture, tables, last_para):
        fn = self.ns['붙임쪽_경계_목록']
        doc = FakeHwp(last_para)
        with patch.dict(fn.__globals__, {
            'hwp': doc, 'hwp_run': doc.run, '_서식통일_현재_HWPX': lambda: FIXTURES / fixture,
            '본문_표_문단번호': lambda: tables, '진단로그': lambda *a: None,
        }):
            return fn()

    def test_xml_boundaries_for_three_forms(self):
        self.assertEqual(self.boundaries('attp_text.hwpx', {}, 9), [(6, '문단', True), (8, '문단', False)])
        self.assertEqual(self.boundaries('attp_table.hwpx', {5: (0, 5, 0), 7: (0, 7, 0)}, 8), [(5, '표', True)])
        self.assertEqual(self.boundaries('attp_shape.hwpx', {}, 8), [(5, '도형', False)])

    def test_xml_skipped_when_numbers_differ(self):
        self.assertIsNone(self.boundaries('attp_table.hwpx', {5: (0, 5, 0)}, 8))     # 표 위치가 다름
        self.assertIsNone(self.boundaries('attp_text.hwpx', {}, 12))                  # 문단 수가 다름

    def blocked(self, boundaries, lines_before, prev_page=1, new_page=True, com_break=False):
        fn = self.ns['붙임쪽_마지막쪽_막힘']
        doc = FakeHwp(9, pages={i: (1 if i < 6 else 2) for i in range(10)})
        with patch.dict(fn.__globals__, {
            'hwp': doc, 'hwp_run': doc.run, '현재_페이지번호': doc.page,
            '본문_표_문단번호': lambda: {}, '붙임쪽_경계_목록': lambda 표문단=None: boundaries,
            '보고서_새쪽_시작인가': lambda n: new_page, '쪽나눔_켜짐': lambda pos: com_break,
            '_보고서_끝내용': lambda a, b, c: ((0, b, 3), None),
            '쪽맞춤_보고서_끝쪽': lambda end: (prev_page, lines_before),
            '페이지맞춤_최대남은줄수': 4, '진단로그': lambda *a: None,
        }):
            return fn(2)

    def test_last_page_blocked_only_with_page_break_and_full_previous_page(self):
        self.assertIn('문단 6', self.blocked([(6, '표', True)], 20))
        self.assertIn('문단 6', self.blocked([(6, '문단', False)], 20, com_break=True))
        self.assertIsNone(self.blocked([(6, '표', True)], 3))           # 앞 쪽 줄이 적으면 기존대로 시도
        self.assertIsNone(self.blocked([(6, '도형', False)], 20))       # 쪽 나눔 없는 붙임
        self.assertIsNone(self.blocked([(6, '표', True)], 20, new_page=False))
        self.assertIsNone(self.blocked([(3, '표', True)], 20))          # 붙임이 앞 쪽에 있음
        self.assertIsNone(self.blocked([], 20))

    def test_whole_pass_skips_impossible_last_page(self):
        fn = self.ns['보고서_페이지수_맞춤_전체_적용']
        attempt = Mock(return_value=True)
        logs = []
        with patch.dict(fn.__globals__, {
            '페이지맞춤_문단간격_사용': True, '쪽맞춤_유형판정_사용': False, '중단_요청됨': lambda: False,
            '마지막쪽_화면줄수': lambda: (3, 2), '페이지맞춤_최대남은줄수': 4,
            '붙임쪽_마지막쪽_막힘': lambda page: '표 표시 붙임(문단 6), 앞 쪽 20줄',
            '보고서_페이지수_맞춤_시도': attempt, '로그': logs.append, 'hwp_run': lambda c: None,
        }):
            self.assertTrue(fn())
        attempt.assert_not_called()
        self.assertTrue(any('불가능' in line for line in logs))

    def test_whole_pass_still_tries_when_not_blocked(self):
        fn = self.ns['보고서_페이지수_맞춤_전체_적용']
        attempt = Mock(return_value=True)
        with patch.dict(fn.__globals__, {
            '페이지맞춤_문단간격_사용': True, '쪽맞춤_유형판정_사용': False, '중단_요청됨': lambda: False,
            '마지막쪽_화면줄수': lambda: (3, 2), '페이지맞춤_최대남은줄수': 4,
            '붙임쪽_마지막쪽_막힘': lambda page: None,
            '보고서_페이지수_맞춤_시도': attempt, '로그': lambda *a: None, 'hwp_run': lambda c: None,
        }):
            self.assertTrue(fn())
        attempt.assert_called_once_with(2)

    def run_report_body(self, page_breaks, target_fit=True):
        fn = self.ns['_문서유형별_쪽맞춤_본체']
        report = {'문단범위': range(0, 12), 'start': 1, 'end': 2, '끝': (None, None), 'overflow': 3}
        fit, issues = Mock(return_value=target_fit), []
        doc = FakeHwp(11)
        with patch.dict(fn.__globals__, {
            'hwp': doc, '쪽맞춤_원본_보고서': None, '중단_요청됨': lambda: False, '로그': lambda *a: None,
            '쪽맞춤_보고서_목록': lambda 제목문단=None, 대상=None: ([dict(report)], [0]),
            '쪽맞춤_문서유형_판정': lambda reports, n: ('1쪽 보고서', [0]),
            '_보고서_내부_쪽나눔_해제': lambda item: 0,
            '붙임쪽_경계_목록': lambda 표문단=None: [(6, '표', True)],
            '붙임쪽_보고서_쪽나눔수': lambda item, boundaries: page_breaks,
            '보고서_1쪽_맞춤': fit, '검수_문제_기록': lambda f, msg: issues.append(msg), '현재_처리파일': 'x',
        }):
            self.assertTrue(fn())
        return fit, issues

    def test_report_with_page_break_attachment_skips_impossible_fit(self):
        fit, issues = self.run_report_body(1)
        fit.assert_not_called()
        self.assertEqual(len(issues), 1)                  # 예전과 같은 검수 기록(어차피 실패하던 경우)

    def test_report_without_page_break_attachment_fits_as_before(self):
        fit, issues = self.run_report_body(0)
        fit.assert_called_once()
        self.assertEqual(issues, [])

    def test_report_page_break_count(self):
        fn = self.ns['붙임쪽_보고서_쪽나눔수']
        report = {'문단범위': range(10, 30)}
        with patch.dict(fn.__globals__, {
            '쪽나눔_켜짐': lambda pos: pos[1] == 25, '보고서_새쪽_시작인가': lambda n: n != 27,
        }):
            boundaries = [(10, '표', True), (15, '표', True), (20, '문단', False), (25, '도형', False),
                          (27, '문단', True), (40, '표', True)]
            self.assertEqual(fn(report, boundaries), 2)   # 15(XML 쪽 나눔), 25(한/글 쪽 나눔)
            self.assertEqual(fn(report, []), 0)


if __name__ == '__main__':
    unittest.main()
