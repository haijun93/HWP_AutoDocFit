"""Thread-backed, pollable scan task shared by document and photo services."""
from __future__ import annotations

import threading
import time
import uuid
from typing import Any, Callable


class BackgroundScan:
    def __init__(self) -> None:
        self._lock = threading.RLock()
        self._task_id: str | None = None
        self._cancel = threading.Event()
        self._state: dict[str, Any] = {"state": "idle", "visited": 0, "started_at": None, "last_progress_at": None, "current_folder": ""}

    def start(self, worker: Callable[[Callable[..., None], threading.Event], dict[str, Any]], *,
              initial_result: dict[str, Any] | None = None, engine: str | None = None) -> dict[str, Any]:
        with self._lock:
            if self._state.get("state") in ("running", "cancelling"):
                return {"ok": False, "code": "SCAN_RUNNING", **self._state}
            self._task_id = uuid.uuid4().hex
            task_id = self._task_id
            self._cancel = threading.Event()
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

    def progress(self) -> dict[str, Any]:
        with self._lock:
            return {"ok": True, **self._state}

    def cancel(self) -> dict[str, Any]:
        with self._lock:
            if self._state.get("state") == "running":
                self._cancel.set()
                self._state["state"] = "cancelling"
            return {"ok": True, **self._state}
