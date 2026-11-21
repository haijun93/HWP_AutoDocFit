"""시작 부가 작업을 첫 화면(웹 창 loaded) 이후로 미루고, 단계별 경과 시간을 한 줄로 남긴다.

개인정보·경로는 기록하지 않는다. 작업 실패는 로그만 남기고 앱을 멈추지 않는다.
"""
from __future__ import annotations

import logging
import threading
import time

_LOG = logging.getLogger("docfit_infra.startup")
_T0 = time.perf_counter()
_MARKS: dict[str, int] = {}
_LOCK = threading.Lock()


def mark(name: str) -> None:
    """처음 부른 시각만 기록한다(프로세스 시작 기준 ms)."""
    with _LOCK:
        _MARKS.setdefault(name, int((time.perf_counter() - _T0) * 1000))


def summary() -> str:
    with _LOCK:
        return " ".join(f"{k}={v}ms" for k, v in sorted(_MARKS.items(), key=lambda kv: kv[1]))


def log_summary() -> None:
    _LOG.info("startup timeline: %s", summary())


class DeferredStartup:
    """등록한 작업을 trigger() 최초 1회 때 각각 백그라운드 스레드로 실행한다."""

    def __init__(self, tasks):
        self._tasks = list(tasks)
        self._fired = False
        self._lock = threading.Lock()

    def trigger(self) -> bool:
        with self._lock:
            if self._fired:
                return False
            self._fired = True
        mark("deferred_start")
        for index, task in enumerate(self._tasks):
            threading.Thread(target=self._run, args=(task,), name=f"deferred-startup-{index}", daemon=True).start()
        log_summary()
        return True

    @staticmethod
    def _run(task) -> None:
        try:
            task()
        except Exception:
            _LOG.debug("deferred startup task failed", exc_info=True)
