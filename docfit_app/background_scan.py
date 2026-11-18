"""Thread-backed, pollable scan task shared by document and photo services."""
from __future__ import annotations

import threading
import uuid
from typing import Any, Callable


class BackgroundScan:
    def __init__(self) -> None:
        self._lock = threading.RLock()
        self._task_id: str | None = None
        self._cancel = threading.Event()
        self._state: dict[str, Any] = {"state": "idle", "progress": 0, "visited": 0}

    def start(self, worker: Callable[[Callable[[int], None], threading.Event], dict[str, Any]]) -> dict[str, Any]:
        with self._lock:
            if self._state.get("state") in ("running", "cancelling"):
                return {"ok": False, "code": "SCAN_RUNNING", **self._state}
            self._task_id = uuid.uuid4().hex
            task_id = self._task_id
            self._cancel = threading.Event()
            self._state = {"state": "running", "task_id": task_id, "progress": 0, "visited": 0}
            cancel = self._cancel

        def report(visited: int) -> None:
            with self._lock:
                if task_id != self._task_id or self._state.get("state") != "running":
                    return
                count = max(int(visited), int(self._state.get("visited", 0)))
                # 전체 항목 수는 사전 파악할 수 없으므로 진행 건수와 단조 증가 퍼센트를 함께 제공한다.
                percent = min(99, max(int(self._state.get("progress", 0)), count))
                self._state.update(visited=count, progress=percent)

        def run() -> None:
            try:
                result = worker(report, cancel)
                with self._lock:
                    if task_id == self._task_id:
                        stopped = cancel.is_set()
                        self._state.update(
                            state="cancelled" if stopped else "complete",
                            progress= int(self._state.get("progress", 0)) if stopped else 100,
                            result=result,
                        )
            except Exception as exc:
                with self._lock:
                    if task_id == self._task_id:
                        self._state.update(state="failed", error=str(exc))

        threading.Thread(target=run, name="docfit-library-scan", daemon=True).start()
        return {"ok": True, "task_id": task_id, "state": "running"}

    def progress(self) -> dict[str, Any]:
        with self._lock:
            return {"ok": True, **self._state}

    def cancel(self) -> dict[str, Any]:
        with self._lock:
            if self._state.get("state") == "running":
                self._cancel.set()
                self._state["state"] = "cancelling"
            return {"ok": True, **self._state}
