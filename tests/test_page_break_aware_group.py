"""문단 배치가 원래 쪽 나눔 앞을 넘치게 해 짧은 쪽을 만들지 않는지(ROLLBACK4). COM은 실행하지 않는다.

가짜 hwp는 문단 높이(=줄간격)와 강제 쪽 나눔으로 쪽을 배치하므로, 시험은 남은 문단 모양과 쪽 수를 본다.
"""
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock, patch
from xml.etree.ElementTree import Element

from tests.test_rollback_page_spacing import StatefulHwp

ROOT = Path(__file__).resolve().parents[1]
쪽높이 = 640      # 줄간격 160% 문단 4개


class PagedHwp(StatefulHwp):
    """문단마다 한 줄. 쪽 높이를 넘거나 강제 쪽 나눔을 만나면 다음 쪽."""

    def __init__(self, count, breaks=(), ctrl_enter=()):
        super().__init__(count)
        for para in breaks:
            self.paras[para]['PagebreakBefore'] = 1
        self.ctrl_enter = set(ctrl_enter)

    def pages(self):
        page, used, result = 1, 0, {}
        for i, p in self.paras.items():
            height = p['LineSpacing']
            if used and (p['PagebreakBefore'] or i in self.ctrl_enter or used + height > 쪽높이):
                page, used = page + 1, 0
            result[i] = page
            used += height
        return result

    def page_count(self):
        return max(self.pages().values())

    def run(self, command):
        para = self.pos[1]
        if command == 'MovePageBegin':
            pages = self.pages()
            para = min(i for i in pages if pages[i] == pages[para])
        elif command == 'MoveNextParaBegin':
            para = min(para + 1, len(self.paras) - 1)
        elif command == 'MoveDocEnd':
            para = len(self.paras) - 1
        self.pos = (0, para, 0)
        return True


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))

    def call(self, fn, *args, hwpx=None, **kwargs):
        doc = self.doc
        self.issues = Mock()
        values = {
            'hwp': doc, 'hwp_run': doc.run, '중단_요청됨': lambda: False, '로그': Mock(), '진단로그': Mock(),
            '쪽범위_사용중': lambda: False, '쪽나누기_사용': True, '배치_강제쪽나눔': None,
            '세트문장_통계': {'대상': 0, '성공': 0, '실패': 0, '축소횟수': 0, '확대횟수': 0},
            '세트문장_페이지줄간격_최대시도': 6, '세트문장_최대줄간격_퍼센트': 200, '세트문장_최소줄간격_퍼센트': 140,
            '쪽배치_쪽나눔문단': set(), '검수_문제_기록': self.issues, '현재_처리파일': 'x',
            '문단_첫줄_쪽': lambda start: doc.pages()[start[1]],
            '보고서_묶음_쪽별줄수': self.line_counts, '쪽_본문줄수': lambda pos: 4,
            '_묶음_위간격_축소_당김': lambda *a: False,
            '_묶음뒤_같은쪽_줄간격_맞춤': lambda *a: 0,
            '_서식통일_현재_HWPX': lambda: None if hwpx is None else Path('x.hwpx'),
            '붙임쪽_본문문단': lambda path: hwpx, '붙임쪽_표문단번호': lambda paragraphs: set(),
            '본문_표_문단번호': lambda: {},
        }
        with patch.dict(self.ns[fn].__globals__, values):
            return self.ns[fn](*args, **kwargs)

    def line_counts(self, paragraphs):
        pages, counts = self.doc.pages(), {}
        for start, _, _ in paragraphs:
            counts[pages[start[1]]] = counts.get(pages[start[1]], 0) + 1
        return counts

    @staticmethod
    def bundle(*numbers):
        return [((0, n, 0), (0, n, 1), '본문') for n in numbers]

    def move(self, **kwargs):
        """문단 3·4(1·2쪽에 걸침)를 모은다. 앞 문단을 넓히면 2쪽으로 밀 수 있다."""
        paragraphs = self.bundle(3, 4)
        return self.call('_묶음_같은쪽_이동', (0, 3, 0), paragraphs, self.line_counts(paragraphs), True,
                         '묶음', **kwargs)

    def break_move(self):
        paragraphs = self.bundle(3, 4)
        return self.call('_묶음_쪽나눔_이동', (0, 3, 0), paragraphs, self.line_counts(paragraphs), '묶음')


