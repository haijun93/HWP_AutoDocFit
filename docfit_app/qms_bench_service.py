"""Benchmark service adapter for qms_bench CLI engine."""
from __future__ import annotations

import json
import ntpath
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
from typing import Any, Callable, Dict, List, Optional

from .api_contract import ServiceError


DEFAULT_QUERIES = [
    {"id": "q1", "kind": "korean_short", "text": "계약", "comparability": "comparable"},
    {"id": "q2", "kind": "korean_short", "text": "보고서", "comparability": "comparable"},
    {"id": "q3", "kind": "korean_medium", "text": "인사평가", "comparability": "comparable"},
    {"id": "q4", "kind": "english_numeric", "text": "report", "comparability": "comparable"},
    {"id": "q5", "kind": "english_numeric", "text": "2026", "comparability": "comparable"},
    {"id": "q6", "kind": "ext_filter", "text": "ext:pdf", "filter": "ext:pdf", "comparability": "comparable"},
    {"id": "q7", "kind": "multi_word", "text": "회의록 결과", "comparability": "comparable"},
    {"id": "q8", "kind": "rare_zero", "text": "xyznonexistent999", "comparability": "comparable"},
]


class QmsBenchService:
    def __init__(
        self,
        data_root: Path | str,
        executable: Optional[Path | str] = None,
        runner: Callable = subprocess.run,
        popen: Callable = subprocess.Popen,
        developer_mode_checker: Optional[Callable[[], bool]] = None,
        opener: Optional[Callable[[str], Any]] = None,
    ):
        self.data_root = Path(data_root).resolve()
        self.benchmarks_dir = (self.data_root / "benchmarks").resolve()
        self.benchmarks_dir.mkdir(parents=True, exist_ok=True)
        self.executable = Path(executable) if executable else self._find_executable()
        self.runner = runner
        self.popen = popen
        self.developer_mode_checker = developer_mode_checker or (lambda: True)
        self.opener = opener or getattr(os, "startfile", lambda p: None)

        self._lock = threading.RLock()
        self._current_process: Optional[subprocess.Popen] = None
        self._events: List[Dict[str, Any]] = []
        self._status: str = "idle"  # idle, running, completed, failed, cancelled
        self._run_id: Optional[str] = None
        self._current_result: Optional[Dict[str, Any]] = None
        self._error_message: Optional[str] = None

    @staticmethod
    def _find_executable() -> Path:
        name = "qms_bench.exe" if os.name == "nt" else "qms_bench"
        app_root = Path(__file__).resolve().parents[1]
        candidates = []
        for base in (
            Path(sys.executable).resolve().parent,
            Path(getattr(sys, "_MEIPASS", app_root)),
        ):
            candidates.extend(
                (
                    base / "_internal" / "resources" / name,
                    base / "resources" / name,
                )
            )
        candidates.extend(
            (
                app_root / "resources" / name,
                app_root / "qms_bench" / "target" / "release" / name,
                app_root / name,
            )
        )
        for candidate in candidates:
            if candidate.is_file():
                return candidate
        found = shutil.which("qms_bench")
        return Path(found) if found else candidates[0]

    def _verify_path_inside_benchmarks(self, path_str: Optional[str]) -> Path:
        if not path_str:
            return self.benchmarks_dir
        # Windows absolute paths stay absolute when received on Linux; Path.resolve()
        # would otherwise treat e.g. C:/Windows as a harmless relative child.
        if os.name != "nt" and ntpath.isabs(path_str) and not Path(path_str).is_absolute():
            raise ServiceError("INVALID_PATH", "사용자 데이터 폴더 밖의 경로는 사용할 수 없습니다.")
        candidate = Path(path_str)
        resolved = (
            candidate.resolve()
            if candidate.is_absolute()
            else (self.benchmarks_dir / candidate).resolve()
        )
        try:
            resolved.relative_to(self.benchmarks_dir)
        except ValueError:
            raise ServiceError("INVALID_PATH", "사용자 데이터 폴더 밖의 경로는 사용할 수 없습니다.")
        return resolved

    def bench_detect(self) -> Dict[str, Any]:
        if not self.executable.is_file():
            raise ServiceError("ENGINE_MISSING", "벤치마크 엔진(qms_bench)을 찾을 수 없습니다.")
        try:
            proc = self.runner(
                [str(self.executable), "detect"], creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=15,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as e:
            raise ServiceError("ENGINE_FAILED", f"탐지 명령 실행에 실패했습니다: {e}")

        if proc.returncode != 0:
            raise ServiceError(
                "ENGINE_FAILED",
                f"탐지 엔진이 오류를 반환했습니다 (종료 코드 {proc.returncode}): {proc.stderr.strip()}",
            )

        try:
            return json.loads(proc.stdout)
        except json.JSONDecodeError:
            raise ServiceError("ENGINE_FAILED", "탐지 엔진의 응답 JSON을 읽을 수 없습니다.")

    def _get_or_create_query_set(self, query_set_type: str, include_raw: bool) -> Path:
        qs_dir = self.benchmarks_dir / "query_sets"
        qs_dir.mkdir(parents=True, exist_ok=True)

        if query_set_type == "my_files":
            # 내 파일 표본 질의 세트 생성 시도
            target_file = qs_dir / "my_files_query_set.json"
            # 사용자 데이터 폴더 내 파일명 수집 (최대 500개)
            sample_names = []
            try:
                for p in self.data_root.rglob("*"):
                    if p.is_file() and not p.name.startswith("."):
                        sample_names.append(p.name)
                        if len(sample_names) >= 500:
                            break
            except Exception:
                sample_names = []

            if sample_names and self.executable.is_file():
                names_json = qs_dir / "_temp_filenames.json"
                try:
                    names_json.write_text(json.dumps(sample_names, ensure_ascii=False), encoding="utf-8")
                    res = self.runner(
                        [str(self.executable), "sample-queries", str(names_json), "42", "10"], creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
                        capture_output=True,
                        text=True,
                        encoding="utf-8",
                        timeout=10,
                        check=False,
                    )
                    if res.returncode == 0:
                        parsed = json.loads(res.stdout)
                        target_file.write_text(json.dumps(parsed, ensure_ascii=False, indent=2), encoding="utf-8")
                        return target_file
                except Exception:
                    pass
                finally:
                    if names_json.exists():
                        try:
                            names_json.unlink()
                        except OSError:
                            pass

        # 기본 질의 세트 생성 및 반환
        default_file = qs_dir / "default_query_set.json"
        data = {
            "schema_version": 1,
            "set_version": "1.0.0",
            "seed": 42,
            "queries": DEFAULT_QUERIES,
        }
        default_file.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
        return default_file

    def bench_start(self, options: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        if not self.developer_mode_checker():
            raise ServiceError("DEVELOPER_MODE_REQUIRED", "개발자 모드에서만 실행할 수 있습니다.")

        with self._lock:
            if self._current_process is not None and self._current_process.poll() is None:
                raise ServiceError("ALREADY_RUNNING", "벤치마크가 이미 실행 중입니다.")

            if not self.executable.is_file():
                raise ServiceError("ENGINE_MISSING", "벤치마크 엔진(qms_bench)을 찾을 수 없습니다.")

            opts = options or {}
            repeats = opts.get("repeats", 10)
            if not isinstance(repeats, int) or repeats < 1 or repeats > 1000:
                raise ServiceError("INVALID_INPUT", "반복 횟수는 1 이상 1,000 이하의 정수여야 합니다.")

            query_set_type = opts.get("query_set_type", "default")
            if query_set_type not in ("default", "my_files"):
                query_set_type = "default"

            include_raw = bool(opts.get("include_raw", False))
            adapter = opts.get("adapter", "everything")
            if adapter not in ("everything", "kfind", "fake", "http", "es"):
                adapter = "everything"

            qs_path = self._get_or_create_query_set(query_set_type, include_raw)

            cmd = [
                str(self.executable),
                "run",
                "--adapter",
                adapter,
                "--query-set",
                str(qs_path),
                "--out-dir",
                str(self.benchmarks_dir),
                "--repeats",
                str(repeats),
            ]

            self._events = []
            self._status = "running"
            self._error_message = None
            self._current_result = None
            self._run_id = None

            try:
                proc = self.popen(
                    cmd, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    encoding="utf-8",
                    bufsize=1,
                )
            except OSError as e:
                self._status = "failed"
                self._error_message = f"프로세스 시작 실패: {e}"
                raise ServiceError("ENGINE_FAILED", self._error_message)

            self._current_process = proc
            thread = threading.Thread(
                target=self._reader_worker, args=(proc,), daemon=True
            )
            thread.start()

            return {
                "started": True,
                "status": "running",
                "repeats": repeats,
                "query_set_type": query_set_type,
                "adapter": adapter,
            }

    def _reader_worker(self, proc: subprocess.Popen) -> None:
        if proc.stdout:
            for line in proc.stdout:
                line_str = line.strip()
                if not line_str:
                    continue
                try:
                    event_data = json.loads(line_str)
                    if isinstance(event_data, dict):
                        with self._lock:
                            self._events.append(event_data)
                            if event_data.get("event") == "start":
                                self._run_id = event_data.get("run_id")
                            elif event_data.get("event") == "complete":
                                self._run_id = event_data.get("run_id")
                                json_path_str = event_data.get("json_path")
                                if json_path_str and Path(json_path_str).is_file():
                                    try:
                                        self._current_result = json.loads(
                                            Path(json_path_str).read_text(encoding="utf-8")
                                        )
                                    except Exception:
                                        pass
                except json.JSONDecodeError:
                    pass

        proc.wait()
        with self._lock:
            if self._status == "running":
                if proc.returncode == 0:
                    self._status = "completed"
                else:
                    self._status = "failed"
                    err = proc.stderr.read() if proc.stderr else ""
                    self._error_message = err.strip() or f"종료 코드: {proc.returncode}"

    def bench_status(self) -> Dict[str, Any]:
        with self._lock:
            # 프로세스가 끝났는지 확인
            if (
                self._status == "running"
                and self._current_process is not None
                and self._current_process.poll() is not None
            ):
                if self._current_process.returncode == 0:
                    self._status = "completed"
                else:
                    self._status = "failed"
                    err = (
                        self._current_process.stderr.read()
                        if self._current_process.stderr
                        else ""
                    )
                    self._error_message = err.strip() or f"종료 코드: {self._current_process.returncode}"

            return {
                "status": self._status,
                "run_id": self._run_id,
                "events_count": len(self._events),
                "events": list(self._events),
                "error": self._error_message,
                "result": self._current_result,
            }

    def bench_cancel(self) -> Dict[str, Any]:
        with self._lock:
            if self._current_process is not None and self._current_process.poll() is None:
                try:
                    self._current_process.terminate()
                except OSError:
                    pass
                self._status = "cancelled"
                return {"cancelled": True}
            return {"cancelled": False, "message": "실행 중인 벤치마크가 없습니다."}

    def bench_list_results(self) -> Dict[str, Any]:
        results = []
        for p in sorted(self.benchmarks_dir.glob("run-*.json"), key=os.path.getmtime, reverse=True):
            if not p.is_file():
                continue
            try:
                data = json.loads(p.read_text(encoding="utf-8"))
                run_meta = data.get("run", {})
                summary_meta = data.get("summary", {})
                results.append(
                    {
                        "filename": p.name,
                        "path": str(p),
                        "run_id": run_meta.get("run_id", p.stem),
                        "adapter": run_meta.get("adapter"),
                        "repeats": run_meta.get("repeats"),
                        "total_queries": run_meta.get("total_queries"),
                        "completed": summary_meta.get("completed", False),
                        "mtime": os.path.getmtime(p),
                        "size_bytes": p.stat().st_size,
                    }
                )
            except Exception:
                continue
        return {"results": results}

    def bench_open_folder(self, payload: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        target = self.benchmarks_dir
        if payload and payload.get("folder"):
            target = self._verify_path_inside_benchmarks(payload.get("folder"))
        try:
            self.opener(str(target))
            return {"opened": True, "path": str(target)}
        except Exception as e:
            raise ServiceError("OPEN_FOLDER_FAILED", f"폴더 열기에 실패했습니다: {e}")

    def execute(self, action: Optional[str], payload: Optional[Dict[str, Any]] = None) -> Any:
        if action == "detect":
            return self.bench_detect()
        if action == "start":
            return self.bench_start(payload)
        if action == "status":
            return self.bench_status()
        if action == "cancel":
            return self.bench_cancel()
        if action == "list_results":
            return self.bench_list_results()
        if action == "open_folder":
            return self.bench_open_folder(payload)
        raise ServiceError("INVALID_ACTION", f"지원하지 않는 벤치마크 명령입니다: {action}")
