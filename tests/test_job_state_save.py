"""JOBSTATE1 F04·F05: 저장 뒤 상태와 결과 경로(합성 HWPX 저장)."""
import os
from pathlib import Path
import queue
import runpy
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch

from docfit_app.jobs import JobManager, legacy_event
from tests.gui_support import gui_root
from tests.test_hwpx_core import make_hwpx

ENTRY = str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py')
OK_CHECK = {'ok': True, 'text_similarity': 1.0, 'issues': []}
BAD_CHECK = {'ok': False, 'text_similarity': 0.6,
             'issues': [{'severity': 'error', 'message': '본문 감소'}]}


def drain(events):
    items = []
    while not events.empty():
        items.append(events.get_nowait())
    return items


class SaveHwp:
    """SaveAs 가 실제 합성 HWPX 를 쓰는 가짜 한/글."""

    def __init__(self):
        self.XHwpDocuments = Mock(Count=1)

    def SaveAs(self, Path, Format, arg):
        make_hwpx(__import__('pathlib').Path(Path))
        return True

    def Clear(self, option):
        pass


class AfterSaveTests(unittest.TestCase):
    """F04·F05: 저장에 성공한 뒤의 검수 실패·취소·예외·무결성 실패."""

    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(ENTRY)

    def run_document(self, audit=None, compare=None, cancelled=None):
        fn = self.ns['문서_처리']
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        source = Path(folder.name) / 'source.hwpx'
        make_hwpx(source)
        output = Path(folder.name) / 'result.hwpx'
        events, records = queue.Queue(), []
        settings = {
            'hwp': SaveHwp(), '작업_모드': 'format', '표준서식_사용': False, '준말_등록표': {},
            '쪽범위_요청': (1, 1), '검수_사용': True, '선택_세부작업': {'style_unify': False},
            '작업_반복횟수': 1, '중단_요청됨': cancelled or (lambda: False),
            '로그': Mock(), '진단로그': Mock(), '상태': Mock(), '단계초기화': Mock(), '단계표시': Mock(),
            '한글_문서_열기': lambda *a: True, '원본_뷰어_문서표시': Mock(), '비교보기_임베드_재확인': Mock(),
            '쪽범위_계산': lambda *a: True, '표준서식_선행_적용': lambda *a: None,
            '한칸표_영역_목록': lambda: set(), '디자인표_영역_목록': lambda: set(),
            '보고서_페이지보호_전체해제': lambda: True, '문서_처리_1회': lambda *a: True,
            '처리_쪽수_구하기': lambda: 3, '저장파일명': lambda f: output,
            '저장결과_규칙검수': audit or (lambda *a, **k: {}),
            'compare_documents': compare or (lambda a, b: OK_CHECK),
            '숫자_대조': lambda s: {'status': 'skipped', 'reason': '', 'review': []},
            '최종검수_문서목록': records, '검수_문제목록': [], 'gui_queue': events,
        }
        with patch.dict(fn.__globals__, settings):
            result = fn(str(source), 1, 1)
        return result, records, drain(events), str(source), output

    def assert_saved_once(self, events, source, output):
        self.assertTrue(output.is_file())
        self.assertEqual([e for e in events if e[0] == 'saved'], [('saved', source, str(output), 3)])

    def states(self, events):
        return [e[2] for e in events if e[0] == 'document_state']

    def test_clean_save_keeps_existing_event_and_result(self):
        result, records, events, source, output = self.run_document()
        self.assertIs(result, True)
        self.assert_saved_once(events, source, output)
        self.assertEqual(self.states(events), [])
        self.assertEqual([(r['status'], r['success'], r['integrity_ok']) for r in records],
                         [('saved', True, True)])

    def test_inspection_refusal_keeps_saved_result(self):
        result, records, events, source, output = self.run_document(audit=lambda *a, **k: False)
        self.assertFalse(result)
        self.assert_saved_once(events, source, output)
        self.assertEqual(self.states(events), ['inspection_failed'])
        self.assertEqual([(r['status'], r['success'], r['output']) for r in records],
                         [('inspection_failed', False, str(output))])

    def test_cancel_after_save_is_not_reported_as_inspection_failure(self):
        stop = []
        def audit(*a, **k):
            stop.append(True)
            return False
        result, records, events, source, output = self.run_document(
            audit=audit, cancelled=lambda: bool(stop))
        self.assertFalse(result)
        self.assert_saved_once(events, source, output)
        self.assertEqual(self.states(events), ['cancelled'])
        self.assertEqual([(r['status'], r['output']) for r in records], [('cancelled', str(output))])

    def test_exception_after_save_keeps_output(self):
        def compare(a, b):
            raise RuntimeError('대조 예외')
        result, records, events, source, output = self.run_document(compare=compare)
        self.assertFalse(result)
        self.assert_saved_once(events, source, output)
        self.assertEqual(self.states(events), ['inspection_failed'])
        self.assertEqual([(r['status'], r['output']) for r in records],
                         [('inspection_failed', str(output))])
        self.assertIn('대조 예외', records[0]['error'])

    def test_check_item_error_is_inspection_failure(self):
        checks = {'page_group': {'status': 'error', 'checked': 0, 'issues': [], 'error': '저장 결과 재열기 실패'},
                  'word_check': {'status': 'passed', 'checked': 1, 'issues': []}}
        result, records, events, source, output = self.run_document(audit=lambda *a, **k: dict(checks))
        self.assertFalse(result)
        self.assert_saved_once(events, source, output)
        self.assertEqual(self.states(events), ['inspection_failed'])
        self.assertEqual([(r['status'], r['success'], r['output']) for r in records],
                         [('inspection_failed', False, str(output))])
        self.assertIn('저장 결과 재열기 실패', records[0]['error'])
        # 최종 검수가 쓰는 항목별 결과와 무결성 대조는 남긴다.
        self.assertEqual(records[0]['rule_checks']['page_group']['status'], 'error')
        self.assertIs(records[0]['integrity_ok'], True)

    def test_reopen_failure_in_real_inspection_is_inspection_failure(self):
        audit = self.ns['저장결과_규칙검수']
        opened = Mock(return_value=False)
        with patch.dict(audit.__globals__, {
                '선택_세부작업': {'style_unify': False}, '검수_사용': True, '작업_모드': 'format',
                '중단_요청됨': lambda: False, '한글_문서_열기': opened, '검수_문제_기록': Mock(),
                '로그': Mock(), 'hwp': Mock()}):
            checks = audit('source.hwpx', 'result.hwpx', 결과창=False)
        opened.assert_called_once()
        self.assertEqual({item['status'] for item in checks.values()}, {'error'})
        result, records, events, _, _ = self.run_document(audit=lambda *a, **k: checks)
        self.assertFalse(result)
        self.assertEqual(self.states(events), ['inspection_failed'])

    def test_non_error_check_statuses_stay_saved(self):
        checks = {key: {'status': key, 'checked': 0, 'issues': []}
                  for key in ('passed', 'failed', 'incomplete', 'not_applicable')}
        result, records, events, _, _ = self.run_document(audit=lambda *a, **k: checks)
        self.assertIs(result, True)
        self.assertEqual(self.states(events), [])
        self.assertEqual(records[0]['status'], 'saved')

    def test_check_error_keeps_integrity_failure_visible(self):
        checks = {'page_group': {'status': 'error', 'checked': 0, 'issues': [], 'error': '검사 예외'}}
        result, records, events, _, _ = self.run_document(
            audit=lambda *a, **k: checks, compare=lambda a, b: BAD_CHECK)
        self.assertFalse(result)
        self.assertEqual(self.states(events), ['inspection_failed'])
        self.assertIs(records[0]['integrity_ok'], False)
        self.assertIn('무결성 확인 필요', records[0]['error'])

    def test_integrity_failure_is_a_separate_status(self):
        result, records, events, source, output = self.run_document(compare=lambda a, b: BAD_CHECK)
        self.assertIs(result, True)   # 기존 필드 호환: 파일은 처리·저장됐다.
        self.assert_saved_once(events, source, output)
        self.assertEqual(self.states(events), ['integrity_failed'])
        self.assertEqual([(r['status'], r['success'], r['integrity_ok']) for r in records],
                         [('integrity_failed', True, False)])

    def test_job_api_receives_state_without_changing_saved_contract(self):
        manager = JobManager()
        identifier = manager.begin('all')
        manager.report(identifier, 'started', {})
        legacy_event(manager, identifier, ('saved', 'a.hwpx', 'out.hwpx', 3))
        legacy_event(manager, identifier, ('document_state', 'a.hwpx', 'integrity_failed', '확인 필요'))
        events = manager.events()['events']
        done = next(e['data'] for e in events if e['type'] == 'item_done')
        self.assertEqual((done['status'], done['output']), ('converted', 'out.hwpx'))
        state = next(e['data'] for e in events if e['data'].get('result_status'))
        self.assertEqual((state['source'], state['result_status']), ('a.hwpx', 'integrity_failed'))


