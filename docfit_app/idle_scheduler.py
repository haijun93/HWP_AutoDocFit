"""쉬는 시간 색인 스케줄러(IDLE1, 2026-11-13).

`IdleGate`(사용자 유휴 판정)를 한 곳에 두고, 급하지 않은 색인 작업(`IdleJob`)을 쉬는 시간에만 하나씩 돌린다.
지금 쓰는 곳: 타임머신 본문 자동 재색인, 업무자료함 자료 변경분 색인. 새 검색 엔진(kfind 서비스)의 `ingest`
보내기도 같은 인터페이스로 붙인다(`docs/IDLE1_IDLE_INDEXING_2026-11-13.md` 4절).

작업 하나의 상태(`status()["jobs"][이름]["state"]`):

    none ─request/주기 도래─▶ waiting ─쉬는 시간─▶ running ⇄ yielded(사용자 입력·보류)
                                                  └─▶ done | failed | cancelled ─(every 있으면)─▶ waiting

- 작업은 문서 하나를 시작하기 직전마다 `ctx.checkpoint()`를 부른다. 쉬는 시간이 아니면 그 자리에서 기다리고
  (진행 중인 문서는 끝낸 뒤), 다시 쉬는 시간이 오면 이어서 한다. 앱 종료·취소면 False를 돌려준다.
- `ctx.promote()`(사용자가 직접 같은 색인을 시작함)는 유휴 판정과 낮은 우선순위를 풀고 끝까지 바로 돌린다.
- 불가능한 전이: waiting에서 쉬는 시간 확인 없이 running, 한 번에 두 작업 running(작업은 한 스레드에서 차례로).
"""
from __future__ import annotations

import logging
import threading
import time
from typing import Protocol, runtime_checkable

from .idle_gate import IdleGate, begin_background, end_background

JOB_STATES = ("none", "waiting", "running", "yielded", "done", "failed", "cancelled")


@runtime_checkable
class IdleJob(Protocol):
    """쉬는 시간에 돌릴 작업. `run`은 스케줄러 스레드에서 불리며 끝날 때까지 돌아오지 않는다."""

    name: str

    def pending(self) -> bool:
        """할 일이 있을 수 있으면 True(가볍게 판단). False면 이번 차례를 건너뛴다."""

    def run(self, ctx: "IdleContext") -> None:
        """문서마다 `ctx.checkpoint()`가 False면 멈추고, `ctx.progress(n, m)`로 진행을 알린다."""


class IdleContext:
    """작업 한 번 실행의 손잡이. checkpoint는 일하는 스레드에서 불러야 한다(우선순위가 스레드 단위)."""

    def __init__(self, gate, name, *, on_change=None, poll_seconds=1.0):
        self.name = name
        self._gate = gate
        self._stop = threading.Event()
        self._foreground = threading.Event()
        self._on_change = on_change or (lambda ctx: None)
        self._poll_seconds = poll_seconds
        self._background_threads = set()
        self._lock = threading.Lock()
        self.done = 0
        self.total = None
        self.yielded = False

    # 상태 ------------------------------------------------------------
    @property
    def stopped(self):
        return self._stop.is_set()

    @property
    def foreground(self):
        return self._foreground.is_set()

    def stop(self):
        self._stop.set()

    def promote(self):
        """사용자가 직접 시작한 것으로 바꾼다: 더는 양보하지 않고 낮은 우선순위도 푼다(다음 checkpoint에서)."""
        self._foreground.set()
        self._set_yielded(False)

    def progress(self, done, total=None):
        with self._lock:
            self.done = int(done)
            self.total = None if total is None else int(total)
        self._on_change(self)

    def _set_yielded(self, value):
        with self._lock:
            changed, self.yielded = self.yielded != value, value
        if changed:
            self._on_change(self)

    # 일하는 스레드 쪽 ------------------------------------------------
    def enter_background(self):
        ident = threading.get_ident()
        if self.foreground or ident in self._background_threads:
            return
        if begin_background():
            self._background_threads.add(ident)

    def leave_background(self):
        ident = threading.get_ident()
        if ident in self._background_threads:
            end_background()
            self._background_threads.discard(ident)

    def checkpoint(self, cancel_event=None):
        """문서 하나를 시작해도 되는지. 쉬는 시간이 아니면 될 때까지 기다린다. 멈춰야 하면 False."""
        def halted():
            return self._stop.is_set() or (cancel_event is not None and cancel_event.is_set())

        while True:
            if halted():
                self._set_yielded(False)
                return False
            if self.foreground:
                self.leave_background()
                self._set_yielded(False)
                return True
            self.enter_background()
            if self._gate.poll().state == "idle":
                self._set_yielded(False)
                return True
            self._set_yielded(True)
            (cancel_event or self._stop).wait(self._poll_seconds)


