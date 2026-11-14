"""Opt-in, low-overhead freeze snapshots for the desktop host."""
from __future__ import annotations

from collections import deque
from datetime import datetime
import json
import os
from pathlib import Path
import sys
import threading
import time
import traceback


class FreezeTracer:
    def __init__(self, log_dir=None, *, interval=1.0, threshold=3.0, keep=20,
                 hung_checker=None, clock=time.monotonic):
        self.log_dir = Path(log_dir) if log_dir is not None else (
            Path(os.environ.get('LOCALAPPDATA', Path.home())) /
            'HwpAutoDocFit' / 'freeze-logs')
        self.interval = interval
        self.threshold = threshold
        self.keep = keep
        self.hung_checker = hung_checker
        self.clock = clock
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread = None
        self._heartbeat = clock()
        self._tk_heartbeat = clock()
        self._tk_pending = {}
        self._api = deque(maxlen=20)
        # _tk 왕복 계측(Tk 제거 0단계): 최근 왕복과 호출 이름별 누계. 이름 수는 호출 지점 수로 한정된다.
        self._tk_calls = deque(maxlen=50)
        self._tk_stats = {}
        self._recorded = set()
        self._window = None

    @property
    def running(self):
        return self._thread is not None and self._thread.is_alive()

    def start(self, window=None):
        with self._lock:
            if self.running:
                return False
            self._window = window
            self._heartbeat = self.clock()
            self._tk_heartbeat = self._heartbeat
            self._stop.clear()
            self._thread = threading.Thread(target=self._watch, name='freeze-tracer', daemon=True)
            self._thread.start()
            return True

    def stop(self, timeout=2):
        self._stop.set()
        thread = self._thread
        if thread and thread is not threading.current_thread():
            thread.join(timeout)
        if thread is None or not thread.is_alive():
            self._thread = None
        try:
            if self.write_tk_summary() is not None:
                with self._lock:   # 같은 통계를 두 번 남기지 않는다.
                    self._tk_calls.clear()
                    self._tk_stats.clear()
        except Exception:
            # Diagnostics must never terminate or impede the host.
            pass

    def heartbeat(self):
        self._heartbeat = self.clock()

    def tk_heartbeat(self):
        self._tk_heartbeat = self.clock()

    def begin_tk(self, name):
        token = object()
        with self._lock:
            self._tk_pending[token] = {'name': str(name)[:120], 'started': self.clock()}
        return token

    def end_tk(self, token):
        with self._lock:
            self._tk_pending.pop(token, None)
        self.heartbeat()

    def record_api(self, name, elapsed, outcome):
        with self._lock:
            self._api.append({'name': str(name)[:120], 'duration_seconds': round(elapsed, 3),
                              'outcome': str(outcome)[:40], 'time': datetime.now().astimezone().isoformat()})

    def record_tk(self, name, queued, ran, total, outcome):
        """_tk 왕복 한 번: 호출 이름, 큐 대기·실행·전체 시간(초), 결과. 실행되지 않았으면 ran은 None."""
        name = str(name)[:120]
        entry = {'name': name, 'queue_seconds': round(max(0.0, queued), 3),
                 'run_seconds': None if ran is None else round(max(0.0, ran), 3),
                 'total_seconds': round(max(0.0, total), 3), 'outcome': str(outcome)[:40],
                 'time': datetime.now().astimezone().isoformat()}
        with self._lock:
            self._tk_calls.append(entry)
            stat = self._tk_stats.get(name)
            if stat is None:
                if len(self._tk_stats) >= 300:
                    name = '(other)'
                stat = self._tk_stats.setdefault(name, {
                    'count': 0, 'failed': 0, 'total_seconds': 0.0, 'queue_seconds': 0.0,
                    'max_seconds': 0.0, 'max_queue_seconds': 0.0})
            stat['count'] += 1
            if outcome != 'ok':
                stat['failed'] += 1
            stat['total_seconds'] += max(0.0, total)
            stat['queue_seconds'] += max(0.0, queued)
            stat['max_seconds'] = max(stat['max_seconds'], total)
            stat['max_queue_seconds'] = max(stat['max_queue_seconds'], queued)

    def tk_summary(self):
        """호출 이름별 왕복 통계(전체 시간이 큰 순)."""
        with self._lock:
            rows = [dict(stat, name=name) for name, stat in self._tk_stats.items()]
        for row in rows:
            for key in ('total_seconds', 'queue_seconds', 'max_seconds', 'max_queue_seconds'):
                row[key] = round(row[key], 3)
        return sorted(rows, key=lambda row: row['total_seconds'], reverse=True)

    def write_tk_summary(self):
        """기록한 왕복이 있으면 이름별 통계를 파일로 남긴다(멈춤이 없어도 지연 지점을 볼 수 있게)."""
        calls = self.tk_summary()
        if not calls:
            return None
        with self._lock:
            recent = list(self._tk_calls)
        self.log_dir.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now().astimezone().strftime('%Y%m%d-%H%M%S-%f')
        path = self.log_dir / f'tk-roundtrips-{stamp}.json'
        payload = {'timestamp': datetime.now().astimezone().isoformat(),
                   'round_trips': sum(row['count'] for row in calls),
                   'total_seconds': round(sum(row['total_seconds'] for row in calls), 3),
                   'calls': calls, 'recent': recent}
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding='utf-8')
        self._prune('tk-roundtrips-*.json')
        return path

    def check_now(self):
        now = self.clock()
        with self._lock:
            pending = list(self._tk_pending.values())
        tk_stuck = [p for p in pending if now - p['started'] >= self.threshold]
        hung = False
        checker = self.hung_checker
        if checker is not None:
            try:
                hung = bool(checker(self._window))
            except Exception:
                hung = False
        frozen = (now - self._heartbeat >= self.threshold or
                  now - self._tk_heartbeat >= self.threshold or bool(tk_stuck) or hung)
        if not frozen:
            self._recorded.clear()
            return None
        causes = []
        if now - self._heartbeat >= self.threshold:
            causes.append('main_heartbeat')
        if now - self._tk_heartbeat >= self.threshold:
            causes.append('tk_heartbeat')
        causes.extend('tk:' + p['name'] for p in tk_stuck)
        if hung:
            causes.append('IsHungAppWindow')
        signature = tuple(sorted(set(causes)))
        if signature in self._recorded:
            return None
        self._recorded.add(signature)
        return self._write_snapshot(signature, pending)

    def _watch(self):
        while not self._stop.wait(self.interval):
            try:
                self.check_now()
            except Exception:
                # Diagnostics must never terminate or impede the host.
                continue

    def _write_snapshot(self, causes, pending):
        self.log_dir.mkdir(parents=True, exist_ok=True)
        frames = sys._current_frames()
        stacks = {}
        for thread in threading.enumerate():
            frame = frames.get(thread.ident)
            if frame is not None:
                stacks[f'{thread.name} (ident={thread.ident})'] = ''.join(traceback.format_stack(frame))
        with self._lock:
            api = list(self._api)
            tk_calls = list(self._tk_calls)
        payload = {
            'timestamp': datetime.now().astimezone().isoformat(),
            'causes': list(causes),
            'memory': {'rss_bytes': _rss_bytes(), 'python_threads': threading.active_count()},
            'tk_pending': [{'name': p['name'], 'wait_seconds': round(max(0, self.clock()-p['started']), 3)}
                           for p in pending],
            'recent_api_requests': api,
            'recent_tk_roundtrips': tk_calls,
            'tk_roundtrip_stats': self.tk_summary(),
            'threads': stacks,
        }
        stamp = datetime.now().astimezone().strftime('%Y%m%d-%H%M%S-%f')
        path = self.log_dir / f'freeze-{stamp}.json'
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding='utf-8')
        self._prune()
        return path

    def _prune(self, pattern='freeze-*.json'):
        files = sorted(self.log_dir.glob(pattern), key=lambda p: p.stat().st_mtime, reverse=True)
        for old in files[self.keep:]:
            try:
                old.unlink()
            except OSError:
                pass

    def open_folder(self):
        self.log_dir.mkdir(parents=True, exist_ok=True)
        if os.name == 'nt':
            os.startfile(str(self.log_dir))
        return str(self.log_dir)


def _rss_bytes():
    try:
        import ctypes
        from ctypes import wintypes
        class Counters(ctypes.Structure):
            _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD),
                        ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                        ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                        ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                        ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]
        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        ctypes.WinDLL('psapi').GetProcessMemoryInfo(ctypes.windll.kernel32.GetCurrentProcess(),
                                                    ctypes.byref(counters), counters.cb)
        return int(counters.WorkingSetSize)
    except Exception:
        try:
            import resource
            return int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024)
        except Exception:
            return None


def _is_hung(window):
    if window is None:
        return False
    try:
        import ctypes
        handle = getattr(window, 'hwnd', window)
        return bool(ctypes.windll.user32.IsHungAppWindow(int(handle)))
    except Exception:
        return False