class ExpandTests(Base):
    def test_expansion_that_overflows_before_break_is_undone(self):
        self.doc = PagedHwp(10, breaks=(8,))
        original = self.doc.snapshot()
        self.assertEqual(self.doc.page_count(), 3)
        self.assertEqual(self.move(), '실패')
        self.assertEqual(self.doc.snapshot(), original)
        self.assertEqual(self.doc.page_count(), 3)

    def test_expansion_with_room_before_break_still_succeeds(self):
        self.doc = PagedHwp(10, breaks=(7,))          # 2쪽에 한 줄 여유
        self.assertEqual(self.move(), '성공')
        self.assertEqual(self.doc.page_count(), 3)
        self.assertEqual(self.doc.paras[7]['PagebreakBefore'], 1)
        self.assertGreater(self.doc.paras[0]['LineSpacing'], 160)

    def test_document_without_break_is_unchanged_behaviour(self):
        self.doc = PagedHwp(12)                       # 넓히면 쪽 수가 늘어도 지금처럼 성공
        self.assertEqual(self.move(), '성공')
        self.assertEqual([self.doc.paras[i]['LineSpacing'] for i in range(4)], [170, 170, 170, 160])
        self.assertEqual(self.doc.page_count(), 4)

    def test_ctrl_enter_break_from_saved_hwpx_is_respected(self):
        self.doc = PagedHwp(10, ctrl_enter=(8,))
        original = self.doc.snapshot()
        saved = [Element('p', pageBreak='1' if i == 8 else '0') for i in range(10)]
        self.assertEqual(self.move(hwpx=saved), '실패')
        self.assertEqual(self.doc.snapshot(), original)

    def test_saved_hwpx_with_other_paragraph_count_is_ignored(self):
        self.doc = PagedHwp(10)
        saved = [Element('p', pageBreak='1') for _ in range(9)]
        self.assertEqual(self.move(hwpx=saved), '성공')


class BreakMoveTests(Base):
    def test_added_break_that_overflows_before_break_is_undone(self):
        self.doc = PagedHwp(10, breaks=(8,))
        original = self.doc.snapshot()
        self.assertFalse(self.break_move())
        self.assertEqual(self.doc.snapshot(), original)

    def test_added_break_with_room_is_kept(self):
        self.doc = PagedHwp(10, breaks=(7,))
        self.assertTrue(self.break_move())
        self.assertEqual(self.doc.page_count(), 3)
        self.assertEqual(self.doc.paras[3]['PagebreakBefore'], 1)


class UnresolvedTests(Base):
    def test_unsolvable_bundle_is_reported_and_page_count_kept(self):
        self.doc = PagedHwp(10, breaks=(8,))
        original = self.doc.snapshot()
        paragraphs = self.bundle(3, 4)
        fn = '세트문장_같은쪽_시도'
        extra = {'보고서_문단역할': lambda text: '본문', '보고서_본문묶음_수집': lambda pos: paragraphs,
                 '쪽보다_긴_묶음인가': lambda *a: False, '보고서_압축조건': lambda counts: False}
        with patch.dict(self.ns[fn].__globals__, extra):
            self.assertTrue(self.call(fn, (0, 3, 0), 'ㅇ 묶음'))
        self.assertEqual(self.doc.snapshot(), original)
        self.issues.assert_called_once()
        self.assertIn('문장 묶음 페이지 분리', self.issues.call_args[0][1])


class BreakListTests(Base):
    def test_released_and_restored_break_updates_the_list(self):
        self.doc = PagedHwp(10, breaks=(8,))
        fn = '쪽나눔_설정'
        with patch.dict(self.ns[fn].__globals__, {'hwp': self.doc, '배치_강제쪽나눔': {8}}):
            self.ns[fn]((0, 8, 0), False)
            self.assertEqual(self.ns[fn].__globals__['배치_강제쪽나눔'], set())
            self.ns[fn]((0, 5, 0), True)
            self.assertEqual(self.ns[fn].__globals__['배치_강제쪽나눔'], {5})


if __name__ == '__main__':
    unittest.main()
