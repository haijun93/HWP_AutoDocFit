"""몽땅검색 색인 backend 계약과 기존 SQLite 구현 어댑터."""
from __future__ import annotations

from abc import ABC, abstractmethod
import json
import logging
import os
import shutil
import subprocess
import threading
from datetime import datetime, timezone
from pathlib import Path
import re
from threading import Event
from typing import Any


class SearchBackend(ABC):
    """문서 색인·검색 backend의 service 경계."""

    @abstractmethod
    def index_file(self, path: str) -> dict[str, Any]:
        """파일을 색인하고 기존 색인 응답 사전을 돌려준다."""

    @abstractmethod
    def remove_file(self, path: str) -> None:
        """파일의 색인 데이터를 제거한다."""

    @abstractmethod
    def search(self, query: str, *, roots=None, hierarchy: bool = False,
               limit: int = 100, offset: int = 0,
               sort: str = "hierarchy_relevance", start=None, end=None,
               related_groups: bool = False, filenames: bool = False) -> list[dict[str, Any]]:
        """기존 본문검색 또는 위계검색 결과 형식으로 검색한다."""

    @abstractmethod
    def status(self, *, roots=None) -> dict[str, Any]:
        """색인 상태 요약을 돌려준다."""

    @abstractmethod
    def cancel(self, cancel_event: Event) -> bool:
        """진행 중 작업의 취소 이벤트를 설정하고 취소 요청 여부를 돌려준다."""


class SqliteBackend(SearchBackend):
    """ContentSearchIndex를 감싸는 하위 호환 어댑터."""

    def __init__(self, index):
        self.index = index

    def index_file(self, path: str) -> dict[str, Any]:
        return self.index.index_file(path)

    def index_files(self, paths, *, batch_size=250):
        # SQLite를 기본값으로 둘 때 기존 파일별 호출 순서와 응답을 보존한다.
        return [self.index_file(path) for path in paths]

    def remove_file(self, path: str) -> None:
        return self.index.remove_file(path)

    def search(self, query: str, *, roots=None, hierarchy: bool = False,
               limit: int = 100, offset: int = 0,
               sort: str = "hierarchy_relevance", start=None, end=None,
               related_groups: bool = False, filenames: bool = False) -> list[dict[str, Any]]:
        if related_groups:
            from .related_search import search_related
            return search_related(self.index, query, roots=roots, limit=limit, offset=offset,
                                  start=start, end=end, filenames=filenames)
        if filenames:
            return self.index.search_files(query, roots=roots, limit=limit, start=start, end=end)
        if hierarchy:
            return self.index.search(query, roots=roots, limit=limit, sort=sort, start=start, end=end)
        return self.index.keyword_search(query, roots=roots, limit=limit, offset=offset,
                                         start=start, end=end)

    def status(self, *, roots=None) -> dict[str, Any]:
        return self.index.status_summary(roots=roots)

    def cancel(self, cancel_event: Event) -> bool:
        was_cancelled = cancel_event.is_set()
        cancel_event.set()
        return not was_cancelled


