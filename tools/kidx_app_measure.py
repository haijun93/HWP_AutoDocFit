#!/usr/bin/env python3
"""Measure breakdown of TimemachineService content indexing application path."""
from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from tools.kidx_bench import make_synthetic
from docfit_app.timemachine_service import TimemachineService
from docfit_core.timemachine.rules import Rules, excluded_name


class ConnectionProxy:
    def __init__(self, target):
        self._target = target
        self.write_time = 0.0

    def __getattr__(self, name):
        return getattr(self._target, name)

    def execute(self, *args, **kwargs):
        t0 = time.perf_counter()
        try:
            return self._target.execute(*args, **kwargs)
        finally:
            self.write_time += (time.perf_counter() - t0)

    def executemany(self, *args, **kwargs):
        t0 = time.perf_counter()
        try:
            return self._target.executemany(*args, **kwargs)
        finally:
            self.write_time += (time.perf_counter() - t0)

    def __enter__(self):
        return self._target.__enter__()

    def __exit__(self, exc_type, exc_val, exc_tb):
        return self._target.__exit__(exc_type, exc_val, exc_tb)


def measure_app_path(fixtures: Path, count: int = 300) -> dict:
    with tempfile.TemporaryDirectory(prefix="kidx11-measure-") as temp_dir:
        temp_path = Path(temp_dir)
        root = make_synthetic(fixtures, count, temp_path / "documents")
        db_path = temp_path / "app_path.db"

        service = TimemachineService(temp_path / "settings", db_path=db_path, agent=False)
        try:
            service._settings = {
                "roots": [str(root)],
                "time_mode": "max",
                "enabled": False,
                "extensions": ["hwpx", "txt", "md"],
                "rust_content_index": True,
            }

            roots = list(service._settings["roots"])
            extensions = frozenset(service._settings["extensions"])
            cancel_event = threading.Event()

            # 1. 파일 목록 (File listing)
            t0 = time.perf_counter()
            raw_walk = []
            for r in roots:
                for folder, dirs, files in os.walk(r, followlinks=False):
                    for name in files:
                        raw_walk.append((folder, name))
            t_file_list = time.perf_counter() - t0

            # 2. 분배 (Distribution)
            t0 = time.perf_counter()
            rules = Rules(extensions=extensions, extra_extensions=extensions)
            rust_paths, python_paths = [], []
            for folder, name in raw_walk:
                ext = Path(name).suffix.lower().lstrip(".")
                if excluded_name(name) or ext not in extensions:
                    continue
                path = os.path.join(folder, name)
                if not service._path_in_active_root(path):
                    continue
                if rules.skip_file(name):
                    continue
                if ext in {"txt", "md", "hwpx"}:
                    rust_paths.append(path)
                else:
                    python_paths.append(path)
            t_distribute = time.perf_counter() - t0

            # 3. klib 실행 & 4. 진행률 변환 (klib execution & Progress conversion)
            executable = service._klib_executable()
            t_klib_start = time.perf_counter()
            process = subprocess.Popen(
                [str(executable), "index-content", service.content_index.path, *map(str, rust_paths)],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
            )
            with service._content_lock:
                service._content_process = process

            lines = queue.Queue()
            def read_output():
                for line in process.stdout:
                    lines.put(line)
                lines.put(None)
            threading.Thread(target=read_output, name="klib-progress-reader", daemon=True).start()

            t_progress = 0.0
            outcomes = {}
            seen = 0
            indexed = 0
            errors = 0
            while True:
                try:
                    line = lines.get(timeout=0.05)
                except queue.Empty:
                    continue
                if line is None:
                    break
                tp0 = time.perf_counter()
                try:
                    event = json.loads(line)
                    p = os.path.normcase(os.path.abspath(event.get("path", "")))
                    status = event.get("status")
                    if p:
                        outcomes[p] = status
                    if status in ("indexed", "skipped"):
                        seen += 1
                        indexed += (status == "indexed")
                        with service._content_lock:
                            service._content_state.update(seen=seen, indexed=indexed, errors=errors)
                except Exception:
                    pass
                t_progress += (time.perf_counter() - tp0)

            returncode = process.wait()
            if process.stdout:
                process.stdout.close()
            t_klib_total = time.perf_counter() - t_klib_start
            t_klib_pure = max(0.0, t_klib_total - t_progress)

            # 대체 목록 계산 (Failed calculation)
            t0 = time.perf_counter()
            failed = [path for path in rust_paths if returncode != 0 or outcomes.get(os.path.normcase(os.path.abspath(str(path)))) in (None, "error")]
            python_paths.extend(failed)
            t_failed_calc = time.perf_counter() - t0

            # 5. DB 쓰기 & 6. Python 대체 처리
            proxy = ConnectionProxy(service.content_index.db)
            service.content_index.db = proxy

            t_fallback_start = time.perf_counter()
            for path in python_paths:
                res = service.content_index.index_file(path)
                seen += 1
                indexed += bool(res.get("indexed"))
                errors += bool(res.get("error"))
                with service._content_lock:
                    service._content_state.update(seen=seen, indexed=indexed, errors=errors)
            t_fallback_total = time.perf_counter() - t_fallback_start

            service.content_index.db = proxy._target
            db_write_time = proxy.write_time
            t_python_extract_only = max(0.0, t_fallback_total - db_write_time)

            # Database rows verification
            row_docs = service.content_index.db.execute("SELECT count(*) FROM content_meta").fetchone()[0]
            row_paras = service.content_index.db.execute("SELECT count(*) FROM content_paragraph_fts").fetchone()[0]
            row_errors = service.content_index.db.execute("SELECT count(*) FROM content_errors").fetchone()[0]

            total_elapsed = t_file_list + t_distribute + t_klib_total + t_failed_calc + t_fallback_total

            return {
                "total_documents": count,
                "klib_returncode": returncode,
                "klib_candidates": len(rust_paths),
                "python_fallback_count": len(python_paths),
                "db_documents": row_docs,
                "db_paragraphs": row_paras,
                "db_errors": row_errors,
                "timings_sec": {
                    "file_list": round(t_file_list, 4),
                    "distribute": round(t_distribute, 4),
                    "klib_exec": round(t_klib_pure, 4),
                    "progress_conversion": round(t_progress, 4),
                    "failed_calc": round(t_failed_calc, 4),
                    "db_write": round(db_write_time, 4),
                    "python_fallback": round(t_fallback_total, 4),
                    "python_extract_only": round(t_python_extract_only, 4),
                    "total": round(total_elapsed, 4),
                },
            }
        finally:
            service.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, default=Path("tests/fixtures"), help="fixtures directory")
    parser.add_argument("--synth", type=int, default=300, help="synthetic document count")
    args = parser.parse_args()

    data = measure_app_path(args.fixtures, args.synth)
    timings = data["timings_sec"]
    print("## 앱 경로 300문서 1회 구간별 시간 측정")
    print()
    print("| 구간 | 소요 시간(초) | 비중(%) | 비고 |")
    print("|---|---:|---:|---|")
    total = timings["total"]
    def pct(v): return f"{(v / total * 100):.1f}%" if total > 0 else "0.0%"
    print(f"| 파일 목록 (os.walk) | {timings['file_list']:.4f} | {pct(timings['file_list'])} | 300문서 디렉터리 순회 |")
    print(f"| 분배 (확장자/규칙 필터) | {timings['distribute']:.4f} | {pct(timings['distribute'])} | 지원(hwpx,txt,md)/비지원 분배 |")
    print(f"| klib 실행 (subprocess) | {timings['klib_exec']:.4f} | {pct(timings['klib_exec'])} | returncode={data['klib_returncode']} |")
    print(f"| 진행률 변환 (JSON 파싱·락) | {timings['progress_conversion']:.4f} | {pct(timings['progress_conversion'])} | 표준출력 큐 및 상태 갱신 |")
    print(f"| DB 쓰기 (SQLite) | {timings['db_write']:.4f} | {pct(timings['db_write'])} | Python 대체 중 DB 쿼리 실행 시간 |")
    print(f"| Python 대체 처리 (index_file) | {timings['python_fallback']:.4f} | {pct(timings['python_fallback'])} | {data['python_fallback_count']}개 문서 재처리(본문 추출 {timings['python_extract_only']:.4f}초 + DB 쓰기 {timings['db_write']:.4f}초) |")
    print(f"| **전체 합계** | **{total:.4f}** | **100.0%** | 문서 {data['db_documents']}개, 문단 {data['db_paragraphs']}개, 오류 {data['db_errors']}개 |")
    print()
    print("JSON:")
    print(json.dumps(data, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
