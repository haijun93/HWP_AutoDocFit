"""Lifecycle and named-pipe protocol for the optional native menu dock."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import time


PIPE_NAME = r"\\.\pipe\HwpAutoDocFit.dock"


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
                 pipe_request=_pipe_request):
        self._executable = executable
        self._process_start = process_start
        self._pipe_request = pipe_request
        self._process = None

    def start(self):
        if self._process is not None and self._process.poll() is None:
            return {"started": True}
        executable = self._executable()
        if executable is None:
            raise FileNotFoundError("resources 폴더에서 dock.exe를 찾을 수 없습니다")
        flags = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        self._process = self._process_start(
            [str(executable)], shell=False, close_fds=True, creationflags=flags,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        return {"started": True}

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
        self._process = None
        return {"stopped": True}