class RustBackend(SearchBackend):
    """Long-lived klib JSONL client. Set KLIB_EXE to select a local binary."""

    def __init__(self, executable=None, *, index_dir=None, timeout=30.0):
        candidate = executable or os.environ.get("KLIB_EXE") or shutil.which("klib.exe") or shutil.which("klib")
        self.executable = os.fspath(candidate) if candidate else None
        self.index_dir = os.fspath(index_dir) if index_dir else os.environ.get("KLIB_TANTIVY_DIR")
        self.timeout = timeout
        self._process = None
        self._lock = threading.RLock()

    def _request(self, payload):
        with self._lock:
            if not self.executable:
                raise RuntimeError("klib 실행 파일을 찾지 못했습니다 (KLIB_EXE 미설정).")
            if self._process is None or self._process.poll() is not None:
                env = os.environ.copy()
                if self.index_dir:
                    env["KLIB_TANTIVY_DIR"] = self.index_dir
                self._process = subprocess.Popen(
                    [self.executable, "search-content"], stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                    encoding="utf-8", errors="replace", bufsize=1, env=env,
                )
            process = self._process
            process.stdin.write(json.dumps(payload, ensure_ascii=False) + "\n")
            process.stdin.flush()
            ready = threading.Event()
            response = []
            def read_line():
                response.append(process.stdout.readline())
                ready.set()
            reader = threading.Thread(target=read_line, daemon=True)
            reader.start()
            if not ready.wait(self.timeout):
                self._stop_locked()
                raise TimeoutError(f"klib {payload.get('op')} 요청이 {self.timeout:g}초를 넘었습니다.")
            if not response[0]:
                returncode = process.poll()
                self._stop_locked()
                raise RuntimeError(f"klib 프로세스가 응답 전에 종료되었습니다 (exit={returncode}).")
            try:
                result = json.loads(response[0])
            except json.JSONDecodeError as error:
                self._stop_locked()
                raise RuntimeError(f"klib JSON 응답을 읽지 못했습니다: {error}") from error
            if not result.get("ok"):
                raise RuntimeError(result.get("error") or "klib 요청 실패")
            return result

    def _stop_locked(self):
        process, self._process = self._process, None
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
        if process is not None:
            for stream in (process.stdin, process.stdout):
                if stream is not None:
                    stream.close()

    def close(self):
        with self._lock:
            if self._process is not None and self._process.poll() is None:
                try:
                    self._process.stdin.write('{"op":"shutdown"}\n')
                    self._process.stdin.flush()
                    self._process.stdin.close()
                    self._process.wait(timeout=2)
                except (OSError, subprocess.TimeoutExpired):
                    self._stop_locked()
            if self._process is not None:
                for stream in (self._process.stdin, self._process.stdout):
                    if stream is not None:
                        stream.close()
            self._process = None

    def index_file(self, path):
        return {key: value for key, value in self._request({"op": "index_file", "path": os.path.abspath(path)}).items()
                if key in {"indexed", "changed", "error"}}

    def index_files(self, paths, *, batch_size=250):
        paths = list(paths)
        if not paths:
            return []
        if not 1 <= batch_size <= 500:
            raise ValueError("batch_size는 1~500이어야 합니다.")
        results = []
        for start in range(0, len(paths), batch_size):
            batch = [os.path.abspath(path) for path in paths[start:start + batch_size]]
            results.extend(self._request({"op": "index_batch", "paths": batch}).get("results", []))
        return results

    def remove_file(self, path):
        self._request({"op": "remove_file", "path": os.path.abspath(path)})

    @staticmethod
    def _document(path, records):
        path = os.path.abspath(path)
        stat = os.stat(path)
        return {"path": path, "name": Path(path).name,
                "mtime_us": int(stat.st_mtime * 1_000_000), "mtime": stat.st_mtime,
                "size": stat.st_size,
                "paragraphs": [{"paragraph_index": r.paragraph_index, "text": r.text,
                                "level_rank": r.level_rank, "level": r.level} for r in records]}

    def index_records(self, path, records):
        """이미 추출한 문단도 같은 프로세스·v5 색인에 반영한다."""
        return self._request(dict(self._document(path, records), op="replace"))

    def index_many(self, documents):
        return self._request({"op": "replace_many", "documents": [
            self._document(path, records) for path, records in documents]})

    def search(self, query, *, roots=None, hierarchy=False, limit=100, offset=0,
               sort="hierarchy_relevance", start=None, end=None, related_groups=False,
               filenames=False):
        if related_groups:
            # SEMI1 응답은 이미 공통 행 계약이다. 기존 SRCH 변환을 거치면 근거를 잃는다.
            return self._request({"op": "search", "query": query, "related_groups": True,
                "filenames": filenames, "roots": [os.path.abspath(p) for p in roots] if roots else [],
                "limit": limit, "offset": offset, "start": start, "end": end})["results"]
        if filenames:
            raise ValueError("Rust 파일명 검색은 related_groups=True 또는 names 서비스를 사용하세요.")
        result = self._request({"op": "search", "query": query, "roots": roots or [],
                                "hierarchy": hierarchy, "limit": limit, "offset": offset,
                                "sort": sort, "start": start, "end": end})
        rows = result.get("results", [])
        terms = [term for term in re.findall(r"[^\W_]+", query, re.UNICODE) if term]
        converted = []
        for rank, row in enumerate(rows, offset + 1):
            path = os.path.abspath(row.get("path", ""))
            name = row.get("name") or os.path.basename(path)
            extension = os.path.splitext(path)[1].lstrip(".").lower()
            snippet = row.get("snippet", "")
            spans = []
            for term in terms:
                match = re.search(re.escape(term), snippet, re.IGNORECASE)
                if match:
                    spans.append([match.start(), match.end()])
            position = row.get("position") or {"page": None, "sheet": None, "row": None}
            if hierarchy:
                locations = row.get("locations") or []
                converted.append({"path": path, "name": name, "extension": extension,
                    "mtime": row.get("mtime"), "folder_path": os.path.dirname(path),
                    "folder_name": os.path.basename(os.path.dirname(path)), "locations": locations,
                    "breadcrumb": row.get("breadcrumb", []), "sections": row.get("sections", []),
                    "breadcrumb_text": row.get("breadcrumb_text", ""), "snippet": snippet,
                    "highlights": spans, "position": position, "score": row.get("score", 0),
                    "relevance_rank": rank})
            else:
                counts = {term: snippet.casefold().count(term.casefold()) for term in dict.fromkeys(terms)}
                total = sum(counts.values())
                converted.append({"path": path, "name": name, "score": total,
                    "total_matches": total, "position": position, "best_level": "본문",
                    "breadcrumb": [], "breadcrumb_text": "", "context_hits": 0,
                    "match_counts": counts, "snippet": snippet, "highlights": spans,
                    "relevance_rank": rank})
        return converted

    def status(self, *, roots=None):
        result = self._request({"op": "status"})
        return {"needs_reindex_count": 0, "needs_reindex": False, "reindex_guide": None,
                "has_index": result.get("document_count", 0) > 0,
                "indexed_document_count": result.get("document_count", 0), "persistent_error_count": 0}

    def cancel(self, cancel_event):
        was_cancelled = cancel_event.is_set()
        cancel_event.set()
        with self._lock:
            self._stop_locked()
        return not was_cancelled


