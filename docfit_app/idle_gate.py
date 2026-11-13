"""사용자 유휴 판정 상태 기계(IDLE1, 2026-11-13).

검색 색인처럼 급하지 않은 일을 "쉬는 시간"에만 돌리기 위해, 흩어지기 쉬운 판정 조건을 한 곳에 모은다.

상태(값은 `IdleDecision.state`):

| 상태 | 뜻 | 들어오는 조건 |
|---|---|---|
| `paused` | 사용자가 "쉬는 시간에 색인"을 껐다 | 설정 꺼짐(다른 조건보다 먼저) |
| `active` | 사용자가 쓰는 중 | 마지막 입력이 `idle_seconds`보다 최근 |
| `held`   | 입력은 없지만 지금 돌리면 방해된다 | 배터리 전원·전체 화면/발표 모드·다른 프로그램의 CPU·디스크 부하 높음·입력 시각을 모름 |
| `idle`   | 쉬는 시간: 색인을 돌려도 된다 | 위 어느 것도 아님 |

- 전이는 `decide(이전 상태, 신호, 설정)` 하나로만 정한다(순수 함수, 시험 가능). 어느 상태에서 어느 상태로도 갈 수 있으나
  `idle`로는 반드시 `decide`가 모든 조건을 확인한 뒤에만 간다(설정 꺼짐·입력 있음 상태에서 바로 `idle` 불가).
- 부하 기준은 들어갈 때(`*_enter`)와 나갈 때(`*_exit`)를 달리 둬서 색인 자신이 만든 작은 흔들림으로 들락거리지 않게 한다.
  CPU·디스크는 **이 앱 프로세스 몫을 뺀** 다른 프로그램의 부하로 본다(색인 자체가 부하를 올려 스스로 멈추지 않게).
- 입력은 부하와 달리 기준을 두 개로 나누지 않는다. 입력이 들어오면 곧바로 `active`(즉시 양보).
"""
from __future__ import annotations

import contextlib
import ctypes
import os
import subprocess
import threading
import time
from dataclasses import asdict, dataclass, replace

STATES = ("active", "idle", "held", "paused")
DEFAULT_IDLE_SECONDS = 120
MIN_IDLE_SECONDS = 15
MAX_IDLE_SECONDS = 3600

# Windows 상수
_THREAD_MODE_BACKGROUND_BEGIN = 0x00010000
_THREAD_MODE_BACKGROUND_END = 0x00020000
# 자식 프로세스(klib 등)를 낮은 우선순위로 띄울 때 쓰는 생성 플래그. Windows가 아니면 0.
LOW_PRIORITY_CREATIONFLAGS = getattr(subprocess, "IDLE_PRIORITY_CLASS", 0)


@dataclass(frozen=True)
class IdleSignals:
    """한 번 잰 신호. 모르는 값은 None(그 조건은 판정에서 빼되, 입력 시각을 모르면 보류)."""
    input_idle_seconds: float | None = None
    cpu_percent: float | None = None          # 이 프로세스를 뺀 전체 CPU 사용률(0~100)
    disk_bytes_per_second: float | None = None  # 이 프로세스를 뺀 디스크 읽기+쓰기
    on_battery: bool | None = None
    fullscreen: bool | None = None            # 전체 화면 앱·D3D 전체 화면·발표 모드


@dataclass(frozen=True)
class IdleConfig:
    enabled: bool = True
    idle_seconds: int = DEFAULT_IDLE_SECONDS
    cpu_enter: float = 25.0
    cpu_exit: float = 60.0
    disk_enter: float = 8 * 1024 * 1024
    disk_exit: float = 32 * 1024 * 1024


@dataclass(frozen=True)
class IdleDecision:
    state: str
    reason: str  # disabled | input | input_unknown | battery | fullscreen | cpu | disk | idle


