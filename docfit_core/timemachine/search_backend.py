"""SEMI1 옵션 검색 어댑터. 기본 SQLite API는 그대로 위임한다.

SRCH 계열이 아직 병합되지 않은 기준선에서도 독립적으로 쓸 수 있다.
Rust 연관 색인은 기존 자료함 색인과 분리한 index_dir를 명시해야 한다.
"""
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
from typing import Protocol


class SearchBackend(Protocol):
    def index_file(self, path): ...
    def remove_file(self, path): ...
    def search(self, query, **options): ...
    def status(self, **options): ...
    def cancel(self, cancel_event): ...


class SqliteBackend:
    def __init__(self, index):
        self.index = index

    def index_file(self, path):
        return self.index.index_file(path)

    def remove_file(self, path):
        return self.index.remove_file(path)

    def search(self, query, *, roots=None, hierarchy=False, limit=100, offset=0,
               sort="hierarchy_relevance", start=None, end=None, related_groups=False,
               filenames=False):
        if related_groups:
            from .related_search import search_related
            return search_related(self.index, query, roots=roots, limit=limit, offset=offset,
                                  start=start, end=end, filenames=filenames)
        if filenames:
            return self.index.search_files(query, roots=roots, limit=limit, start=start, end=end)
        if hierarchy:
            return self.index.search(query, roots=roots, limit=limit, sort=sort, start=start, end=end)
        return self.index.keyword_search(query, roots=roots, limit=limit, offset=offset, start=start, end=end)

    def status(self, *, roots=None):
        return self.index.status_summary(roots=roots)

    def cancel(self, cancel_event):
        previous = cancel_event.is_set()
        cancel_event.set()
        return not previous


class RustBackend:
    """Explicit related_groups=True opt-in; legacy requests can use a supplied fallback.

    State: stopped -> ready -> request -> ready; I/O/timeout -> stopped.
    A single reader owns stdout for the process lifetime; timed-out replies are discarded
    with their process, so the next request cannot consume an older response.
    """
    def __init__(self, executable=None, *, index_dir, fallback=None, timeout=30):
        self.executable = os.fspath(executable or os.environ.get("KLIB_EXE", "klib.exe"))
        self.index_dir = os.fspath(index_dir)
        self.fallback = fallback
        self.timeout = timeout
        self._process = None
        self._lock = threading.RLock()

    def _request(self, payload):
        with self._lock:
            if self._process is None:
                process = subprocess.Popen([self.executable, "search-content", self.index_dir],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                    text=True, encoding="utf-8", bufsize=1)
                self._process = process
                replies = queue.Queue()
                self._replies = replies
                def reader():
                    try:
                        for line in process.stdout:
                            replies.put(line)
                    finally:
                        replies.put(None)
                self._reader = threading.Thread(target=reader, daemon=True)
                self._reader.start()
            try:
                self._process.stdin.write(json.dumps(payload, ensure_ascii=False) + "\n")
                self._process.stdin.flush()
                line = self._replies.get(timeout=self.timeout)
                if line is None:
                    raise RuntimeError("Rust 검색 프로세스가 종료되었습니다.")
                response = json.loads(line)
                if not response.get("ok"):
                    raise ValueError(response.get("error", "Rust 검색 실패"))
                return response
            except (OSError, queue.Empty, json.JSONDecodeError, RuntimeError):
                self.close()
                raise

    def index_file(self, path):
        from .content_search import extract_paragraphs
        path = os.path.abspath(path)
        records = extract_paragraphs(path)
        return self.index_records(path, records)

    def index_records(self, path, records):
        """추출된 사본/합성 문단을 전달한다. 검색할 때 원문 접근 없음."""
        payload = self._document(path, records)
        return self._request(dict(payload, op="replace"))

    @staticmethod
    def _document(path, records):
        path = os.path.abspath(path)
        stat = os.stat(path)
        return {"path": path, "name": Path(path).name,
            "mtime_us": int(stat.st_mtime * 1_000_000), "mtime": stat.st_mtime, "size": stat.st_size,
            "paragraphs": [{"paragraph_index": r.paragraph_index, "text": r.text,
                            "level_rank": r.level_rank, "level": r.level} for r in records]}

    def index_many(self, documents):
        """(경로, ParagraphRecord 목록)을 한 writer 커밋으로 반영한다."""
        return self._request({"op": "replace_many", "documents": [
            self._document(path, records)
            for path, records in documents]})

    def remove_file(self, path):
        self._request({"op": "remove", "path": os.path.abspath(path)})

    def search(self, query, *, roots=None, hierarchy=False, limit=100, offset=0,
               sort="hierarchy_relevance", start=None, end=None, related_groups=False,
               filenames=False):
        if not related_groups:
            if self.fallback is None:
                raise ValueError("연관 검색 옵션 또는 SQLite fallback이 필요합니다.")
            return self.fallback.search(query, roots=roots, hierarchy=hierarchy, limit=limit,
                offset=offset, sort=sort, start=start, end=end, filenames=filenames)
        return self._request({"op": "search", "query": query, "related_groups": True,
            "filenames": filenames, "limit": limit, "offset": offset,
            "roots": [os.path.abspath(p) for p in roots] if roots else [],
            "start": start, "end": end})["results"]

    def status(self, *, roots=None):
        return self._request({"op": "status", "roots": [os.path.abspath(p) for p in roots] if roots else []})["status"]

    def close(self):
        with self._lock:
            process, self._process = self._process, None
            if process:
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=5)
                self._reader.join(timeout=1)
                process.stdin.close()
                process.stdout.close()

    def cancel(self, cancel_event):
        previous = cancel_event.is_set()
        cancel_event.set()
        # Interrupt the child before waiting for the request lock. The stdout
        # reader then signals EOF to the blocked request immediately.
        process = self._process
        if process is not None and process.poll() is None:
            process.terminate()
        self.close()
        return not previous
