"""IPCJOB3: file_rename apply/undo job 이관(2단계). 한/글 COM 없음."""
import tempfile
import unittest
from pathlib import Path

from docfit_app.api_contract import ServiceError
from docfit_app.application_service import ApplicationService
from docfit_app.file_rename_service import FileRenameService
from docfit_app.jobs import JobManager

RULES = [{"kind": "number", "digits": 2, "per_folder": True}]


def finish(manager, jid, timeout=5):
    thread = manager._jobs[jid]['thread']
    thread.join(timeout)
    assert not thread.is_alive()
    return manager.job_status(jid)


class _Dialogs:
    def __init__(self, paths):
        self.paths = paths

    def select_files(self, *a):
        return {"ok": True, "cancelled": False, "selection_id": "t"}

    def resolve_selection(self, *a):
        return list(self.paths)


class RenameJobTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve() / 'work'
        (self.root / 'sub').mkdir(parents=True)
        for n in ('a.txt', 'b.txt'):
            (self.root / n).write_text(n)
        (self.root / 'sub' / 'c.txt').write_text('c')
        self.service = FileRenameService(Path(self.tmp.name).resolve() / 'settings', dialogs=_Dialogs([str(self.root)]))
        self.service.command('pick_folder', {'recursive': True})
        self.plan = self.service.command('preview', {'rules': RULES})
        self.manager = JobManager()
        self.addCleanup(lambda: self.manager.shutdown(timeout=.1))

    def start(self, op, payload):
        runner = self.service.job_runner(op, payload)
        jid = self.manager.begin('file_rename_' + op, {})
        return self.manager.start('file_rename_' + op, {}, runner, job_id=jid)

    def names(self):
        return sorted(p.relative_to(self.root).as_posix() for p in self.root.rglob('*') if p.is_file())

    def test_apply_job_matches_sync_and_event_order(self):
        jid = self.start('apply', {'plan_id': self.plan['plan_id']})
        status = finish(self.manager, jid)
        self.assertEqual(status['state'], 'done')
        self.assertEqual(status['result']['done'], 3)
        self.assertNotIn('entries', status['result'])
        self.assertTrue(all('_0' in n for n in self.names()))
        types = [e['type'] for e in self.manager.events()['events'] if e['job_id'] == jid]
        self.assertEqual(types[0], 'started')
        self.assertEqual(types[-1], 'done')
        self.assertIn('progress', types)

    def test_undo_job_restores_names(self):
        finish(self.manager, self.start('apply', {'plan_id': self.plan['plan_id']}))
        status = finish(self.manager, self.start('undo', {}))
        self.assertEqual(status['state'], 'done')
        self.assertEqual(status['result']['done'], 3)
        self.assertEqual(self.names(), ['a.txt', 'b.txt', 'sub/c.txt'])

    def test_start_validation_errors_do_not_create_job(self):
        with self.assertRaises(ValueError) as ctx:
            self.service.job_runner('apply', {'plan_id': 'x'})
        self.assertEqual(ctx.exception.code, 'RENAME_PLAN_STALE')
        with self.assertRaises(ValueError) as ctx:
            self.service.job_runner('undo', {})
        self.assertEqual(ctx.exception.code, 'RENAME_NO_HISTORY')
        self.assertEqual(self.manager.events()['events'], [])
        self.assertEqual(self.names(), ['a.txt', 'b.txt', 'sub/c.txt'])

    def test_stale_file_fails_with_envelope_and_changes_nothing(self):
        runner = self.service.job_runner('apply', {'plan_id': self.plan['plan_id']})
        (self.root / 'a.txt').write_text('changed-longer')
        jid = self.manager.begin('file_rename_apply', {})
        status = finish(self.manager, self.manager.start('file_rename_apply', {}, runner, job_id=jid))
        self.assertEqual(status['state'], 'failed')
        self.assertEqual(status['error']['code'], 'RENAME_PLAN_STALE')
        self.assertIn('recoverable', status['error'])
        self.assertEqual(self.names(), ['a.txt', 'b.txt', 'sub/c.txt'])

    def test_cancel_between_groups_keeps_done_and_history_undoable(self):
        calls = []

        def cancelled_after_first_group():
            calls.append(1)
            return len(calls) > 1  # 첫 묶음(가장 깊은 sub/c.txt)만 실행

        result = None
        with self.service._lock:
            plan = self.service._plan
        from docfit_core.file_rename import execute_plan
        result = execute_plan(plan, self.service.history, cancelled=cancelled_after_first_group)
        self.assertEqual(len(result['done']), 1)
        self.assertEqual(len(result['skipped']), 2)
        self.assertEqual(result['failed'], [])
        self.assertTrue((self.root / 'a.txt').exists() and (self.root / 'b.txt').exists())
        from docfit_core.file_rename import undo
        back = undo(self.service.history, result['run_id'])
        self.assertEqual(len(back['done']), 1)
        self.assertEqual(self.names(), ['a.txt', 'b.txt', 'sub/c.txt'])

    def test_cancel_job_reports_cancelled_with_partial(self):
        inner = self.service.job_runner('apply', {'plan_id': self.plan['plan_id']})
        holder = {}

        def runner(report, cancelled):
            self.manager.cancel(holder['jid'])  # 실행 중 취소 요청 → 첫 묶음 확인 지점에서 멈춤
            return inner(report, cancelled)
        holder['jid'] = jid = self.manager.begin('file_rename_apply', {})
        self.manager.start('file_rename_apply', {}, runner, job_id=jid)
        status = finish(self.manager, jid)
        self.assertEqual(status['state'], 'cancelled')
        self.assertEqual(self.names(), ['a.txt', 'b.txt', 'sub/c.txt'])  # 원본 불변
        ev = [e for e in self.manager.events()['events'] if e['type'] == 'cancelled'][0]
        self.assertEqual(ev['data']['partial'], {'success': 0, 'failed': 0, 'skipped': 3})
        # 계획은 소비됨(동기 apply와 같음): 다시 미리보기를 해야 하고, 그 뒤에는 정상 실행된다
        with self.assertRaises(ValueError) as ctx:
            self.service.job_runner('apply', {'plan_id': self.plan['plan_id']})
        self.assertEqual(ctx.exception.code, 'RENAME_PLAN_STALE')
        plan = self.service.command('preview', {'rules': RULES})
        again = finish(self.manager, self.start('apply', {'plan_id': plan['plan_id']}))
        self.assertEqual(again['state'], 'done')

    def test_singleton_rejects_second_apply(self):
        import threading
        gate = threading.Event()
        jid = self.manager.begin('file_rename_apply', {})
        self.manager.start('file_rename_apply', {}, lambda r, c: gate.wait(2), job_id=jid)
        with self.assertRaises(ServiceError) as ctx:
            self.manager.begin('file_rename_apply', {})
        self.assertEqual(ctx.exception.code, 'JOB_RUNNING')
        gate.set()
        finish(self.manager, jid)


