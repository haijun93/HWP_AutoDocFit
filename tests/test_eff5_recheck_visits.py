"""EFF5: 재검사 대상이 정해진 순회의 방문·호출 수. 상태형 가짜 hwp 를 쓰며 COM 은 실행하지 않는다.

본문(리스트 0) 순회는 표 칸 목록에 들어가지 않는다(실제 한/글 확인: docs/EFF5_RESULT_2026-11-27.md)."""
from collections import Counter
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock, patch


class BodyHwp:
    """본문 문단 count 개(길이 10)와 표 칸 목록 areas. 이동 명령 이름별 횟수를 센다."""

    def __init__(self, count, areas=()):
        self.count, self.areas = count, tuple(areas)
        self.pos = (0, 0, 0)
        self.calls = Counter()

    def GetPos(self):
        self.calls['GetPos'] += 1
        return self.pos

    def SetPos(self, *pos):
        self.calls['SetPos'] += 1
        self.pos = tuple(pos) if pos[0] == 0 or pos[0] in self.areas else (0, 0, 0)
        return True

    def run(self, command):
        self.calls[command] += 1
        lst, para, i = self.pos
        if command == 'MoveDocBegin':
            self.pos = (0, 0, 0)
        elif command == 'MoveDocEnd':
            self.pos = (0, self.count - 1, 10)
        elif command == 'MoveParaBegin':
            self.pos = (lst, para, 0)
        elif command in ('MoveParaEnd', 'MoveLineEnd'):
            self.pos = (lst, para, 10)
        elif command == 'MoveNextChar':
            if i < 10:
                self.pos = (lst, para, i + 1)
            elif lst == 0 and para + 1 < self.count:
                self.pos = (0, para + 1, 0)
        return True


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def patched(self, name, doc, **extra):
        def start():
            doc.run('MoveDocBegin')

        def advance():
            doc.run('MoveParaEnd')
            end = doc.GetPos()
            doc.run('MoveNextChar')
            return doc.GetPos() != end

        return patch.dict(self.ns[name].__globals__, {
            'hwp': doc, 'hwp_run': doc.run, '순회_시작': start, '범위_다음_문단으로_진행': advance,
            '중단_요청됨': lambda: False, '쪽범위_끝지남': lambda pos: False,
            '쪽범위_안인가': lambda *a: True, '표셀_자간_제외인가': lambda: False,
            '_알파_일괄서식_문서': False, '로그': Mock(), '진단로그': Mock(), **extra,
        })


class IndentVisitTests(Base):
    def run_indent(self, targets, count=100, cell_paras=()):
        doc = BodyHwp(count)
        collect = Mock(return_value={})
        done = []
        with self.patched('문단_내어쓰기_전체_갱신', doc, **{
                '_붙임목록_내어쓰기_대상수집': collect, '현재문단_텍스트': lambda: '□ 글',
                '현재_한칸표인가': lambda: False, '내어쓰기_변경문단': set(),
                '_내어쓰기_한문단': lambda *a: done.append(doc.pos[:2])}):
            self.assertTrue(self.ns['문단_내어쓰기_전체_갱신'](대상문단=targets))
        return doc, collect, done

    def test_no_target_argument_visits_every_paragraph(self):
        doc, collect, done = self.run_indent(None)
        self.assertEqual(done, [(0, i) for i in range(100)])
        self.assertEqual(doc.calls['MoveParaBegin'], 100)
        collect.assert_called_once()

    def test_walk_stops_after_last_target(self):
        doc, collect, done = self.run_indent({(0, 2), (0, 5), (7, 0)})
        self.assertEqual(done, [(0, 2), (0, 5)])          # 처리 대상은 그대로
        self.assertEqual(doc.calls['MoveParaBegin'], 7)   # 0~5 와 대상이 끝난 6번 문단
        self.assertEqual(doc.pos, (0, 99, 10))            # 커서는 전처럼 문서 끝
        collect.assert_called_once()                      # 붙임 문맥 수집은 그대로

    def test_last_paragraph_target_is_not_missed(self):
        doc, _, done = self.run_indent({(0, 0), (0, 99)})
        self.assertEqual(done, [(0, 0), (0, 99)])
        self.assertEqual(doc.calls['MoveParaBegin'], 100)

    def test_cell_only_targets_do_not_walk_the_body(self):
        doc, collect, done = self.run_indent({(7, 0), (9, 1)})
        self.assertEqual(done, [])
        self.assertEqual(sum(doc.calls.values()), 0)
        collect.assert_not_called()


