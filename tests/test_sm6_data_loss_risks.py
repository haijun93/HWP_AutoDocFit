"""SM6: 현재 동작 고정 시험 — 데이터 손실 위험 (가)·(나).

실제 한/글 COM·실제 프로세스 종료 없음. 가짜 COM 생성기와 가짜 tasklist/taskkill 실행기만 쓴다.
이 시험은 '현재 동작'을 고정한다. 위험을 고치면(docs/SM6_DATA_LOSS_RISKS_2026-11-12.md 3절)
기대값을 바꿔야 하는 시험이며, 그 사실을 각 시험 이름에 `currently_`로 표시했다.
"""
import runpy
from pathlib import Path
from queue import Queue
from threading import Event
from types import SimpleNamespace as NS
import unittest
from unittest.mock import Mock, patch

from desktop_web_ui import DesktopWebBridge
from docfit_core import hwp_process as hp
from docfit_engine import com_driver


class FakeTasks:
    """tasklist/taskkill 흉내. 살아 있는 Hwp.exe PID 집합만 들고 있다."""

    def __init__(self, *pids):
        self.alive = set(pids)
        self.killed = []

    def __call__(self, args):
        if args[0] == 'tasklist':
            return NS(returncode=0, stdout=''.join(f'"Hwp.exe","{pid}","Console"\n' for pid in sorted(self.alive)))
        if args[0] == 'taskkill':
            pid = int(args[2])
            self.killed.append(pid)
            self.alive.discard(pid)
            return NS(returncode=0)
        raise AssertionError(args)


def _clear_owned():
    for pid in hp.owned_pids():
        hp.forget(pid)


class PreservedResultWindowTests(unittest.TestCase):
    """(가) 자동닫기 끔으로 보존한 결과 창이 다음 작업의 강제 정리에 함께 닫힌다."""

    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))
        cls.gui = cls.ns['HwpAutoDocFitGUI']

    def setUp(self):
        _clear_owned()
        self.addCleanup(_clear_owned)
        self.tasks = FakeTasks()
        patcher = patch.object(hp, '_run', side_effect=self.tasks)
        patcher.start()
        self.addCleanup(patcher.stop)

    def _create(self, new_pid):
        """작업 한/글 시작(한글_시작, C:263~): DispatchEx 전후 tasklist 차이를 소유로 등록하고(C:225~236)
        그 PID를 작업 COM 객체에 묶는다. 같은 namespace를 _detach/_close가 이어 쓴다."""
        def dispatch_ex(prog_id):
            self.tasks.alive.add(new_pid)
            app = Mock(name=f'hwp{new_pid}')
            app.RegisterModule.return_value = True
            return app
        win32 = NS(DispatchEx=dispatch_ex, Dispatch=Mock(side_effect=AssertionError('fallback')))
        self.ns = ns = {'win32': win32, '로그': Mock(), 'pythoncom': Mock(), '비교보기_사용': False,
                        '_표시중인_최상위창_목록': Mock(), '작업창_임베드': Mock(),
                        'REGISTER_MODULE_NAME': 'm', 'REGISTER_MODULE_VALUE': 'v',
                        'hwp': None, '작업_hwp_hwnd': 0, '창_임베드_해제': Mock()}
        ns['한글_COM_인스턴스_생성'] = com_driver._install(_docfit_namespace=ns, _docfit_symbol='한글_COM_인스턴스_생성')
        start = com_driver._install(_docfit_namespace=ns, _docfit_symbol='한글_시작')
        return start()

    def _detach(self):
        """자동닫기 끔의 작업 끝 처리(P:869~870)와 같은 작업창_분리 — 창을 사용자에게 넘긴다."""
        detach = com_driver._install(_docfit_namespace=self.ns, _docfit_symbol='작업창_분리')
        detach()
        return self.ns

    def test_detach_releases_preserved_window_from_ownership(self):
        # SM6 전: 분리 뒤에도 {101}이 소유로 남았다. SM7 (가) 수정 뒤: 넘긴 창은 소유가 아니다.
        app = self._create(101)
        self.assertEqual(hp.owned_pids(), {101})
        self._detach()
        self.assertEqual(hp.owned_pids(), set())
        self.assertIsNone(self.ns['hwp'])
        self.assertEqual(hp.hand_over(app), set())   # 두 번 넘겨도 아무 일 없음

    def test_auto_close_keeps_window_owned_when_quit_fails(self):
        """자동닫기 켬(작업창_닫기)은 창을 넘기지 않는다: Quit 실패로 남은 창은 계속 정리 대상."""
        app = self._create(101)
        app.Quit.side_effect = RuntimeError('quit failed')
        com_driver._install(_docfit_namespace=self.ns, _docfit_symbol='작업창_닫기')()
        self.assertEqual(hp.owned_pids(), {101})
        self.assertEqual(hp.kill_owned(), [101])

    def test_working_window_not_yet_handed_over_stays_owned(self):
        """아직 넘기지 않은 작업 중인 한/글은 지금처럼 정리 대상이다."""
        self._create(101)
        self.assertEqual(hp.kill_owned(), [101])

    def test_stale_registration_on_same_thread_is_not_bound_to_job_window(self):
        """같은 스레드의 다른 한/글 생성(예: 분석용) 등록이 작업 창 묶음에 섞이지 않는다."""
        hp.register_owned({303})      # 앞서 이 스레드가 만든 다른 앱 소유 한/글
        self.tasks.alive.add(303)
        self._create(101)
        self._detach()
        self.assertEqual(hp.owned_pids(), {303})   # 303은 넘기지 않았으므로 소유로 남는다

    def test_stop_grace_of_next_job_spares_preserved_result_window(self):
        # SM6 전: killed == [101, 202](보존 결과 창까지 강제 종료). SM7 (가) 뒤: [202]만.
        self._create(101)          # 작업 1: 결과 창(사용자가 계속 편집)
        self._detach()             # 자동닫기 끔 → 창 보존(사용자에게 넘김)
        self.tasks.alive.add(555)  # 사용자가 직접 연 한/글(소유 아님)
        self._create(202)          # 작업 2 시작
        self.assertEqual(hp.owned_pids(), {202})

        app = NS(root=Mock(), worker=Mock(), running=True, closing=False,
                 stop_button=Mock(), status_var=Mock(), 로그표시=Mock())
        app._중단_유예_확인 = lambda: self.gui._중단_유예_확인(app)
        app.worker.is_alive.return_value = True   # 작업 2가 10초 안에 멈추지 않음
        self.gui.작업중단(app)                      # 사용자가 '중단'
        app.root.after.assert_called_once()
        self.gui._중단_유예_확인(app)              # 10초 뒤 유예 확인(L:4889~4896)

        self.assertEqual(self.tasks.killed, [202])               # 멈춘 작업 2만 강제 종료
        self.assertIn(101, self.tasks.alive)                      # 넘긴 결과 창은 무사
        self.assertIn(555, self.tasks.alive)                      # 사용자 소유 창은 무사
        self.assertIn('강제로 종료', app.status_var.set.call_args.args[0])

    def test_app_close_deadline_spares_preserved_result_window(self):
        # SM6 전: killed == [101, 202]. SM7 (가) 뒤: [202]만.
        self._create(101)
        self._detach()
        self._create(202)
        app = NS(root=Mock(), worker=Mock(), 로그표시=Mock(), _강제_종료=Mock(), _비교창_분리_후_보존=Mock())
        app.worker_종료확인 = Mock()   # 200ms 뒤 재호출 예약만 받는다
        app.worker.is_alive.return_value = True
        with patch('time.monotonic', side_effect=[100, 110]):
            self.gui.worker_종료확인(app)   # 종료 시작
            self.gui.worker_종료확인(app)   # 10초 경과(L:5749~5751)
        self.assertEqual(self.tasks.killed, [202])
        self.assertIn(101, self.tasks.alive)

    def test_preserved_window_without_new_job_is_not_killed_by_idle_close(self):
        """막는 경로 확인: 작업이 없을 때의 정상 종료는 강제 정리를 부르지 않는다(L:5734~5735)."""
        self._create(101)
        self._detach()
        app = NS(closing=False, running=False, root=Mock(), _비교창_분리_후_보존=Mock())
        self.gui.종료(app)
        app.root.destroy.assert_called_once_with()
        self.assertEqual(self.tasks.killed, [])


