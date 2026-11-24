#!/usr/bin/env python3
"""One-shot synthetic SQLite search baseline. All generated corpus data stays temporary."""
from __future__ import annotations

import argparse
import json
import os
import random
import statistics
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine.search_backend import RustBackend, SqliteBackend


WORDS = ("행정", "정책", "문서", "검색", "데이터", "민원", "안전", "환경", "지원", "절차",
         "성과", "운영", "계획", "점검", "지역", "서비스", "정보", "협력", "기준", "개선")
MARKERS = ("1.", "(1)", "가.", "□", "○", "-", "※")
PHRASES = ("행정 절차 지원", "지역 정책 운영", "안전 점검 계획", "환경 데이터 분석")


def rss_bytes():
    """Return current working set and process peak working set where available."""
    try:
        import psutil  # type: ignore
        process = psutil.Process()
        processes = [process, *process.children(recursive=True)]
        rss = sum(item.memory_info().rss for item in processes if item.is_running())
        peak = max((getattr(item.memory_info(), "peak_wset", item.memory_info().rss)
                    for item in processes if item.is_running()), default=rss)
        return rss, peak
    except ImportError:
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes

            class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
                _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD),
                            ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
                            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                            ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t)]
            counters = PROCESS_MEMORY_COUNTERS()
            counters.cb = ctypes.sizeof(counters)
            handle = ctypes.windll.kernel32.GetCurrentProcess()
            if ctypes.windll.psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
                return counters.WorkingSetSize, counters.PeakWorkingSetSize
    return None, None


class PeakSampler:
    def __init__(self):
        self.peak = 0
        self.current = 0
        self.stop_event = threading.Event()
        self.thread = threading.Thread(target=self._sample, daemon=True)

    def _sample(self):
        while not self.stop_event.wait(0.2):
            current, peak = rss_bytes()
            self.current = current or self.current
            self.peak = max(self.peak, current or 0, peak or 0)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop_event.set()
        self.thread.join()
        current, peak = rss_bytes()
        self.current = current or self.current
        self.peak = max(self.peak, current or 0, peak or 0)


def make_paragraphs(rng: random.Random, doc_number: int) -> str:
    count = rng.randint(20, 50)
    lines = [f"제{doc_number % 12 + 1}장 {rng.choice(WORDS)} 추진 계획"]
    for paragraph in range(count - 1):
        marker = MARKERS[paragraph % len(MARKERS)]
        words = rng.sample(WORDS, k=6)
        phrase = PHRASES[(doc_number + paragraph) % len(PHRASES)]
        lines.append(f"{marker} {phrase}와 {' '.join(words)}를 바탕으로 추진 현황을 확인하고 개선 방안을 마련한다.")
    return "\n".join(lines) + "\n"


def make_corpus(folder: Path, count: int, seed: int, *, partition_size: int | None = None) -> list[Path]:
    rng = random.Random(seed)
    paths = []
    for number in range(count):
        destination = folder
        if partition_size:
            destination = folder / f"part_{number // partition_size:03d}"
            destination.mkdir(exist_ok=True)
        path = destination / f"synthetic_{number:06d}.txt"
        path.write_text(make_paragraphs(rng, number), encoding="utf-8")
        paths.append(path)
    return paths


def timed_search(backend, query):
    started = time.perf_counter()
    rows = backend.search(query, limit=20)
    return (time.perf_counter() - started) * 1000, len(rows)


def database_bytes(database: Path) -> int:
    return sum(path.stat().st_size for path in database.parent.glob(database.name + "*") if path.is_file())