class BodyRecheckVisitTests(Base):
    def run_body(self, name, targets, worker, count=100):
        doc = BodyHwp(count)
        done = []
        extra = {worker: lambda *a, **k: done.append(doc.pos[:2]) or True, '재검사_대상문단': targets}
        if name == '본문_문장부호_처리':
            extra['끝위치추출'] = lambda: (0, count - 1, 10)
        with self.patched(name, doc, **extra):
            self.assertIs(self.ns[name](), True)
        return doc, done

    def check(self, name, worker):
        doc, done = self.run_body(name, {(0, 3), (0, 8)}, worker)
        self.assertEqual(done, [(0, 3), (0, 8)])
        self.assertLessEqual(doc.calls['MoveParaEnd'], 8)   # 대상이 아닌 0~7번만 건너뛰기 이동
        self.assertEqual(doc.pos, (0, 99, 10))
        full, done = self.run_body(name, None, worker, count=20)
        self.assertEqual(sorted(set(done)), [(0, i) for i in range(20)])
        self.assertEqual(full.calls['MoveDocEnd'], 0)

    def test_body_spacing(self):
        self.check('본문_기존자간조정', '자간자동조정')

    def test_body_short_line(self):
        self.check('본문_문장부호_처리', '문장부호_줄병합_시도')


class ControlRecheckTests(Base):
    def run_control(self, name, targets, single=frozenset()):
        doc = BodyHwp(3, areas=(2, 3))
        done = []
        survey = Mock(return_value=set(single))
        guard = Mock(return_value=set())
        worker = lambda *a, **k: done.append(doc.pos[0]) or True
        with self.patched(name, doc, **{
                '재검사_대상문단': targets, '한칸표_영역_목록': survey, '서식표_보호영역_조사': guard,
                '쪽범위_사용중': lambda: False, '숫자_표칸인가': lambda: False,
                '컨트롤_줄병합_대상인가': lambda: True, '자간자동조정': worker,
                '문장부호_줄병합_시도': worker, '작업_모드': 'all', '디자인표_보호영역': set(),
                '괄호_안쪽_공백_정리_문단_처리': Mock(), '쉼표_공백_정리_문단_처리': Mock(),
                '콜론_공백_정리_문단_처리': Mock()}):
            self.assertIs(self.ns[name](), True)
        return doc, done, survey, guard

    def test_body_only_recheck_skips_table_survey(self):
        for name in ('컨트롤_내부_자간조정', '컨트롤_내부_문장부호_처리'):
            with self.subTest(name=name):
                doc, done, survey, guard = self.run_control(name, {(0, 1), (0, 2)})
                self.assertEqual(done, [])
                self.assertEqual(sum(doc.calls.values()), 0)
                survey.assert_not_called()
                guard.assert_not_called()

    def test_cell_target_is_still_processed(self):
        for name in ('컨트롤_내부_자간조정', '컨트롤_내부_문장부호_처리'):
            with self.subTest(name=name):
                _, done, survey, _ = self.run_control(name, {(0, 1), (3, 0)})
                self.assertEqual(done, [3])
                survey.assert_called_once()

    def test_first_pass_still_visits_single_cell_table(self):
        """E04 는 그대로 둔다: 1차 순회는 한 칸 표(영역 2)도 다시 방문한다."""
        _, done, _, _ = self.run_control('컨트롤_내부_자간조정', None, single={2})
        self.assertEqual(done, [2, 3])


if __name__ == '__main__':
    unittest.main()
