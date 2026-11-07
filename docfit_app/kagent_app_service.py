"""K에이전트(kagent.exe) → 앱 서비스 파이프: 몽땅클립보드가 앱에 맡기는 작은 일(지금은 엑셀 표 '목표 결과' 변환).

빠른 목록이나 관리창에서 엑셀 표 항목을 고르면 에이전트가 저장해 둔 클립보드 HTML을 이 파이프로 보내고,
앱은 `docfit_core.excel_paste_target`(순수 Python, 한/글 COM 불필요)으로 바꿔 돌려준다. 에이전트는 앱이
꺼져 있거나 응답이 늦으면 원래 형식 그대로 붙인다(폴백).

- 파이프 이름: ``\\\\.\\pipe\\HwpAutoDocFit.appsvc.<사용자 SID>`` (현재 사용자·SYSTEM만 열 수 있음, 원격 거부).
- 한 줄 JSON 요청 → 한 줄 JSON 응답. HTML은 바이트 그대로 오가도록 base64로 싣는다.
- 요청: ``{"cmd":"xl_target","html":"<base64 CF_HTML>","text":"<탭 구분 글>"}`` 또는 ``{"cmd":"ping"}``.
- 응답: ``{"ok":true,"changed":true,"html":"<base64>","text":"..."}`` / ``{"ok":true,"changed":false,"reason":"..."}``.
"""
from __future__ import annotations

import base64
import binascii
import json
import logging
import os
import threading

from docfit_core.excel_paste_target import clean_tsv, to_target_html_report

PIPE_PREFIX = "HwpAutoDocFit.appsvc"
PROTOCOL = 1
# 에이전트의 항목 하나 최대 크기(50MB) + base64 여유.
MAX_REQUEST = 72 * 1024 * 1024

_log = logging.getLogger(__name__)


def _is_excel_html(raw: bytes) -> bool:
    low = raw[:65536].lower()
    return (b"urn:schemas-microsoft-com:office:excel" in low or b"excel.sheet" in low) and b"<td" in raw.lower()


def handle(request) -> dict:
    """요청 하나를 처리한다(입출력 없음, 어느 OS에서나 시험 가능). 실패는 항상 ok=False 응답으로."""
    if not isinstance(request, dict):
        return dict(ok=False, message="요청 형식을 확인해 주세요.")
    cmd = request.get("cmd")
    if cmd == "ping":
        return dict(ok=True, protocol=PROTOCOL)
    if cmd != "xl_target":
        return dict(ok=False, message="지원하지 않는 명령입니다.")
    try:
        raw = base64.b64decode(str(request.get("html") or ""), validate=True)
    except (binascii.Error, ValueError):
        return dict(ok=False, message="HTML을 읽지 못했습니다.")
    raw = raw.rstrip(b"\0")  # 클립보드 메모리 끝의 NUL은 형식 밖 값이다(다시 붙일 때 에이전트가 하나만 붙인다).
    if not raw:
        return dict(ok=True, changed=False, reason="HTML 없음")
    if not _is_excel_html(raw):
        return dict(ok=True, changed=False, reason="엑셀 표 HTML이 아님")
    shading = "strip" if request.get("keep_cell_background") is False else "keep"
    html, report = to_target_html_report(raw, shading=shading)
    if not report.get("changed"):
        return dict(ok=True, changed=False, reason=str(report.get("reason") or "바꿀 것 없음"))
    text = request.get("text")
    text = clean_tsv(text) if isinstance(text, str) else ""
    return dict(ok=True, changed=True, html=base64.b64encode(html).decode("ascii"), text=text,
                report={k: report[k] for k in ("numeric", "blank", "width", "padding") if k in report})


