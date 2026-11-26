"""Thread-backed, pollable scan task shared by document and photo services."""
from __future__ import annotations

import threading
import time
import uuid
from typing import Any, Callable

# 한 번의 진행 응답에 싣는 새 행의 상한(브리지 전달·화면 구성이 한 번에 커지지 않게).
PAGE_ROWS = 4000


class BackgroundScan:
    def __init__(self) -> None:
        self._lock = threading.RLock()
        self._task_id: str | None = None
        self._cancel = threading.Event()
        self._state: dict[str, Any] = {"state": "idle", "visited": 0, "started_at": None, "last_progress_at": None, "current_folder": ""}
        self._rows: list[dict[str, Any]] = []
        self._streamed_key: str | None = None

    def start(self, worker: Callable[[Callable[..., None], threading.Event], dict[str, Any]], *,
              initial_result: dict[str, Any] | None = None, engine: str | None = None,
              streamed_key: str | None = None) -> dict[str, Any]:
        with self._lock:
            if self._state.get("state") in ("running", "cancelling"):
                return {"ok": False, "code": "SCAN_RUNNING", **self._state}
            self._task_id = uuid.uuid4().hex
            task_id = self._task_id
            self._cancel = threading.Event()
            self._rows = []
            self._streamed_key = streamed_key
            now = time.time()
            self._state = {"state": "running", "task_id": task_id, "visited": 0,
                           "started_at": now, "last_progress_at": now, "current_folder": ""}
            if engine:
                self._state["engine"] = engine
            cancel = self._cancel

        def report(visited: int, current_folder: str | None = None) -> None:
            with self._lock:
                if task_id != self._task_id or self._state.get("state") != "running":
                    return
                count = max(int(visited), int(self._state.get("visited", 0)))
                self._state.update(visited=count, last_progress_at=time.time())
                if current_folder is not None:
                    self._state["current_folder"] = str(current_folder)

        def publish(row: dict[str, Any]) -> None:
            """찾은 항목 하나를 바로 내보낸다(`progress(since)` 가 순서대로 가져간다)."""
            with self._lock:
                if task_id == self._task_id:
                    self._rows.append(row)

        report.publish = publish  # type: ignore[attr-defined]

        def run() -> None:
            try:
                result = worker(report, cancel)
                with self._lock:
                    if task_id == self._task_id:
                        if isinstance(result, dict) and result.get("engine"):
                            self._state["engine"] = result["engine"]
                        stopped = cancel.is_set()
                        self._state.update(
                            state="cancelled" if stopped else "complete",
                            result=result,
                        )
            except Exception as exc:
                with self._lock:
                    if task_id == self._task_id:
                        self._state.update(state="failed", error=str(exc))

        threading.Thread(target=run, name="docfit-library-scan", daemon=True).start()
        response = {"ok": True, "task_id": task_id, "state": "running"}
        if initial_result is not None:
            response.update(state="cached", result=initial_result, cache_state="cached")
        if engine:
            response["engine"] = engine
        return response

    def progress(self, since: int | None = None) -> dict[str, Any]:
        """현재 상태. `since` 를 주면 그 뒤의 새 행(최대 PAGE_ROWS)만 싣고, 최종 결과의 전체 목록은 뺀다."""
        with self._lock:
            response = {"ok": True, **self._state}
            if since is None:
                return response
            start = max(0, min(int(since), len(self._rows)))
            rows = self._rows[start:start + PAGE_ROWS]
            response.update(rows=rows, next=start + len(rows), total=len(self._rows))
            result = response.get("result")
            if self._streamed_key and isinstance(result, dict):
                response["result"] = {k: v for k, v in result.items() if k != self._streamed_key}
            return response

    def cancel(self) -> dict[str, Any]:
        with self._lock:
            if self._state.get("state") == "running":
                self._cancel.set()
                self._state["state"] = "cancelling"
            return {"ok": True, **self._state}