def clamp_idle_seconds(value):
    """설정 값 검사. 정수가 아니거나 범위 밖이면 ValueError."""
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError("유휴 기준 시간(초)은 정수로 지정해 주세요.")
    if not MIN_IDLE_SECONDS <= value <= MAX_IDLE_SECONDS:
        raise ValueError(f"유휴 기준 시간은 {MIN_IDLE_SECONDS}~{MAX_IDLE_SECONDS}초로 지정해 주세요.")
    return value


def decide(previous, signals, config):
    """상태 전이 규칙의 유일한 정의. previous는 STATES 중 하나(처음이면 None)."""
    if not config.enabled:
        return IdleDecision("paused", "disabled")
    if signals.input_idle_seconds is None:
        return IdleDecision("held", "input_unknown")
    if signals.input_idle_seconds < config.idle_seconds:
        return IdleDecision("active", "input")
    if signals.on_battery:
        return IdleDecision("held", "battery")
    if signals.fullscreen:
        return IdleDecision("held", "fullscreen")
    staying = previous == "idle"
    cpu_limit = config.cpu_exit if staying else config.cpu_enter
    disk_limit = config.disk_exit if staying else config.disk_enter
    if signals.cpu_percent is not None and signals.cpu_percent > cpu_limit:
        return IdleDecision("held", "cpu")
    if signals.disk_bytes_per_second is not None and signals.disk_bytes_per_second > disk_limit:
        return IdleDecision("held", "disk")
    return IdleDecision("idle", "idle")


REASON_TEXT = {
    "disabled": "쉬는 시간 색인이 꺼져 있음",
    "input": "사용 중",
    "input_unknown": "입력 시각을 알 수 없음",
    "battery": "배터리 전원",
    "fullscreen": "전체 화면·발표 모드",
    "cpu": "다른 프로그램이 CPU를 많이 씀",
    "disk": "다른 프로그램이 디스크를 많이 씀",
    "idle": "쉬는 시간",
}


# ---------------------------------------------------------------- Windows 신호 측정
class _FILETIME(ctypes.Structure):
    _fields_ = [("low", ctypes.c_uint32), ("high", ctypes.c_uint32)]

    def value(self):
        return (self.high << 32) | self.low


class _LASTINPUTINFO(ctypes.Structure):
    _fields_ = [("cbSize", ctypes.c_uint32), ("dwTime", ctypes.c_uint32)]


class _SYSTEM_POWER_STATUS(ctypes.Structure):
    _fields_ = [("ACLineStatus", ctypes.c_ubyte), ("BatteryFlag", ctypes.c_ubyte),
                ("BatteryLifePercent", ctypes.c_ubyte), ("SystemStatusFlag", ctypes.c_ubyte),
                ("BatteryLifeTime", ctypes.c_uint32), ("BatteryFullLifeTime", ctypes.c_uint32)]


# SHQueryUserNotificationState 값: 2 전체 화면 앱, 3 D3D 전체 화면, 4 발표 모드
_BUSY_NOTIFICATION_STATES = frozenset((2, 3, 4))

_KERNEL32 = None


def _kernel32():
    """핸들 인자형을 밝힌 전용 kernel32(64비트에서 의사 핸들 -1이 잘리지 않게). 전역 windll 원형은 건드리지 않는다."""
    global _KERNEL32
    if _KERNEL32 is None:
        dll = ctypes.WinDLL("kernel32", use_last_error=True)
        handle, pointer, bool_ = ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int
        dll.GetCurrentProcess.restype = handle
        dll.GetCurrentProcess.argtypes = []
        dll.GetCurrentThread.restype = handle
        dll.GetCurrentThread.argtypes = []
        dll.GetProcessTimes.restype = bool_
        dll.GetProcessTimes.argtypes = [handle, pointer, pointer, pointer, pointer]
        dll.GetSystemTimes.restype = bool_
        dll.GetSystemTimes.argtypes = [pointer, pointer, pointer]
        dll.SetThreadPriority.restype = bool_
        dll.SetThreadPriority.argtypes = [handle, ctypes.c_int]
        dll.GetSystemPowerStatus.restype = bool_
        dll.GetSystemPowerStatus.argtypes = [pointer]
        dll.GetTickCount.restype = ctypes.c_uint32
        dll.GetTickCount.argtypes = []
        _KERNEL32 = dll
    return _KERNEL32


