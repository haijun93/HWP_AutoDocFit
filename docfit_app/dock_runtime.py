"""Lifecycle and named-pipe protocol for the optional native menu dock."""
from __future__ import annotations

import json
import logging
import os
from pathlib import Path
import subprocess
import sys
import threading
import time


PIPE_NAME = r"\\.\pipe\HwpAutoDocFit.dock"
_TAB_EVENT_COUNT = 16


class _TabEventReceiver:
    """Temporary Python-to-Flutter bridge while the Python host remains in place.

    Rust Win32 host integration should replace this receiver when that host exists.
    """

    def __init__(self, dispatch_js):
        if os.name != "nt":
            raise OSError("Dock tab events are available only on Windows")
        import ctypes
        from ctypes import wintypes

        self._ctypes = ctypes
        self._kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self._dispatch_js = dispatch_js
        self._handles = []
        self._thread = None
        self._closed = False
        self._kernel32.CreateEventW.argtypes = (ctypes.c_void_p, wintypes.BOOL, wintypes.BOOL, wintypes.LPCWSTR)
        self._kernel32.CreateEventW.restype = wintypes.HANDLE
        self._kernel32.WaitForMultipleObjects.argtypes = (wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE), wintypes.BOOL, wintypes.DWORD)
        self._kernel32.WaitForMultipleObjects.restype = wintypes.DWORD
        self._kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        self._kernel32.CloseHandle.restype = wintypes.BOOL
        self._kernel32.SetEvent.argtypes = (wintypes.HANDLE,)
        self._kernel32.SetEvent.restype = wintypes.BOOL

    def start(self):
        # No explicit security descriptor means the current process token's default DACL;
        # unprefixed names are local to this Windows session.
        try:
            for kind in ("open", "close"):
                for tab in range(_TAB_EVENT_COUNT):
                    name = f"docfit-{kind}-tab-{tab}"
                    handle = self._kernel32.CreateEventW(None, False, False, name)
                    if not handle:
                        raise OSError(self._ctypes.get_last_error(), f"CreateEventW failed: {name}")
                    self._handles.append((handle, name))
            stop = self._kernel32.CreateEventW(None, True, False, None)
            if not stop:
                raise OSError(self._ctypes.get_last_error(), "CreateEventW failed for receiver stop event")
            self._stop_handle = stop
            self._handles.append((stop, None))
            self._thread = threading.Thread(target=self._run, name="dock-tab-event-receiver", daemon=True)
            self._thread.start()
        except Exception:
            self.stop()
            raise

    def _run(self):
        logger = logging.getLogger(__name__)
        handles = [h for h, _ in self._handles]
        array = (self._ctypes.c_void_p * len(handles))(*handles)
        while True:
            result = self._kernel32.WaitForMultipleObjects(len(handles), array, False, 0xFFFFFFFF)
            if result == 0xFFFFFFFF:
                logger.error("WaitForMultipleObjects failed: %s", self._ctypes.get_last_error())
                return
            index = result  # WAIT_OBJECT_0 + zero-based index.
            if index == len(handles) - 1:
                return
            if not 0 <= index < len(handles) - 1:
                logger.error("Unexpected dock event wait result: %#x", result)
                return
            name = self._handles[index][1]
            try:
                self._dispatch_js(f"window.dispatchEvent(new Event('{name}'));")
            except Exception:
                logger.exception("Dock tab event dispatch failed: %s", name)

    def stop(self):
        if self._closed:
            return
        self._closed = True
        stop = getattr(self, "_stop_handle", None)
        if stop:
            self._kernel32.SetEvent(stop)
        if self._thread is not None and self._thread is not threading.current_thread():
            self._thread.join(timeout=2)
            if self._thread.is_alive():
                logging.getLogger(__name__).error("Dock tab event receiver did not stop")
        for handle, _ in self._handles:
            self._kernel32.CloseHandle(handle)
        self._handles.clear()


def bundled_exe() -> Path | None:
    """Find dock.exe in the packaged resources or local Rust build output."""
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1]))
    candidates = (
        base / "resources" / "dock.exe",
        base / "dock" / "target" / "release" / "dock.exe",
    )
    return next((path for path in candidates if path.is_file()), None)


def _pipe_request(command: str, timeout: float) -> dict:
    """Send one newline-delimited JSON request using DOCK2's byte-stream protocol."""
    # Rust dock/src/native.rs reads one JSON value and replies with one JSON line.
    with open(PIPE_NAME, "r+b", buffering=0) as pipe:
        pipe.write(json.dumps({"cmd": command}, separators=(",", ":")).encode("utf-8") + b"\n")
        pipe.flush()
        pipe_read = getattr(pipe, "read1", pipe.read)
        deadline = time.monotonic() + timeout
        response = bytearray()
        while b"\n" not in response:
            if time.monotonic() >= deadline:
                raise TimeoutError("Dock 파이프 응답 시간 초과")
            chunk = pipe_read(2048)
            if not chunk:
                raise OSError("Dock 파이프 연결이 끊겼습니다")
            response.extend(chunk)
        return json.loads(bytes(response).split(b"\n", 1)[0].decode("utf-8"))


class DockRuntime:
    def __init__(self, *, executable=bundled_exe, process_start=subprocess.Popen,
                 pipe_request=_pipe_request, dispatch_js=None):
        self._executable = executable
        self._process_start = process_start
        self._pipe_request = pipe_request
        self._dispatch_js = dispatch_js
        self._process = None
        self._tab_events = None

    def start(self):
        if self._process is not None and self._process.poll() is None:
            return {"started": True}
        executable = self._executable()
        if executable is None:
            raise FileNotFoundError("resources 폴더에서 dock.exe를 찾을 수 없습니다")
        if self._dispatch_js is not None and self._tab_events is None:
            self._tab_events = _TabEventReceiver(self._dispatch_js)
            self._tab_events.start()
        flags = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        try:
            self._process = self._process_start(
                [str(executable)], shell=False, close_fds=True, creationflags=flags,
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
        except Exception:
            self._stop_tab_events()
            raise
        return {"started": True}

    def _stop_tab_events(self):
        receiver, self._tab_events = self._tab_events, None
        if receiver is not None:
            receiver.stop()

    def ready(self, timeout: float) -> bool:
        deadline = time.monotonic() + max(0.0, timeout)
        while True:
            remaining = deadline - time.monotonic()
            try:
                result = self._pipe_request("ready", max(0.01, remaining))
                if result.get("ok") is True and result.get("ready") is True:
                    return True
            except (OSError, TimeoutError, ValueError, json.JSONDecodeError):
                pass
            if remaining <= 0:
                return False
            time.sleep(min(0.1, remaining))

    def stop(self):
        try:
            self._pipe_request("quit", 1.0)
        except (OSError, TimeoutError, ValueError, json.JSONDecodeError):
            pass
        self._stop_tab_events()
        self._process = None
        return {"stopped": True}