class WiringTests(unittest.TestCase):
    def test_dispatch_start_job_and_sync_unchanged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve() / 'w'
            root.mkdir()
            (root / 'x.txt').write_text('x')
            app = ApplicationService(Path(d) / 'settings.json')
            try:
                rename = app._rename()
                rename._dialogs = _Dialogs([str(root)])
                app.dispatch('file_rename', ['pick_folder', {}])
                plan = app.dispatch('file_rename', ['preview', {'rules': RULES}])
                with self.assertRaises(ServiceError) as ctx:
                    app.dispatch('start_job', ['file_rename_apply', {'plan_id': 'bad'}])
                self.assertEqual(ctx.exception.code, 'RENAME_PLAN_STALE')
                jid = app.dispatch('start_job', ['file_rename_apply', {'plan_id': plan['plan_id']}])
                app.jobs._jobs[jid]['thread'].join(5)
                status = app.dispatch('job_status', [jid])
                self.assertEqual(status['state'], 'done')
                self.assertTrue((root / 'x_01.txt').exists())
                jid = app.dispatch('start_job', ['file_rename_undo', {}])
                app.jobs._jobs[jid]['thread'].join(5)
                self.assertEqual(app.dispatch('job_status', [jid])['result']['done'], 1)
                self.assertTrue((root / 'x.txt').exists())
                # 기존 동기 호출 경로는 그대로
                self.assertEqual(app.dispatch('file_rename', ['apply', {'plan_id': 'x'}])['code'], 'RENAME_PLAN_STALE')
            finally:
                app.jobs.shutdown(timeout=.1)


if __name__ == '__main__':
    unittest.main()
