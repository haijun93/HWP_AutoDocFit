"""Fake-COM coverage for parallel HWP5 conversion; never starts Hancom Office."""
from __future__ import annotations

import json
import struct
import tempfile
import threading
import time
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from docfit_core import hwp_bulk, hwp_bulk_parallel


def _hwp5(flags: int = 1) -> bytes:
    header = bytearray(256)
    header[:17] = b"HWP Document File"
    struct.pack_into("<I", header, 32, 0x05010001)
    struct.pack_into("<I", header, 36, flags)
    data = bytearray(hwp_bulk.OLE_SIGNATURE)
    data.extend(b"\0" * (512 - len(data)))
    data.extend(header)
    data.extend(b"\0" * (1024 - len(data)))
    return bytes(data)


def _write_hwpx(path: str | Path, valid: bool = True) -> None:
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("mimetype", "application/hwp+zip")
        archive.writestr("Contents/header.xml", "<head/>" if valid else "<head>")
        archive.writestr("Contents/section0.xml", "<sec/>")


class FakeHwp:
    """Small COM-shaped fake. It records thread ownership and never launches Hwp.exe."""

    def __init__(self, *, invalid_output: bool = False, fail_names=()):
        self.invalid_output = invalid_output
        self.fail_names = set(fail_names)
        self.calls = []
        self.quit = False

    def Open(self, path, fmt, arg):
        self.calls.append(("Open", Path(path).name, threading.get_ident()))
        return Path(path).name not in self.fail_names

    def SaveAs(self, path, fmt, arg):
        self.calls.append(("SaveAs", Path(path).name, threading.get_ident()))
        _write_hwpx(path, valid=not self.invalid_output)
        return True

    def Clear(self, option):
        self.calls.append(("Clear", option, threading.get_ident()))

    def Quit(self):
        self.quit = True


class HwpBulkParallelTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def _source(self, name: str, flags: int = 1) -> Path:
        path = self.root / name
        path.write_bytes(_hwp5(flags))
        return path

    def _row(self, path: Path, **extra) -> str:
        row = {"path": str(path), "kind": "normal", "label": "정상 HWP5",
               "size": path.stat().st_size, "hwpx_exists": False,
               "version": "5.1.0.1", "flags": 1}
        row.update(extra)
        return json.dumps(row, ensure_ascii=False)

    def test_scan_jsonl_accepts_only_normal_hwp5_and_existing_rows_are_skipped(self):
        normal = self._source("normal.hwp")
        encrypted = self._source("encrypted.hwp", flags=3)
        existing = self._source("existing.hwp")
        rows = [
            self._row(normal),
            self._row(encrypted, kind="password", flags=3),
            self._row(existing, kind="existing", file_kind="normal", hwpx_exists=True),
            "{malformed",
        ]
        self.assertEqual(hwp_bulk_parallel.read_scan_jsonl(rows), [normal])
        # Existing scanner records without hwpx_exists also retain the base type.
        self.assertEqual(hwp_bulk_parallel._scan_record(
            {"path": str(existing), "kind": "existing", "file_kind": "normal"}),
            (str(existing), True))

    def test_worker_distribution_uses_independent_com_objects_and_respects_limit(self):
        sources = [self._source(f"{i}.hwp") for i in range(6)]
        created = []

        def factory():
            app = FakeHwp()
            created.append(app)
            return app

        state = hwp_bulk_parallel.ParallelState()
        with mock.patch.object(hwp_bulk_parallel, "worker_limit", return_value=3):
            results = hwp_bulk_parallel.run_parallel(
                [self._row(path) for path in sources], app_factory=factory,
                workers=8, restart_every=100, state=state)
        self.assertEqual(len(created), 3)
        self.assertEqual([item.status for item in results], ["success"] * 6)
        self.assertTrue(all(app.quit for app in created))
        self.assertEqual(state.done, state.succeeded)
        self.assertEqual(len({id(app) for app in created}), 3)
        self.assertTrue(all(len([c for c in app.calls if c[0] == "Open"]) == 2 for app in created))

    def test_stop_leaves_unprocessed_sources_for_resume(self):
        sources = [self._source(f"stop{i}.hwp") for i in range(4)]
        stop = threading.Event()
        app = FakeHwp()

        def stop_after_first(progress):
            if progress["done"] == 1:
                stop.set()

        state = hwp_bulk_parallel.ParallelState()
        with mock.patch.object(hwp_bulk_parallel, "worker_limit", return_value=1):
            first = hwp_bulk_parallel.run_parallel(
                [self._row(path) for path in sources], app_factory=lambda: app,
                workers=1, stop=stop, on_progress=stop_after_first, state=state)
        self.assertEqual(len(first), 1)
        self.assertEqual(state.done, 1)
        # Resume with the original input. The posted result is skipped without overwrite;
        # remaining HWP files are processed.
        resume_app = FakeHwp()
        with mock.patch.object(hwp_bulk_parallel, "worker_limit", return_value=1):
            resumed = hwp_bulk_parallel.run_parallel(
                [self._row(path) for path in sources], app_factory=lambda: resume_app,
                workers=1)
        self.assertEqual(len(resumed), 4)
        self.assertEqual(sum(result.status == "success" for result in resumed), 3)
        self.assertEqual(sum(result.status == "skipped" for result in resumed), 1)

    def test_invalid_generated_hwpx_is_removed_and_source_is_preserved(self):
        source = self._source("invalid-result.hwp")
        original = source.read_bytes()
        result = hwp_bulk_parallel.run_parallel(
            [self._row(source)], app_factory=lambda: FakeHwp(invalid_output=True), workers=1)[0]
        self.assertEqual(result.status, "failed")
        self.assertEqual(source.read_bytes(), original)
        self.assertFalse(source.with_suffix(".hwpx").exists())
        self.assertEqual(list(self.root.glob(".~docfit_*")), [])

    def test_existing_hwpx_is_never_overwritten(self):
        source = self._source("existing-output.hwp")
        target = source.with_suffix(".hwpx")
        target.write_bytes(b"preexisting-output")
        app = FakeHwp()
        result = hwp_bulk_parallel.run_parallel(
            [self._row(source)], app_factory=lambda: app, workers=1)[0]
        self.assertEqual(result.status, "skipped")
        self.assertEqual(target.read_bytes(), b"preexisting-output")
        self.assertEqual(app.calls, [])

    def test_progress_callback_reports_rate_and_remaining_time(self):
        sources = [self._source(f"progress{i}.hwp") for i in range(3)]
        progress = []
        with mock.patch.object(hwp_bulk_parallel, "worker_limit", return_value=1):
            hwp_bulk_parallel.run_parallel(
                [self._row(path) for path in sources], app_factory=FakeHwp,
                workers=1, on_progress=progress.append)
        self.assertEqual(len(progress), 3)
        self.assertEqual(progress[-1]["done"], 3)
        self.assertGreater(progress[-1]["rate_per_minute"], 0)
        self.assertIsNotNone(progress[-1]["remaining_seconds"])

    def test_periodic_com_restart_closes_and_recreates_worker_instance(self):
        sources = [self._source(f"restart{i}.hwp") for i in range(3)]
        apps = []

        def factory():
            app = FakeHwp()
            apps.append(app)
            return app

        with mock.patch.object(hwp_bulk_parallel, "worker_limit", return_value=1):
            results = hwp_bulk_parallel.run_parallel(
                [self._row(path) for path in sources], app_factory=factory,
                workers=1, restart_every=1)
        self.assertEqual(len(results), 3)
        self.assertEqual(len(apps), 3)
        self.assertTrue(all(app.quit for app in apps))


if __name__ == "__main__":
    unittest.main()
