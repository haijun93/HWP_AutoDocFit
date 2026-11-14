import threading
import time
import unittest

from docfit_app.idle_gate import IdleGate, IdleSignals
from docfit_app.idle_scheduler import IdleContext, IdleIndexScheduler, IdleJob

QUIET = IdleSignals(input_idle_seconds=600, cpu_percent=0)
BUSY = IdleSignals(input_idle_seconds=0)


class Probe:
    def __init__(self, signals):
        self.signals = signals

    def sample(self):
        return self.signals


class ListJob:
    """문서 목록을 하나씩 처리하는 가짜 작업."""

    def __init__(self, name="docs", items=3, on_item=None):
        self.name = name
        self.items = list(range(items))
        self.processed = []
        self.on_item = on_item or (lambda index, ctx: None)
        self.runs = 0

    def pending(self):
        return True

    def run(self, ctx):
        self.runs += 1
        total = len(self.items)
        for index in self.items:
            if not ctx.checkpoint():
                return
            self.processed.append(index)
            self.on_item(index, ctx)
            ctx.progress(len(self.processed), total)


def wait_for(predicate, timeout=3.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.01)
    return False


class IdleContextTests(unittest.TestCase):
    def test_checkpoint_yields_until_idle_then_continues(self):
        probe = Probe(BUSY)
        gate = IdleGate(probe, sample_interval=0)
        ctx = IdleContext(gate, "x", poll_seconds=0.01)
        result = {}
        worker = threading.Thread(target=lambda: result.setdefault("ok", ctx.checkpoint()))
        worker.start()
        self.assertTrue(wait_for(lambda: ctx.yielded))
        probe.signals = QUIET
        worker.join(2)
        self.assertTrue(result["ok"])
        self.assertFalse(ctx.yielded)

    def test_checkpoint_false_on_stop_or_cancel(self):
        gate = IdleGate(Probe(BUSY), sample_interval=0)
        ctx = IdleContext(gate, "x", poll_seconds=0.01)
        cancel = threading.Event()
        cancel.set()
        self.assertFalse(ctx.checkpoint(cancel))
        ctx.stop()
        self.assertFalse(ctx.checkpoint())

    def test_promote_skips_gate(self):
        gate = IdleGate(Probe(BUSY), sample_interval=0)
        ctx = IdleContext(gate, "x", poll_seconds=0.01)
        ctx.promote()
        self.assertTrue(ctx.checkpoint())

    def test_list_job_satisfies_protocol(self):
        self.assertIsInstance(ListJob(), IdleJob)


class SchedulerTests(unittest.TestCase):
    def setUp(self):
        self.probe = Probe(QUIET)
        self.scheduler = IdleIndexScheduler(IdleGate(self.probe, sample_interval=0), poll_seconds=0.01,
                                            sleep_seconds=0.05)

    def tearDown(self):
        self.scheduler.close()

    def test_request_runs_job_when_idle(self):
        job = ListJob()
        self.scheduler.register(job)
        self.assertEqual(self.scheduler.status()["jobs"]["docs"]["state"], "none")
        self.assertTrue(self.scheduler.request("docs"))
        self.assertTrue(self.scheduler.wait_idle_queue())
        self.assertEqual(job.processed, [0, 1, 2])
        status = self.scheduler.status()["jobs"]["docs"]
        self.assertEqual((status["state"], status["done"], status["total"]), ("done", 3, 3))

    def test_does_not_start_while_user_active(self):
        self.probe.signals = BUSY
        job = ListJob()
        self.scheduler.register(job, due=True)
        time.sleep(0.15)
        self.assertEqual(job.runs, 0)
        self.assertEqual(self.scheduler.status()["jobs"]["docs"]["state"], "waiting")
        self.probe.signals = QUIET
        self.assertTrue(self.scheduler.wait_idle_queue())
        self.assertEqual(job.runs, 1)

    def test_user_input_yields_after_current_document_and_resumes(self):
        gate_flip = threading.Event()

        def on_item(index, ctx):
            if index == 0:
                self.probe.signals = BUSY  # 첫 문서 처리 중에 사용자가 입력
                gate_flip.set()

        job = ListJob(items=3, on_item=on_item)
        self.scheduler.register(job, due=True)
        self.assertTrue(gate_flip.wait(2))
        self.assertTrue(wait_for(lambda: self.scheduler.status()["jobs"]["docs"]["state"] == "yielded"))
        self.assertEqual(job.processed, [0])  # 문서 하나 끝낸 뒤 멈춤
        self.probe.signals = QUIET
        self.assertTrue(self.scheduler.wait_idle_queue())
        self.assertEqual(job.processed, [0, 1, 2])

    def test_disabled_scheduler_waits_paused(self):
        job = ListJob()
        self.scheduler.register(job)
        self.scheduler.configure(enabled=False)
        self.scheduler.request("docs")
        time.sleep(0.15)
        self.assertEqual(job.runs, 0)
        self.assertEqual(self.scheduler.status()["state"], "paused")
        self.scheduler.configure(enabled=True)
        self.assertTrue(self.scheduler.wait_idle_queue())
        self.assertEqual(job.runs, 1)

    def test_recurring_job_runs_again(self):
        job = ListJob(items=1)
        self.scheduler.register(job, every=0.05)
        self.assertTrue(wait_for(lambda: job.runs >= 2))

    def test_failed_job_reports_error_and_scheduler_survives(self):
        class Broken(ListJob):
            def run(self, ctx):
                raise RuntimeError("boom")

        broken, good = Broken("broken"), ListJob("good")
        with self.assertLogs("docfit_app.idle_scheduler", level="ERROR"):
            self.scheduler.register(broken, due=True)
            self.scheduler.register(good, due=True)
            self.assertTrue(self.scheduler.wait_idle_queue())
        jobs = self.scheduler.status()["jobs"]
        self.assertEqual(jobs["broken"]["state"], "failed")
        self.assertIn("boom", jobs["broken"]["last_error"])
        self.assertEqual(jobs["good"]["state"], "done")

    def test_close_stops_running_job(self):
        self.probe.signals = QUIET
        started = threading.Event()

        def on_item(index, ctx):
            started.set()
            self.probe.signals = BUSY

        job = ListJob(items=5, on_item=on_item)
        self.scheduler.register(job, due=True)
        self.assertTrue(started.wait(2))
        self.scheduler.close()
        self.assertLess(len(job.processed), 5)

    def test_register_rejects_non_job(self):
        with self.assertRaises(TypeError):
            self.scheduler.register(object())


if __name__ == "__main__":
    unittest.main()
