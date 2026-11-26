"""JOBSTATE1: 단계 실패·취소 구분, 재시작 실패, 실패 문서 정리(실제 작업_실행)."""
import os
from pathlib import Path
import queue
import runpy
import tempfile
import unittest
from unittest.mock import Mock, patch

ENTRY = str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py')


def drain(events):
    items = []
    while not events.empty():
        items.append(events.get_nowait())
    return items


class BatchLoopTests(unittest.TestCase):
    """PM: 단계 실패·취소 구분, 재시작 실패, 실패 문서 정리, 이어하기."""

    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(ENTRY)

    def run_batch(self, outcomes, restart='0', extra=None, **kwargs):
        fn = self.ns['작업_실행']
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        files = [str(Path(folder.name) / f'{name}.hwpx') for name in 'abc'[:len(outcomes)]]
        events, calls, stop = queue.Queue(), [], []
        hwp = Mock()

        def document(파일, index, total, punctuation):
            calls.append(files.index(파일))
            outcome = outcomes[files.index(파일)]
            if outcome == 'cancel':
                stop.append(True)
                return False
            if isinstance(outcome, Exception):
                raise outcome
            if outcome:
                fn.__globals__['최종검수_문서목록'].append(
                    {'source': 파일, 'output': None, 'success': True, 'status': 'saved'})
            return outcome

        evaluated = []
        def evaluate(goal, documents, *a, **k):
            evaluated.extend(documents)
            return {'score': 0, 'verdict': '확인 필요', 'criteria': [], 'blockers': []}

        status = Mock()
        settings = {
            'hwp': hwp, '문서_처리': document, 'gui_queue': events, '중단_요청됨': lambda: bool(stop),
            '설정_불러오기': lambda: {}, '이전_작업_임시폴더_정리': lambda: 0, '절전_방지': Mock(),
            '원본_뷰어_분리': Mock(), '원본_뷰어_닫기': Mock(), '작업창_분리': Mock(), '작업창_닫기': Mock(),
            '보안모듈_초기화': Mock(),
            # 실행 시작 때 비워지는 한/글 객체는 연결 단계에서 채워진다.
            '한글_시작': lambda: fn.__globals__.__setitem__('hwp', (extra or {}).get('hwp', hwp)), '로그': Mock(), '상태': status,
            '진행률': Mock(), 'pythoncom': Mock(), 'evaluate_work': evaluate,
        }
        settings.update(extra or {})
        with patch.dict(fn.__globals__, settings), \
                patch.dict(os.environ, {'DOCFIT_RESTART_BETWEEN_DOCS': restart}):
            fn(files, 검수=False, 자동닫기=False, 준말_등록={}, **{
                key: value(files) if callable(value) else value for key, value in kwargs.items()})
        return dict(files=files, calls=calls, events=drain(events), hwp=(extra or {}).get('hwp', hwp),
                    records=evaluated, status=status)

    def final(self, run):
        return [e for e in run['events'] if e[0] in ('finished', 'stopped', 'fatal_error')]

    def test_stage_failure_continues_and_closes_failed_document(self):
        run = self.run_batch([False, True])
        self.assertEqual(run['calls'], [0, 1])
        self.assertEqual(self.final(run)[0][:3], ('finished', 1, 1))
        self.assertEqual([r['status'] for r in run['records']], ['failed', 'saved'])
        self.assertEqual([e[1] for e in run['events'] if e[0] == 'document_error'], run['files'][:1])
        run['hwp'].Clear.assert_called_once_with(1)
        self.assertNotIn('모든 작업 완료', [c.args[0] for c in run['status'].call_args_list])

    def test_exception_also_closes_failed_document(self):
        run = self.run_batch([RuntimeError('Execute 예외'), True])
        self.assertEqual(run['calls'], [0, 1])
        run['hwp'].Clear.assert_called_once_with(1)

    def test_successful_documents_are_not_touched(self):
        run = self.run_batch([True, True])
        run['hwp'].Clear.assert_not_called()
        self.assertEqual(self.final(run)[0][:3], ('finished', 2, 0))
        self.assertIn('모든 작업 완료', [c.args[0] for c in run['status'].call_args_list])

    def test_cancel_stops_without_failure_or_cleanup(self):
        run = self.run_batch(['cancel', True])
        self.assertEqual(run['calls'], [0])
        self.assertEqual([e[0] for e in self.final(run)], ['stopped'])
        self.assertFalse([e for e in run['events'] if e[0] == 'document_error'])
        run['hwp'].Clear.assert_not_called()

    def test_uncloseable_failed_document_leaves_rest_unprocessed(self):
        hwp = Mock()
        hwp.Clear.side_effect = RuntimeError('Clear 예외')
        run = self.run_batch([False, True, True], extra={'hwp': hwp})
        self.assertEqual(run['calls'], [0])
        self.assertEqual([r['status'] for r in run['records']], ['failed', 'unprocessed', 'unprocessed'])
        self.assertEqual(self.final(run)[0][:3], ('finished', 0, 3))

    def test_restart_failure_reports_unprocessed_instead_of_aborting(self):
        create = Mock(side_effect=RuntimeError('새 한/글 실패'))
        run = self.run_batch([True, True, True], restart='1', extra={'한글_COM_인스턴스_생성': create})
        self.assertEqual(run['calls'], [0])
        self.assertEqual([e[0] for e in self.final(run)], ['finished'])
        self.assertEqual(self.final(run)[0][1:3], (1, 2))
        self.assertEqual([r['status'] for r in run['records']], ['saved', 'unprocessed', 'unprocessed'])
        self.assertEqual([e[1:3] for e in run['events'] if e[0] == 'document_state'],
                         [(f, 'unprocessed') for f in run['files'][1:]])

if __name__ == '__main__':
    unittest.main()