class ModeSearchBackend(SearchBackend):
    """Keeps SQLite as the configured default and the shadow response authority."""

    def __init__(self, sqlite, rust, mode_provider, *, shadow_log=None):
        self.sqlite = sqlite
        self.rust = rust
        self.mode_provider = mode_provider
        self.shadow_log = Path(shadow_log) if shadow_log else None

    def _mode(self):
        return self.mode_provider() or "sqlite"

    def close(self):
        close = getattr(self.rust, "close", None)
        if close:
            close()

    def _log(self, record):
        try:
            path = self.shadow_log
            if path is None:
                root = Path(os.environ.get("LOCALAPPDATA", Path.home())) / "HwpAutoDocFit"
                path = root / "search-shadow-compare.jsonl"
            path.parent.mkdir(parents=True, exist_ok=True)
            record["timestamp"] = datetime.now(timezone.utc).isoformat()
            with path.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(record, ensure_ascii=False) + "\n")
        except OSError:
            logging.getLogger(__name__).exception("shadow_compare 결과 JSONL 기록 실패")

    def _operate(self, method, *args, **kwargs):
        mode = self._mode()
        if mode == "sqlite":
            return getattr(self.sqlite, method)(*args, **kwargs)
        if mode == "shadow_compare":
            sqlite_result = getattr(self.sqlite, method)(*args, **kwargs)
            try:
                rust_result = getattr(self.rust, method)(*args, **kwargs)
                if method == "search":
                    sqlite_paths = [row.get("path") for row in sqlite_result[:20]]
                    rust_paths = [row.get("path") for row in rust_result[:20]]
                    left, right = set(sqlite_paths), set(rust_paths)
                    all_left = {row.get("path") for row in sqlite_result}
                    all_right = {row.get("path") for row in rust_result}
                    related = {}
                    if kwargs.get("related_groups"):
                        related = {"related_groups": True, "filenames": bool(kwargs.get("filenames")),
                            "result_order_match": [r.get("path") for r in sqlite_result] == [r.get("path") for r in rust_result],
                            "group_matches_match": {r.get("path"): r.get("group_matches") for r in sqlite_result}
                                == {r.get("path"): r.get("group_matches") for r in rust_result}}
                    self._log({"operation": method, "query": args[0] if args else "",
                               **related,
                               "sqlite_top20": sqlite_paths, "rust_top20": rust_paths,
                               "sqlite_result_count": len(sqlite_result), "rust_result_count": len(rust_result),
                               "result_set_match": all_left == all_right,
                               "top20_set_match": left == right,
                               "top20_overlap_count": len(left & right),
                               "match_rate": len(left & right) / max(1, min(20, len(left), len(right))),
                               "missing_in_rust": sorted(left - right), "extra_in_rust": sorted(right - left)})
                else:
                    self._log({"operation": method, "status": "ok"})
            except Exception as error:
                self._log({"operation": method, "status": "rust_error", "reason": str(error)})
                logging.getLogger(__name__).warning("shadow_compare Rust %s 실패: %s", method, error)
            return sqlite_result
        try:
            return getattr(self.rust, method)(*args, **kwargs)
        except Exception as error:
            logging.getLogger(__name__).warning("Rust %s 실패, SQLite fallback 사용: %s", method, error)
            return getattr(self.sqlite, method)(*args, **kwargs)

    def index_file(self, path): return self._operate("index_file", path)
    def index_files(self, paths, *, batch_size=250):
        paths = list(paths)
        mode = self._mode()
        if mode == "sqlite":
            return self.sqlite.index_files(paths, batch_size=batch_size)
        if mode == "shadow_compare":
            sqlite_rows = self.sqlite.index_files(paths, batch_size=batch_size)
            try:
                self.rust.index_files(paths, batch_size=batch_size)
            except Exception as error:
                self._log({"operation": "index_files", "status": "rust_error", "reason": str(error)})
                logging.getLogger(__name__).warning("shadow_compare Rust index_files 실패: %s", error)
            return sqlite_rows
        try:
            rust_rows = self.rust.index_files(paths, batch_size=batch_size)
            if len(rust_rows) != len(paths):
                raise RuntimeError(f"Rust 배치 응답 수 불일치: {len(rust_rows)} / {len(paths)}")
        except Exception as error:
            logging.getLogger(__name__).warning("Rust index_files 실패, SQLite fallback 사용: %s", error)
            return self.sqlite.index_files(paths, batch_size=batch_size)
        # 파일별 추출 실패는 SQLite 단건 경로로 되돌려 기존 형식 지원을 유지한다.
        return [self.sqlite.index_file(path) if row.get("error") else row
                for path, row in zip(paths, rust_rows)]
    def remove_file(self, path): return self._operate("remove_file", path)
    def search(self, query, **kwargs): return self._operate("search", query, **kwargs)
    def status(self, **kwargs): return self._operate("status", **kwargs)
    def cancel(self, cancel_event): return self._operate("cancel", cancel_event)
