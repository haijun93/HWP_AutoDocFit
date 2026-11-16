"""Parallel COM workers for klib ``scan-hwp`` JSON-lines output.

Each worker owns one independent Hancom COM object in its own thread.  This
module deliberately does not create a visible UI or start Hancom at import
time; applications supply ``app_factory`` (typically DispatchEx).
"""
from __future__ import annotations

import json
import os
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable

from docfit_core import hwp_bulk


@dataclass
class ParallelState:
    total: int = 0
    done: int = 0
    succeeded: int = 0
    failed: int = 0
    skipped: int = 0
    rate_per_minute: float = 0.0
    remaining_seconds: float | None = None
    current: str = ""
    results: list[hwp_bulk.BulkResult] = field(default_factory=list)


def worker_limit(requested: int | None = None, *, cpu_count: int | None = None,
                 available_memory_mb: int | None = None, memory_per_worker_mb: int = 768) -> int:
    """Bound COM concurrency by requested count, CPU count and available RAM."""
    cpus = max(1, cpu_count if cpu_count is not None else (os.cpu_count() or 1))
    if available_memory_mb is None:
        try:
            import psutil
            available_memory_mb = int(psutil.virtual_memory().available // (1024 * 1024))
        except Exception:
            available_memory_mb = memory_per_worker_mb * 2
    ram_cap = max(1, available_memory_mb // max(1, memory_per_worker_mb))
    return max(1, min(requested or 2, cpus, ram_cap))


def _scan_record(row: dict) -> tuple[str | None, bool]:
    """Extract path and accept only normal HWP5 records from scan-hwp JSON."""
    path = row.get("path") or row.get("full_path") or row.get("source")
    if not isinstance(path, str) or not path:
        return None, False
    # klib versions expose flags either as a category/type or HWP FileHeader bits.
    # scan-hwp uses kind="existing" when a sibling HWPX already exists, while
    # file_kind retains the underlying classification (for example "normal").
    category = str(row.get("file_kind") if row.get("kind") == "existing" else
                    row.get("kind", row.get("type", row.get("classification", "")))).casefold()
    if row.get("hwpx_exists") is True:
        return path, False
    if any(term in category for term in ("password", "encrypted", "암호", "drm", "distributed", "배포", "hwp3", "hwp 3")):
        return path, False
    if row.get("is_hwp5") is False or row.get("normal") is False:
        return path, False
    flags = row.get("flags", row.get("file_header_flags"))
    if isinstance(flags, dict):
        if flags.get("encrypted") or flags.get("distributed") or flags.get("drm"):
            return path, False
    elif isinstance(flags, int) and flags & (0x02 | 0x04 | 0x10):
        return path, False
    version = row.get("version")
    if isinstance(version, str) and version and not version.split(".", 1)[0].isdigit():
        return path, False
    if isinstance(version, str) and version and int(version.split(".", 1)[0]) < 5:
        return path, False
    # The scanner's own "normal" classification is authoritative. Do not let
    # unknown/error rows pass just because their optional flags are absent.
    if row.get("kind") is not None and str(row["kind"]).casefold() not in {"normal", "existing"}:
        return path, False
    if row.get("kind") == "existing" and str(row.get("file_kind", "")).casefold() != "normal":
        return path, False
    return path, True


def read_scan_jsonl(lines: Iterable[str]) -> list[Path]:
    """Parse scan-hwp JSONL, retaining only normal HWP5 paths that exist."""
    paths, seen = [], set()
    for line in lines:
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except (json.JSONDecodeError, TypeError):
            # A damaged line should not discard the rest of a large scan stream.
            continue
        if not isinstance(row, dict):
            continue
        raw, normal = _scan_record(row)
        if not raw or not normal:
            continue
        path = Path(raw)
        if path.suffix.casefold() != ".hwp" or not path.is_file() or path in seen:
            continue
        # Guard against incomplete/incorrect scanner metadata; require a HWP5 header.
        info = hwp_bulk.read_hwp5_header(path)
        if info is None or info.version >> 24 < 5 or info.is_encrypted or info.is_distributed or info.is_drm:
            continue
        seen.add(path)
        paths.append(path)
    return paths


def _convert_worker(index: int, files: list[Path], app_factory: Callable, restart_every: int,
                    stop: threading.Event, passwords=(), on_result=None):
    results = []
    app = None
    responder = None
    processed = 0
    try:
        for source in files:
            if stop.is_set():
                break
            if app is None:
                app = app_factory()
                responder = hwp_bulk.DialogResponder(None, passwords)
            result = hwp_bulk.convert_one(app, source, responder)
            results.append(result)
            if on_result:
                on_result(result)
            processed += 1
            if stop.is_set():
                break
            if restart_every > 0 and processed % restart_every == 0:
                try:
                    app.Quit()
                except Exception:
                    pass
                app, responder = None, None
    finally:
        if app is not None:
            try:
                app.Quit()
            except Exception:
                pass
        # COM apartments belong to their worker thread; release each one there.
        try:
            import pythoncom
            pythoncom.CoUninitialize()
        except Exception:
            pass
    return index, results


def run_parallel(scan_jsonl: Iterable[str] | str | os.PathLike, *, app_factory: Callable,
                 workers: int | None = 2, restart_every: int = 100,
                 stop: threading.Event | None = None, passwords=(),
                 on_progress: Callable[[dict], None] | None = None,
                 state: ParallelState | None = None, memory_per_worker_mb: int = 768) -> list[hwp_bulk.BulkResult]:
    """Convert normal HWP5 scan results using bounded, independent COM workers.

    ``scan_jsonl`` can be a path or an iterable of JSON lines. Existing outputs
    are reported as skipped. Only failed records are retried on restart: pass
    their source paths as a new JSONL/input batch; successful HWPX files are
    naturally protected by ``convert_one``'s no-overwrite guard.
    """
    stop = stop or threading.Event()
    state = state or ParallelState()
    if isinstance(scan_jsonl, (str, os.PathLike)):
        with open(scan_jsonl, encoding="utf-8") as stream:
            files = read_scan_jsonl(stream)
    else:
        files = read_scan_jsonl(scan_jsonl)
    state.total = len(files)
    state.done = state.succeeded = state.failed = state.skipped = 0
    state.results = []
    if not files:
        if on_progress:
            on_progress(_progress_payload(state, time.monotonic(), ""))
        return []
    count = worker_limit(workers, memory_per_worker_mb=memory_per_worker_mb)
    buckets = [files[i::count] for i in range(count)]
    started = time.monotonic()
    futures = []
    result_lock = threading.Lock()

    def record_result(result):
        with result_lock:
            state.results.append(result)
            state.done += 1
            state.current = result.source
            if result.status == "success":
                state.succeeded += 1
            elif result.status == "failed":
                state.failed += 1
            else:
                state.skipped += 1
            elapsed = max(0.001, time.monotonic() - started)
            state.rate_per_minute = state.done * 60.0 / elapsed
            state.remaining_seconds = ((state.total - state.done) / (state.done / elapsed)
                                       if state.done else None)
            payload = _progress_payload(state, started, state.current)
        if on_progress:
            on_progress(payload)

    with ThreadPoolExecutor(max_workers=count, thread_name_prefix="hwp-com") as pool:
        futures = [pool.submit(_convert_worker, i, bucket, app_factory, restart_every, stop, passwords,
                               record_result)
                   for i, bucket in enumerate(buckets) if bucket]
        for future in as_completed(futures):
            future.result()
    state.results.sort(key=lambda item: item.source.casefold())
    state.current = ""
    return state.results


def _progress_payload(state: ParallelState, started: float, current: str) -> dict:
    return {"total": state.total, "done": state.done, "succeeded": state.succeeded,
            "failed": state.failed, "skipped": state.skipped, "current": current,
            "rate_per_minute": round(state.rate_per_minute, 2),
            "remaining_seconds": (round(state.remaining_seconds, 1)
                                  if state.remaining_seconds is not None else None),
            "elapsed_seconds": round(time.monotonic() - started, 1)}

