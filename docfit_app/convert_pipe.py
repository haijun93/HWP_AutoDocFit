"""수신 중인 앱의 몽땅파일변환 named pipe."""
from __future__ import annotations

import json
import logging
import os
import threading

PIPE_NAME = r"\\.\pipe\HwpAutoDocFit.convert"
MAX_REQUEST = 1_048_576
MAX_PATHS = 2000
_log = logging.getLogger(__name__)


def decode_request(data: bytes, add_paths):
    """JSON 요청을 검증하고 화면의 기존 경로 추가 경로로 넘긴다."""
    if len(data) > MAX_REQUEST:
        return {"ok": False, "error": "too_large"}
    try:
        request = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return {"ok": False, "error": "invalid_json"}
    paths = request.get("paths") if isinstance(request, dict) and request.get("cmd") == "convert_open" else None
    target = request.get("target") if isinstance(request, dict) else None
    from docfit_core.file_conversion import CONVERSION_OUTPUTS
    if target not in CONVERSION_OUTPUTS:
        return {"ok": False, "error": "invalid_target"}
    if not isinstance(paths, list) or not 1 <= len(paths) <= MAX_PATHS or not all(isinstance(p, str) for p in paths):
        return {"ok": False, "error": "invalid_paths"}
    from docfit_core.file_conversion import CONVERSION_INPUTS
    from pathlib import Path
    valid = []
    for value in paths:
        try:
            p = Path(value)
            if p.suffix.lower() in CONVERSION_INPUTS and p.is_file() and not p.is_symlink():
                valid.append(str(p.resolve(strict=True)))
        except (OSError, ValueError):
            continue
    if not valid:
        return {"ok": False, "error": "no_supported_files"}
    add_paths(valid, target)
    return {"ok": True, "added": len(valid)}


class ConvertPipeReceiver:
    def __init__(self, add_paths, *, name=PIPE_NAME, server_factory=None):
        self.name, self.add_paths = name, add_paths
        self.server_factory = server_factory
        self._stop = threading.Event()
        self._thread = None
        self._handle = None

    def start(self):
        if os.name != "nt" and self.server_factory is None:
            raise OSError("몽땅파일변환 파이프는 Windows에서만 사용할 수 있습니다")
        self._thread = threading.Thread(target=self._serve, name="convert-pipe-receiver", daemon=True)
        self._thread.start()
        return self

    def _serve(self):
        if self.server_factory is not None:
            while not self._stop.is_set():
                client = self.server_factory(self._stop)
                if client is None:
                    continue
                try:
                    self._handle_request(client)
                finally:
                    client.close()
            return
        import ctypes
        from ctypes import wintypes
        k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        k32.CreateNamedPipeW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                                         wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p]
        k32.CreateNamedPipeW.restype = wintypes.HANDLE
        k32.ConnectNamedPipe.argtypes = [wintypes.HANDLE, ctypes.c_void_p]
        k32.ReadFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
        k32.WriteFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
        k32.DisconnectNamedPipe.argtypes = [wintypes.HANDLE]
        k32.CloseHandle.argtypes = [wintypes.HANDLE]
        invalid = wintypes.HANDLE(-1).value
        while not self._stop.is_set():
            handle = k32.CreateNamedPipeW(self.name, 0x3, 0x8, 1, 4096, MAX_REQUEST, 0, None)
            if not handle or handle == invalid:
                _log.error("CreateNamedPipeW failed: %s", ctypes.get_last_error()); self._stop.wait(.2); continue
            self._handle = handle
            try:
                connected = k32.ConnectNamedPipe(handle, None) or ctypes.get_last_error() == 535
                if connected and not self._stop.is_set():
                    class Client:
                        def read(_self):
                            buf = ctypes.create_string_buffer(MAX_REQUEST + 1); size = wintypes.DWORD()
                            if not k32.ReadFile(handle, buf, MAX_REQUEST + 1, ctypes.byref(size), None): return b""
                            return buf.raw[:size.value]
                        def write(_self, payload):
                            size = wintypes.DWORD(); k32.WriteFile(handle, payload, len(payload), ctypes.byref(size), None)
                        def close(_self): pass
                    self._handle_request(Client())
            finally:
                k32.DisconnectNamedPipe(handle); k32.CloseHandle(handle); self._handle = None

    def _handle_request(self, client):
        try:
            raw = client.read()
            response = decode_request(raw.split(b"\n", 1)[0], self.add_paths)
        except Exception as exc:
            _log.exception("몽땅파일변환 파이프 요청을 처리하지 못했습니다")
            response = {"ok": False, "error": type(exc).__name__}
        try:
            client.write(json.dumps(response, ensure_ascii=False).encode("utf-8") + b"\n")
        except OSError:
            _log.debug("몽땅파일변환 파이프 응답을 쓰지 못했습니다", exc_info=True)

    def stop(self, timeout=2.0):
        self._stop.set()
        if os.name == "nt":
            try:  # ConnectNamedPipe를 깨워 종료 플래그를 확인하게 한다.
                with open(self.name, "r+b", buffering=0):
                    pass
            except OSError:
                pass
        if self._thread:
            self._thread.join(timeout)
        return not self._thread or not self._thread.is_alive()