def run_size(count: int, seed: int, deadline: float, backend_mode="sqlite", klib_exe=None) -> dict:
    result = {"documents": count, "seed": seed}
    with tempfile.TemporaryDirectory(prefix=f"srch2-{count}-") as temp:
        folder = Path(temp) / "corpus"
        folder.mkdir()
        database = Path(temp) / "search.sqlite"
        rust_index = Path(temp) / "tantivy-index"
        generation_started = time.perf_counter()
        paths = make_corpus(folder, count, seed)
        result["corpus_generation_seconds"] = round(time.perf_counter() - generation_started, 3)
        index = ContentSearchIndex(database) if backend_mode == "sqlite" else None
        backend = SqliteBackend(index) if index is not None else RustBackend(
            executable=klib_exe, index_dir=rust_index)
        with PeakSampler() as peak:
            started = time.perf_counter()
            if backend_mode == "rust":
                backend.index_files(paths, batch_size=250)
            else:
                for i, path in enumerate(paths, 1):
                    backend.index_file(str(path))
                    if time.monotonic() >= deadline:
                        result["initial_index_seconds"] = round(time.perf_counter() - started, 3)
                        result["documents_indexed_before_limit"] = i
                        result["status"] = "stopped_at_time_limit"
                        index.close()
                        result["max_rss_bytes"] = peak.peak or None
                        return result
            result["initial_index_seconds"] = round(time.perf_counter() - started, 3)
            result["documents_indexed_before_limit"] = len(paths)

            query_cases = {"short": "행정", "long": "행정 정책 문서 안전 지원", "and": "행정 정책", "phrase": '"행정 절차 지원"'}
            result["search"] = {}
            for label, query in query_cases.items():
                if time.monotonic() >= deadline:
                    result["search"][label] = {"status": "not_measured_time_limit"}
                    continue
                cold_ms, hits = timed_search(backend, query)
                warm = [timed_search(backend, query)[0] for _ in range(5)] if time.monotonic() < deadline else []
                result["search"][label] = {"query": query, "cold_ms": round(cold_ms, 3),
                    "warm_median_ms": round(statistics.median(warm), 3) if warm else None,
                    "warm_samples": len(warm), "returned_documents": hits}

            changed = paths[::100]
            for n, path in enumerate(changed):
                path.write_text(path.read_text(encoding="utf-8") + f"\n추가 변경 기록 {n} 검색 기준 행정 지원\n", encoding="utf-8")
            query_lags = []
            index_failures = []
            stop_queries = threading.Event()
            first_query_done = threading.Event()

            def query_while_indexing():
                while not stop_queries.is_set():
                    if time.monotonic() >= deadline:
                        break
                    try:
                        elapsed, _ = timed_search(backend, "행정 정책")
                        query_lags.append(elapsed)
                        first_query_done.set()
                    except Exception as exc:  # recorded as a measurement failure, never hidden
                        index_failures.append(type(exc).__name__ + ": " + str(exc))
                        break

            query_thread = threading.Thread(target=query_while_indexing, daemon=True)
            query_thread.start()
            first_query_done.wait(timeout=5)
            incremental_started = time.perf_counter()
            for path in changed:
                if time.monotonic() >= deadline:
                    break
                backend.index_file(str(path))
            stop_queries.set()
            query_thread.join()
            result["incremental_changed_documents"] = len(changed)
            result["incremental_index_seconds"] = round(time.perf_counter() - incremental_started, 3)
            result["search_while_indexing"] = {
                "samples": len(query_lags),
                "median_ms": round(statistics.median(query_lags), 3) if query_lags else None,
                "max_ms": round(max(query_lags), 3) if query_lags else None,
                "errors": index_failures,
            }
            result["max_rss_bytes"] = peak.peak or None
        if index is not None: index.close()
        if hasattr(backend, "close"): backend.close()
        result["index_disk_bytes"] = (database_bytes(database) if backend_mode == "sqlite" else
            sum(path.stat().st_size for path in rust_index.rglob("*") if path.is_file()))
        result["status"] = "complete" if time.monotonic() < deadline else "time_limit_reached"
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", type=int, nargs="+", default=[1000, 10000])
    parser.add_argument("--seed", type=int, default=20261124)
    parser.add_argument("--time-limit-minutes", type=float, default=28.0)
    parser.add_argument("--backend", choices=("sqlite", "rust"), default="sqlite")
    parser.add_argument("--klib-exe", help="Rust klib executable; defaults to KLIB_EXE/PATH")
    args = parser.parse_args()
    deadline = time.monotonic() + args.time_limit_minutes * 60
    output = {"backend": "SQLite / ContentSearchIndex" if args.backend == "sqlite" else "Rust / klib Tantivy",
              "python": sys.version,
              "seed": args.seed, "time_limit_minutes": args.time_limit_minutes, "runs": []}
    previous_rate = None
    for count in args.sizes:
        if time.monotonic() >= deadline:
            output["runs"].append({"documents": count, "status": "not_measured_time_limit"})
            continue
        if count >= 10000 and previous_rate is not None:
            # Predict from the completed 1k run; skip 10k if its estimate exceeds remaining time.
            estimate = previous_rate * (count / 1000)
            remaining = deadline - time.monotonic()
            if estimate > remaining:
                output["runs"].append({"documents": count, "status": "skipped_predicted_over_time_limit",
                                       "estimated_seconds": round(estimate, 1), "remaining_seconds": round(remaining, 1)})
                continue
        run = run_size(count, args.seed, deadline, args.backend, args.klib_exe)
        output["runs"].append(run)
        if count == 1000 and run.get("initial_index_seconds"):
            previous_rate = max(run["initial_index_seconds"], 1.0) + run.get("corpus_generation_seconds", 0)
    print(json.dumps(output, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