class ScreenTests(unittest.TestCase):
    """가짜 작업 스레드를 쓰는 실제 화면."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        patcher = patch.dict(os.environ, {'APPDATA': self.temp.name})
        patcher.start()
        self.addCleanup(patcher.stop)
        ns = runpy.run_path(ENTRY, run_name='job_state_screen_test')
        self.root = gui_root(ns['TkinterDnD'].Tk, ns)
        self.addCleanup(self.root.destroy)
        self.root.withdraw()
        self.root._docfit_hidden_backend = True
        self.app = ns['HwpAutoDocFitGUI'](self.root)
        self.files = []
        for name in 'abc':
            path = Path(self.temp.name) / f'{name}.hwpx'
            make_hwpx(path)
            self.files.append(str(path))
        self.app.files = list(self.files)
        self.output = Path(self.temp.name) / 'b-result.hwpx'
        self.output.write_bytes(b'x')
        self.calls = []
        self.release = threading.Event()
        self.started = threading.Event()
        self.addCleanup(self.release.set)
        self.globals = self.app.작업시작.__globals__
        self.events = queue.Queue()
        self.engine = self.globals['작업_실행']
        stubs = patch.dict(self.globals, {
            '작업_실행': self.fake_job, 'gui_queue': self.events})
        stubs.start()
        self.addCleanup(stubs.stop)
        self.app._서식예시_동기화 = lambda: (None, None)
        self.app._절약_완료창 = Mock()
        self.app._알림 = Mock(return_value=False)

    def fake_job(self, *args, **kwargs):
        self.calls.append((args, kwargs))
        self.started.set()
        self.release.wait(30)   # 사건을 넣는 동안 작업 스레드는 살아 있어야 한다.

    def start(self):
        self.release.set()
        if self.calls:
            self.app.worker.join(10)
        self.release.clear()
        self.started.clear()
        self.app.작업시작('all')
        self.assertTrue(self.app.running)
        self.assertTrue(self.started.wait(10))
        return self.calls[-1]

    def feed(self, *items):
        for item in items:
            self.events.put(item)
        while not self.events.empty():
            self.app.queue_처리()

    def test_integrity_failure_is_shown_separately(self):
        self.start()
        self.feed(('saved', self.files[1], str(self.output), 1),
                  ('document_state', self.files[1], 'integrity_failed', '무결성 확인 필요'),
                  ('finished', 1, 0, 0, 0, 0))
        self.assertEqual(self.app._결과목록[0]['status'], 'integrity_failed')
        self.assertIn('무결성 확인 필요 1개', self.app.status_var.get())
        self.assertTrue(any('b.hwpx' in line for line in self.app._flutter_completion['details']))

if __name__ == '__main__':
    unittest.main()