class WindowsIdleProbe:
    """Windows API로 신호를 잰다. 실패한 항목은 None. 다른 OS에서는 모두 None(→ 보류)."""

    def __init__(self, *, psutil_module=None, clock=time.monotonic):
        self._windows = os.name == "nt"
        self._clock = clock
        self._cpu_prev = None
        self._disk_prev = None
        if psutil_module is None:
            try:
                import psutil as psutil_module  # 선택 의존성: 없으면 디스크 부하는 판정에서 뺀다
            except Exception:
                psutil_module = False
        self._psutil = psutil_module or None

    def sample(self):
        if not self._windows:
            return IdleSignals()
        return IdleSignals(input_idle_seconds=self._input_idle(), cpu_percent=self._cpu(),
                           disk_bytes_per_second=self._disk(), on_battery=self._battery(),
                           fullscreen=self._fullscreen())

    @staticmethod
    def _input_idle():
        try:
            info = _LASTINPUTINFO(ctypes.sizeof(_LASTINPUTINFO), 0)
            if not ctypes.windll.user32.GetLastInputInfo(ctypes.byref(info)):
                return None
            now = _kernel32().GetTickCount() & 0xFFFFFFFF
            return ((now - info.dwTime) & 0xFFFFFFFF) / 1000.0
        except Exception:
            return None

    def _cpu(self):
        try:
            idle, kernel, user = _FILETIME(), _FILETIME(), _FILETIME()
            if not _kernel32().GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user)):
                return None
            created, exited, p_kernel, p_user = _FILETIME(), _FILETIME(), _FILETIME(), _FILETIME()
            kernel32 = _kernel32()
            if not kernel32.GetProcessTimes(kernel32.GetCurrentProcess(), ctypes.byref(created), ctypes.byref(exited),
                                            ctypes.byref(p_kernel), ctypes.byref(p_user)):
                return None
            current = (idle.value(), kernel.value() + user.value(), p_kernel.value() + p_user.value())
        except Exception:
            return None
        previous, self._cpu_prev = self._cpu_prev, current
        if previous is None:
            return None
        return cpu_percent_from_times(previous, current)

    def _disk(self):
        if self._psutil is None:
            return None
        try:
            counters = self._psutil.disk_io_counters()
            total = counters.read_bytes + counters.write_bytes
            own = self._psutil.Process().io_counters()
            own_total = own.read_bytes + own.write_bytes
        except Exception:
            return None
        now = self._clock()
        previous, self._disk_prev = self._disk_prev, (now, total, own_total)
        if previous is None or now <= previous[0]:
            return None
        others = max(0, (total - previous[1]) - (own_total - previous[2]))
        return others / (now - previous[0])

    @staticmethod
    def _battery():
        try:
            status = _SYSTEM_POWER_STATUS()
            if not _kernel32().GetSystemPowerStatus(ctypes.byref(status)):
                return None
        except Exception:
            return None
        return {0: True, 1: False}.get(status.ACLineStatus)

    @staticmethod
    def _fullscreen():
        try:
            state = ctypes.c_int(0)
            if ctypes.windll.shell32.SHQueryUserNotificationState(ctypes.byref(state)) != 0:
                return None
            return state.value in _BUSY_NOTIFICATION_STATES
        except Exception:
            return None


def cpu_percent_from_times(previous, current):
    """(유휴, 커널+사용자, 이 프로세스) 누적 시간 두 개 → 이 프로세스를 뺀 CPU 사용률. 커널 시간에 유휴가 들어 있다."""
    d_idle, d_total, d_own = (current[i] - previous[i] for i in range(3))
    if d_total <= 0:
        return None
    busy = max(0, d_total - d_idle - d_own)
    return min(100.0, busy * 100.0 / d_total)