class _Entry:
    def __init__(self, job, every):
        self.job = job
        self.every = every
        self.due = False
        self.next_at = None
        self.state = "none"
        self.last_run_at = None
        self.last_error = None
        self.ctx = None
        self.done = 0
        self.total = None


class IdleIndexScheduler:
    """등록된 작업을 쉬는 시간에 하나씩 돌린다. 스레드는 처음 등록할 때 하나 띄운다."""

    def __init__(self, gate=None, *, clock=time.monotonic, wall_clock=time.time, poll_seconds=1.0,
                 sleep_seconds=30.0):
        self.gate = gate if gate is not None else IdleGate()
        self._clock = clock
        self._wall_clock = wall_clock
        self._poll_seconds = poll_seconds
        self._sleep_seconds = sleep_seconds
        self._lock = threading.Condition()
        self._entries = {}
        self._stop = threading.Event()
        self._thread = None
        self._running = None

    # 설정 ------------------------------------------------------------
    @property
    def enabled(self):
        return self.gate.config.enabled

    def configure(self, *, enabled=None, idle_seconds=None):
        decision = self.gate.configure(enabled=enabled, idle_seconds=idle_seconds)
        with self._lock:
            self._lock.notify_all()
        return decision

    # 등록 ------------------------------------------------------------
    def register(self, job, *, every=None, due=False):
        """같은 이름이면 바꿔 끼운다. every(초)가 있으면 끝난 뒤 그만큼 지나 다시 돌린다."""
        if not isinstance(job, IdleJob):
            raise TypeError("IdleJob(name·pending·run)이 필요합니다.")
        with self._lock:
            old = self._entries.get(job.name)
            if old is not None and old.ctx is not None:
                old.ctx.stop()
            entry = _Entry(job, every)
            if every:
                entry.next_at = self._clock() + every
            if due:
                entry.due, entry.state = True, "waiting"
            self._entries[job.name] = entry
            self._ensure_thread()
            self._lock.notify_all()

    def unregister(self, name):
        with self._lock:
            entry = self._entries.pop(name, None)
            if entry is not None and entry.ctx is not None:
                entry.ctx.stop()
            self._lock.notify_all()

    def request(self, name):
        """곧 돌려 달라(쉬는 시간이 오면). 이미 도는 중이면 끝난 뒤 한 번 더."""
        with self._lock:
            entry = self._entries.get(name)
            if entry is None:
                return False
            entry.due = True
            if entry.state not in ("running", "yielded"):
                entry.state = "waiting"
            self._ensure_thread()
            self._lock.notify_all()
            return True

    def context(self, name):
        """지금 그 작업을 돌리는 중이면 그 손잡이(promote용), 아니면 None."""
        with self._lock:
            entry = self._entries.get(name)
            return entry.ctx if entry is not None else None

    # 상태 ------------------------------------------------------------
    def status(self):
        gate = self.gate.snapshot()
        with self._lock:
            jobs = {name: {"state": entry.state, "done": entry.done, "total": entry.total,
                           "last_run_at": entry.last_run_at, "last_error": entry.last_error,
                           "every_seconds": entry.every}
                    for name, entry in self._entries.items()}
            running = self._running
        return {"state": gate["state"], "reason": gate["reason"], "reason_text": gate["reason_text"],
                "enabled": gate["enabled"], "idle_seconds": gate["idle_seconds"],
                "running_job": running, "jobs": jobs}

    # 실행 ------------------------------------------------------------
    def _ensure_thread(self):
        if self._stop.is_set() or (self._thread is not None and self._thread.is_alive()):
            return
        self._thread = threading.Thread(target=self._loop, name="idle-index-scheduler", daemon=True)
        self._thread.start()

    def _next_entry(self):
        now = self._clock()
        for entry in self._entries.values():
            if not entry.due and entry.every and entry.next_at is not None and now >= entry.next_at:
                entry.due, entry.state = True, "waiting"
        due = [entry for entry in self._entries.values() if entry.due]
        return due[0] if due else None

    def _wait_seconds(self):
        now = self._clock()
        times = [entry.next_at - now for entry in self._entries.values() if entry.every and entry.next_at]
        return max(0.05, min([self._sleep_seconds, *times]))

    def _on_change(self, ctx):
        with self._lock:
            entry = self._entries.get(ctx.name)
            if entry is None or entry.ctx is not ctx:
                return
            entry.done, entry.total = ctx.done, ctx.total
            if entry.state in ("running", "yielded"):
                entry.state = "yielded" if ctx.yielded else "running"

    def _loop(self):
        while not self._stop.is_set():
            with self._lock:
                entry = self._next_entry()
                if entry is None:
                    self._lock.wait(self._wait_seconds())
                    continue
            # 쉬는 시간이 될 때까지 시작하지 않는다(설정이 꺼지면 paused라 계속 기다림).
            if self.gate.poll().state != "idle":
                self._stop.wait(self._poll_seconds)
                continue
            self._run_entry(entry)

    def _run_entry(self, entry):
        job = entry.job
        try:
            if not job.pending():
                with self._lock:
                    self._finish(entry, "done")
                return
        except Exception:
            logging.getLogger(__name__).exception("쉬는 시간 작업 확인 실패: %s", job.name)
        ctx = IdleContext(self.gate, job.name, on_change=self._on_change, poll_seconds=self._poll_seconds)
        with self._lock:
            if self._entries.get(job.name) is not entry:
                return  # 그 사이 등록이 풀렸다
            entry.due, entry.ctx, entry.state = False, ctx, "running"
            entry.done, entry.total, entry.last_error = 0, None, None
            self._running = job.name
        outcome = "done"
        try:
            job.run(ctx)
            if ctx.stopped:
                outcome = "cancelled"
        except Exception as exc:
            logging.getLogger(__name__).exception("쉬는 시간 작업 실패: %s", job.name)
            outcome = "failed"
            with self._lock:
                entry.last_error = f"{type(exc).__name__}: {exc}"
        finally:
            ctx.leave_background()
            with self._lock:
                entry.ctx = None
                self._running = None
                self._finish(entry, outcome)

    def _finish(self, entry, outcome):
        entry.state = "waiting" if entry.due else outcome
        entry.last_run_at = self._wall_clock()
        if entry.every:
            entry.next_at = self._clock() + entry.every
        self._lock.notify_all()

    def close(self, timeout=5.0):
        self._stop.set()
        with self._lock:
            for entry in self._entries.values():
                if entry.ctx is not None:
                    entry.ctx.stop()
            self._lock.notify_all()
        thread = self._thread
        if thread is not None and thread is not threading.current_thread():
            thread.join(timeout)

    def wait_idle_queue(self, timeout=5.0):
        """시험용: 도는 작업도, 기다리는 작업도 없을 때까지 기다린다."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            with self._lock:
                busy = self._running is not None or any(entry.due for entry in self._entries.values())
            if not busy:
                return True
            time.sleep(0.01)
        return False


_SHARED = None
_SHARED_LOCK = threading.Lock()


def shared_scheduler():
    """앱 전체에서 하나만 쓰는 스케줄러(유휴 판정을 한 곳에서)."""
    global _SHARED
    with _SHARED_LOCK:
        if _SHARED is None:
            _SHARED = IdleIndexScheduler()
        return _SHARED