def handle_line(line: bytes) -> bytes:
    try:
        request = json.loads(line.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        response = dict(ok=False, message="요청을 읽지 못했습니다.")
    else:
        try:
            response = handle(request)
        except Exception as exc:  # 서비스는 어떤 요청에도 죽지 않는다(에이전트는 폴백한다).
            _log.exception("kagent 앱 서비스 요청 처리 실패")
            response = dict(ok=False, message=f"처리하지 못했습니다: {type(exc).__name__}")
    return json.dumps(response, ensure_ascii=False).encode("utf-8") + b"\n"


# ---- Windows 이름 있는 파이프 서버 -------------------------------------------------------------

def pipe_name(sid=None, suffix=""):
    if sid is None:
        sid = _current_sid()
    return rf"\\.\pipe\{PIPE_PREFIX}{suffix}.{sid}"


def _current_sid():
    from .library_bridge import _current_user_pipe_name
    # 자료함 파이프 이름과 같은 SID 계산을 쓴다(`...klib[.test.x].<SID>`의 마지막 마디, SID에는 점이 없다).
    return _current_user_pipe_name().rsplit(".", 1)[-1]


class AppServicePipe:
    """앱이 켜져 있는 동안 에이전트 요청을 받는 단일 인스턴스 파이프(요청은 차례로 처리)."""

    def __init__(self, name=None, handler=handle_line):
        self.name = name or pipe_name()
        self.handler = handler
        self._stop = threading.Event()
        self._thread = None
        self.ready = threading.Event()
        self.failed = None

    def start(self):
        if os.name != "nt":
            raise OSError("앱 서비스 파이프는 Windows에서만 쓸 수 있습니다.")
        self._thread = threading.Thread(target=self._serve, name="kagent-app-service", daemon=True)
        self._thread.start()
        return self

    def stop(self, timeout=2.0):
        self._stop.set()
        try:  # 연결을 기다리는 ConnectNamedPipe를 깨운다.
            with open(self.name, "r+b", buffering=0) as stream:
                stream.write(b'{"cmd":"ping"}\n')
                stream.read(256)
        except OSError:
            pass
        if self._thread:
            self._thread.join(timeout)

    def _serve(self):
        import ctypes
        from ctypes import wintypes
        k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        adv = ctypes.WinDLL("advapi32", use_last_error=True)

        class SECURITY_ATTRIBUTES(ctypes.Structure):
            _fields_ = [("nLength", wintypes.DWORD), ("lpSecurityDescriptor", ctypes.c_void_p),
                        ("bInheritHandle", wintypes.BOOL)]

        k32.CreateNamedPipeW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                                         wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                                         ctypes.POINTER(SECURITY_ATTRIBUTES)]
        k32.CreateNamedPipeW.restype = wintypes.HANDLE
        k32.ConnectNamedPipe.argtypes = [wintypes.HANDLE, ctypes.c_void_p]
        k32.ConnectNamedPipe.restype = wintypes.BOOL
        k32.DisconnectNamedPipe.argtypes = [wintypes.HANDLE]
        k32.CloseHandle.argtypes = [wintypes.HANDLE]
        k32.FlushFileBuffers.argtypes = [wintypes.HANDLE]
        k32.ReadFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                                 ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
        k32.ReadFile.restype = wintypes.BOOL
        k32.WriteFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                                  ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
        k32.WriteFile.restype = wintypes.BOOL
        k32.LocalFree.argtypes = [ctypes.c_void_p]
        adv.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [
            wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p]
        adv.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype = wintypes.BOOL

        sid = self.name.rsplit(".", 1)[-1]
        sd = ctypes.c_void_p()
        # 이 사용자와 SYSTEM만 접근(kagent 파이프와 같은 규칙).
        if not adv.ConvertStringSecurityDescriptorToSecurityDescriptorW(f"D:P(A;;GA;;;{sid})(A;;GA;;;SY)", 1,
                                                                        ctypes.byref(sd), None):
            self.failed = f"보안 설명자 오류 {ctypes.get_last_error()}"
            self.ready.set()
            return
        sa = SECURITY_ATTRIBUTES(ctypes.sizeof(SECURITY_ATTRIBUTES), sd, False)
        invalid = wintypes.HANDLE(-1).value

        def create(first):
            # PIPE_ACCESS_DUPLEX | (처음만) FILE_FLAG_FIRST_PIPE_INSTANCE — 같은 이름을 다른 프로세스가 먼저 잡았으면 포기.
            # PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT(모두 0) | PIPE_REJECT_REMOTE_CLIENTS(8).
            # 인스턴스 2개: 하나를 처리하는 동안 다음 인스턴스가 기다리고 있어 에이전트가 '파이프 없음'(=앱 꺼짐)으로 보지 않는다.
            h = k32.CreateNamedPipeW(self.name, 0x3 | (0x00080000 if first else 0), 0x8, 2, 1 << 16, 1 << 16, 0,
                                     ctypes.byref(sa))
            return None if not h or h == invalid else h

        listening = create(True)
        if listening is None:
            self.failed = f"파이프를 만들지 못했습니다(오류 {ctypes.get_last_error()})"
            self.ready.set()
            k32.LocalFree(sd)
            return
        self.ready.set()
        try:
            while not self._stop.is_set():
                h, listening = listening, None
                try:
                    connected = k32.ConnectNamedPipe(h, None) or ctypes.get_last_error() == 535  # PIPE_CONNECTED
                    while listening is None and not self._stop.is_set():
                        listening = create(False)
                        if listening is None:
                            self._stop.wait(0.2)
                    if connected and not self._stop.is_set():
                        line = self._read_line(k32, h, ctypes, wintypes)
                        if line is not None:
                            self._write_all(k32, h, self.handler(line), ctypes, wintypes)
                            k32.FlushFileBuffers(h)
                    k32.DisconnectNamedPipe(h)
                finally:
                    k32.CloseHandle(h)
        finally:
            if listening is not None:
                k32.CloseHandle(listening)
            k32.LocalFree(sd)

    @staticmethod
    def _read_line(k32, h, ctypes, wintypes):
        data = bytearray()
        buf = ctypes.create_string_buffer(1 << 16)
        n = wintypes.DWORD()
        while True:
            if not k32.ReadFile(h, buf, len(buf), ctypes.byref(n), None) and ctypes.get_last_error() != 234:
                break  # 234 = ERROR_MORE_DATA
            if n.value == 0:
                break
            data += buf.raw[:n.value]
            if b"\n" in data[-n.value:] or len(data) > MAX_REQUEST:
                break
        if len(data) > MAX_REQUEST or not data:
            return None
        return bytes(data.split(b"\n", 1)[0])

    @staticmethod
    def _write_all(k32, h, data, ctypes, wintypes):
        view = memoryview(data)
        n = wintypes.DWORD()
        while view:
            chunk = bytes(view[:1 << 20])
            if not k32.WriteFile(h, chunk, len(chunk), ctypes.byref(n), None) or n.value == 0:
                return
            view = view[n.value:]


_server = None
_server_lock = threading.Lock()


def start_background():
    """앱 시작 때 한 번. 실패해도 앱은 그대로 돌고 에이전트는 원래 형식으로 붙인다."""
    global _server
    with _server_lock:
        if _server is not None or os.name != "nt":
            return _server
        try:
            _server = AppServicePipe().start()
        except Exception:
            _log.exception("kagent 앱 서비스 파이프를 시작하지 못했습니다")
            _server = None
        return _server
