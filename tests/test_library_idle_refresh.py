import os
import tempfile
import unittest
from pathlib import Path

from docfit_app.idle_gate import IdleGate, IdleSignals
from docfit_app.idle_scheduler import IdleContext, IdleJob
from docfit_app.library_bridge import LibraryBridgeError
from docfit_app.library_idle_refresh import LibraryItemRefreshJob

QUIET = IdleSignals(input_idle_seconds=600, cpu_percent=0)


class Probe:
    def __init__(self, signals):
        self.signals = signals

    def sample(self):
        return self.signals


class FakeBridge:
    def __init__(self, items):
        self.items = items
        self.indexed = []
        self.fail_paths = set()

    def request(self, request):
        if request["cmd"] == "item_search":
            return {"ok": True, "items": list(self.items)}
        if request["cmd"] == "index_file":
            if request["path"] in self.fail_paths:
                raise LibraryBridgeError("끊김")
            self.indexed.append((request["path"], request["box_id"]))
            return {"ok": True}
        raise AssertionError(request)


class LibraryItemRefreshJobTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.state = self.base / "상태" / "library-idle.json"
        self.ctx = IdleContext(IdleGate(Probe(QUIET), sample_interval=0), "library-items", poll_seconds=0.01)

    def tearDown(self):
        self.temp.cleanup()

    def file(self, name, text="본문"):
        path = self.base / name
        path.write_text(text, encoding="utf-8")
        return str(path)

    def touch(self, path, text):
        Path(path).write_text(text, encoding="utf-8")
        stat = os.stat(path)
        os.utime(path, ns=(stat.st_atime_ns, stat.st_mtime_ns + 5_000_000_000))

    def item(self, path, box_id=1, kind="file"):
        return {"kind": kind, "source_path": path, "box_id": box_id, "display_name": Path(path).name}

    def test_first_run_is_baseline_then_only_changed_files_reindex(self):
        a, b = self.file("가.txt"), self.file("나.md")
        bridge = FakeBridge([self.item(a), self.item(b, box_id=2), {"kind": "link", "source_path": "https://x"}])
        job = LibraryItemRefreshJob(lambda: bridge, self.state)
        self.assertIsInstance(job, IdleJob)
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [])
        self.assertTrue(job.last_result["baseline"])
        self.touch(b, "바뀐 본문")
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [(b, 2)])
        self.assertEqual((self.ctx.done, self.ctx.total), (1, 1))
        job.run(self.ctx)  # 기준이 옮겨졌으므로 다시 하지 않음
        self.assertEqual(len(bridge.indexed), 1)

    def test_new_item_after_baseline_is_not_reindexed(self):
        a = self.file("가.txt")
        bridge = FakeBridge([self.item(a)])
        job = LibraryItemRefreshJob(lambda: bridge, self.state)
        job.run(self.ctx)
        bridge.items.append(self.item(self.file("새.txt")))
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [])

    def test_failed_file_is_retried_next_time(self):
        a = self.file("가.txt")
        bridge = FakeBridge([self.item(a)])
        job = LibraryItemRefreshJob(lambda: bridge, self.state)
        job.run(self.ctx)
        self.touch(a, "바뀜")
        bridge.fail_paths.add(a)
        job.run(self.ctx)
        self.assertEqual(job.last_result["errors"], 1)
        bridge.fail_paths.clear()
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [(a, 1)])

    def test_multi_box_file_is_skipped_to_keep_memberships(self):
        a = self.file("가.txt")
        bridge = FakeBridge([self.item(a, 1), self.item(a, 2)])
        job = LibraryItemRefreshJob(lambda: bridge, self.state)
        job.run(self.ctx)
        self.touch(a, "바뀜")
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [])
        self.assertEqual(job.last_result["skipped_multi_box"], 1)

    def test_stopped_context_leaves_change_for_next_time(self):
        a = self.file("가.txt")
        bridge = FakeBridge([self.item(a)])
        job = LibraryItemRefreshJob(lambda: bridge, self.state)
        job.run(self.ctx)
        self.touch(a, "바뀜")
        stopped = IdleContext(IdleGate(Probe(QUIET), sample_interval=0), "library-items")
        stopped.stop()
        job.run(stopped)
        self.assertEqual(bridge.indexed, [])
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [(a, 1)])

    def test_no_bridge_means_no_work(self):
        job = LibraryItemRefreshJob(lambda: None, self.state)
        self.assertFalse(job.pending())
        job.run(self.ctx)
        self.assertFalse(self.state.exists())

    def test_missing_source_is_ignored(self):
        bridge = FakeBridge([self.item(str(self.base / "없음.txt"))])
        job = LibraryItemRefreshJob(lambda: bridge, self.state)
        job.run(self.ctx)
        job.run(self.ctx)
        self.assertEqual(bridge.indexed, [])


if __name__ == "__main__":
    unittest.main()