class DoubleCloseTests(unittest.TestCase):
    """(나) 확인 대화상자가 떠 있는 중 닫기를 다시 누르면 확인 없이 강제 종료된다."""

    def setUp(self):
        _clear_owned()
        self.addCleanup(_clear_owned)
        self.callbacks = Queue()
        self.root = NS(after=lambda delay, cb: self.callbacks.put(cb), destroy=Mock())
        self.gui = NS(root=self.root, running=True, 종료=Mock())   # 작업 진행 중
        self.bridge = DesktopWebBridge(self.gui)
        self.bridge.tk_stopped = Event()
        self.addCleanup(self.bridge._shutdown)
        self.bridge.enable_close_guard()
        self.timers = []

        def timer(seconds, callback, args):
            t = Mock(fire=lambda: callback(*args))
            self.timers.append(t)
            return t

        patcher = patch('desktop_web_ui.threading.Timer', side_effect=timer)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.tasks = FakeTasks(101)
        hp.register_owned({101})   # 앞 작업에서 보존한 결과 창
        run = patch.object(hp, '_run', side_effect=self.tasks)
        run.start()
        self.addCleanup(run.stop)

    def test_second_close_during_acknowledged_confirmation_currently_forces_exit(self):
        self.assertFalse(self.bridge.request_close())           # 1번째 닫기
        identifier = self.bridge._close_request['id']
        # Flutter가 확인을 받고(F:2431) '편집 내용을 버릴까요?' 등을 띄운 상태
        self.assertEqual(self.bridge.acknowledge_close(identifier), {'ok': True})
        self.assertTrue(self.bridge.request_close())            # 2번째 닫기(W:3184~3187)
        self.callbacks.get(timeout=2)()                           # 정리 스레드의 root.destroy 예약 실행

        self.gui.종료.assert_not_called()          # '작업 중단하고 종료할까요?'(L:5724) 없이
        self.root.destroy.assert_called_once_with()
        self.assertTrue(self.bridge.tk_stopped.is_set())
        self.assertEqual(self.tasks.killed, [101])  # 보존한 결과 창도 함께 강제 종료
        # 늦게 온 사용자 응답(취소)은 이미 소용이 없다.
        self.assertIsNone(self.bridge._close_request)

    def test_close_after_user_cancel_asks_again(self):
        """막는 경로 확인: 사용자가 취소로 답한 뒤의 닫기는 새 요청(확인 다시)이다."""
        self.bridge.request_close()
        identifier = self.bridge._close_request['id']
        self.bridge.acknowledge_close(identifier)
        self.assertEqual(self.bridge.resolve_dialog(identifier, False), {'accepted': False})
        self.assertFalse(self.bridge.request_close())
        self.assertEqual(self.tasks.killed, [])


if __name__ == '__main__':
    unittest.main()