# ---------------------------------------------------------------- 낮은 우선순위
@contextlib.contextmanager
def background_priority():
    """지금 스레드를 Windows 배경 모드로(CPU·디스크 I/O·메모리 우선순위 낮음). 다른 OS·실패 시 아무것도 안 함.

    THREAD_MODE_BACKGROUND_BEGIN은 호출한 스레드에만 걸리므로 반드시 일하는 스레드 안에서 쓴다.
    """
    started = False
    if os.name == "nt":
        try:
            kernel32 = _kernel32()
            started = bool(kernel32.SetThreadPriority(kernel32.GetCurrentThread(), _THREAD_MODE_BACKGROUND_BEGIN))
        except Exception:
            started = False
    try:
        yield started
    finally:
        if started:
            try:
                kernel32 = _kernel32()
                kernel32.SetThreadPriority(kernel32.GetCurrentThread(), _THREAD_MODE_BACKGROUND_END)
            except Exception:
                pass


# ---------------------------------------------------------------- 상태를 들고 있는 문
class IdleGate:
    """신호를 재고 `decide`로 상태를 정해 들고 있는다. 여러 스레드에서 불러도 된다."""

    def __init__(self, probe=None, config=None, *, clock=time.monotonic, sample_interval=1.0):
        self._probe = probe if probe is not None else WindowsIdleProbe()
        self._config = config or IdleConfig()
        self._clock = clock
        self._sample_interval = sample_interval
        self._lock = threading.Lock()
        self._decision = None
        self._signals = IdleSignals()
        self._sampled_at = None
        self._since = clock()
        self._transitions = 0

    @property
    def config(self):
        with self._lock:
            return self._config

    def configure(self, *, enabled=None, idle_seconds=None):
        with self._lock:
            changes = {}
            if enabled is not None:
                changes["enabled"] = bool(enabled)
            if idle_seconds is not None:
                changes["idle_seconds"] = clamp_idle_seconds(idle_seconds)
            self._config = replace(self._config, **changes)
            self._sampled_at = None  # 다음 poll에서 곧바로 다시 판정
        return self.poll(force=True)

    def poll(self, *, force=False):
        with self._lock:
            now = self._clock()
            if (not force and self._decision is not None and self._sampled_at is not None
                    and now - self._sampled_at < self._sample_interval):
                return self._decision
        signals = self._probe.sample()  # 잠금 밖에서 잰다(API 호출이 느릴 수 있음)
        with self._lock:
            previous = self._decision.state if self._decision else None
            decision = decide(previous, signals, self._config)
            if self._decision is None or decision.state != self._decision.state:
                self._since = now
                self._transitions += 1
            self._decision, self._signals, self._sampled_at = decision, signals, now
            return decision

    def is_idle(self):
        return self.poll().state == "idle"

    def wait_until_idle(self, stop_event, *, bypass=None, poll_seconds=1.0):
        """쉬는 시간이 될 때까지 기다린다. stop_event가 서면 False, bypass가 서면 곧바로 True."""
        while True:
            if stop_event is not None and stop_event.is_set():
                return False
            if bypass is not None and bypass.is_set():
                return True
            if self.poll().state == "idle":
                return True
            waiter = stop_event if stop_event is not None else threading.Event()
            waiter.wait(poll_seconds)

    def snapshot(self):
        with self._lock:
            decision = self._decision or IdleDecision("held", "input_unknown")
            return {"state": decision.state, "reason": decision.reason,
                    "reason_text": REASON_TEXT.get(decision.reason, decision.reason),
                    "since_seconds": round(max(0.0, self._clock() - self._since), 1),
                    "enabled": self._config.enabled, "idle_seconds": self._config.idle_seconds,
                    "transitions": self._transitions, "signals": asdict(self._signals)}
