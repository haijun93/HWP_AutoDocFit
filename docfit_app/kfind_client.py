"""KFS1 파이프 경계. 검색 정책·legacy 대체는 호출자가 맡는다.

연결 전이: none→connecting→connected|disconnected|refused,
connected→disconnected|refused, disconnected|refused→retry→connecting.
모든 상태→none(close). 그 밖의 전이는 금지한다. 재시도는 요청이 있을 때만
첫 5초, 이후 30초 간격이다. 기본 활성화하며 판 확인 전 검색은 보내지 않는다.
"""
from __future__ import annotations

import json
import math
import os
import struct
import threading
import time

PIPE_NAME = r"\\.\pipe\HwpAutoDocFit.Index"
MAX_REQUEST = 65536
MAX_RESPONSE = 64 << 20
TRANSITIONS = {
    "none": {"connecting", "none"},
    "connecting": {"connected", "disconnected", "refused", "none"},
    "connected": {"disconnected", "refused", "none"},
    "disconnected": {"retry", "none"},
    "refused": {"retry", "none"},
    "retry": {"connecting", "none"},
}


class KfindError(RuntimeError):
    pass


class KfindTimeout(KfindError):
    pass


def _encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"),
                      allow_nan=False).encode("utf-8")


def _read(pipe, size):
    chunks = bytearray()
    while len(chunks) < size:
        chunk = pipe.read(size - len(chunks))
        if not chunk:
            raise OSError("파이프가 닫혔습니다")
        chunks.extend(chunk)
    return bytes(chunks)


def _open_pipe(name):
    # 서비스는 연결 하나마다 요청 하나를 받는다. 다음 인스턴스 생성 중의
    # ERROR_PIPE_BUSY는 연결 실패가 아니며 바깥 요청 시간 한도 안에서 기다린다.
    deadline = time.monotonic() + 2.0
    while True:
        try:
            return open(name, "r+b", buffering=0)
        except OSError as exc:
            if getattr(exc, "winerror", None) != 231:
                raise
            if time.monotonic() >= deadline:
                raise KfindTimeout("파이프 연결 시간 초과") from exc
            time.sleep(0.005)


class KfindClient:
    def __init__(self, pipe_name=PIPE_NAME, *, opener=None, clock=None,
                 timeout=2.0, versions=None):
        self.pipe_name = pipe_name
        self._opener = opener or _open_pipe
        self._clock = clock or time.monotonic
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("시간 한도는 유한한 양수여야 합니다")
        self.timeout = timeout
        self.versions = versions or {"format": 3, "fold_rule": 1, "sentence_rule": 1}
        self._workers = threading.BoundedSemaphore(16)
        self._lock = threading.RLock()
        self._connect_lock = threading.Lock()
        self._phase = "none"
        self._reason = None
        self._last_error = None
        self._generation = 0
        self._next_retry = 0
        self._failures = 0
        self._epoch = 0

    def _transition(self, phase):
        if phase not in TRANSITIONS[self._phase]:
            raise ValueError(f"불가능한 연결 전이: {self._phase} → {phase}")
        self._phase = phase

    def state(self):
        with self._lock:
            return dict(state=self._phase, reason=self._reason,
                        last_error=self._last_error, generation=self._generation,
                        retry_at=self._next_retry)

    def close(self):
        with self._lock:
            self._epoch += 1
            self._transition("none")
            self._reason = self._last_error = None
            self._failures = 0
            self._generation = self._next_retry = 0

    def _failed(self, exc, epoch):
        with self._lock:
            if epoch != self._epoch:
                return
            reason = ("denied" if isinstance(exc, PermissionError) else
                      "not_installed" if isinstance(exc, FileNotFoundError) else
                      "timeout" if isinstance(exc, KfindTimeout) else
                      "version_mismatch" if str(exc) == "version_mismatch" else
                      "stopped")
            phase = "refused" if reason in {"denied", "version_mismatch"} else "disconnected"
            if self._phase in {"connecting", "connected"}:
                self._transition(phase)
                self._next_retry = self._clock() + (5 if self._failures == 0 else 30)
                self._failures += 1
                self._reason, self._last_error = reason, str(exc)

    def _exchange(self, request, deadline):
        if self._clock() >= deadline:
            raise KfindTimeout("kfind 요청 시간 초과")
        data = _encode(request)
        if len(data) > MAX_REQUEST:
            raise ValueError("요청이 64KB를 넘습니다")
        done = threading.Event()
        abandoned = threading.Event()
        result = []
        if not self._workers.acquire(blocking=False):
            raise KfindError("진행 중인 파이프 요청이 너무 많습니다")

        def run():
            try:
                with self._opener(self.pipe_name) as pipe:
                    if abandoned.is_set():
                        return
                    frame = struct.pack("<I", len(data)) + data
                    while frame:
                        written = pipe.write(frame)
                        if not written:
                            raise OSError("파이프 쓰기 실패")
                        frame = frame[written:]
                    pipe.flush()
                    size = struct.unpack("<I", _read(pipe, 4))[0]
                    if not 0 < size <= MAX_RESPONSE:
                        raise KfindError("응답 크기 오류")
                    value = json.loads(_read(pipe, size).decode("utf-8"))
                    if not isinstance(value, dict) or value.get("ok") is not True:
                        raise KfindError(str(value.get("error", "응답 형식 오류"))
                                         if isinstance(value, dict) else "응답 형식 오류")
                    if not isinstance(value.get("result"), dict):
                        raise KfindError("응답 result 오류")
                    result.append(value["result"])
            except Exception as exc:
                result.append(exc)
            finally:
                self._workers.release()
                done.set()

        worker = threading.Thread(target=run, daemon=True, name="kfind-pipe")
        worker.start()
        if not done.wait(max(0, deadline - self._clock())):
            abandoned.set()
            if os.name == "nt":
                # 동기 ReadFile/Open 대기를 끊고 핸들 정리는 worker의 with가 맡는다.
                import ctypes
                from ctypes import wintypes
                kernel = ctypes.WinDLL("kernel32", use_last_error=True)
                kernel.OpenThread.restype = wintypes.HANDLE
                kernel.OpenThread.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
                kernel.CancelSynchronousIo.argtypes = [wintypes.HANDLE]
                kernel.CloseHandle.argtypes = [wintypes.HANDLE]
                handle = kernel.OpenThread(1, False, worker.native_id)
                if handle:
                    try:
                        kernel.CancelSynchronousIo(handle)
                    finally:
                        kernel.CloseHandle(handle)
            raise KfindTimeout("kfind 응답 시간 초과")
        if isinstance(result[0], Exception):
            raise result[0]
        return result[0]

    def connect(self, *, _deadline=None):
        deadline = _deadline if _deadline is not None else self._clock() + self.timeout
        if not self._connect_lock.acquire(timeout=max(0, deadline - self._clock())):
            raise KfindTimeout("연결 대기 시간 초과")
        try:
            with self._lock:
                if self._phase == "connected":
                    return True
                if self._phase in {"disconnected", "refused"}:
                    if self._clock() < self._next_retry:
                        raise KfindError(self._reason)
                    self._transition("retry")
                self._transition("connecting")
                self._epoch += 1
                epoch = self._epoch
            try:
                status = self._exchange({"command": "status"}, deadline)
                if any(status.get(k) != v for k, v in self.versions.items()):
                    raise KfindError("version_mismatch")
                if status.get("state") != "ready":
                    raise KfindError(status.get("state", "서비스 상태 오류"))
                with self._lock:
                    if epoch != self._epoch:
                        raise KfindError("연결이 닫혔습니다")
                    self._generation = status["generation"]
                    self._transition("connected")
                    self._reason = self._last_error = None
                    self._failures = 0
                return True
            except Exception as exc:
                self._failed(exc, epoch)
                if isinstance(exc, KfindTimeout):
                    raise
                raise KfindError(str(exc)) from exc
        finally:
            self._connect_lock.release()

    def _request(self, command, **args):
        request = dict(args, command=command)
        if len(_encode(request)) > MAX_REQUEST:
            raise ValueError("요청이 64KB를 넘습니다")
        deadline = self._clock() + self.timeout
        self.connect(_deadline=deadline)
        with self._lock:
            if self._phase != "connected":
                raise KfindError("연결이 닫혔습니다")
            epoch = self._epoch
        try:
            response = self._exchange(request, deadline)
            if "id" in args and response.get("id") != args["id"]:
                raise KfindError("응답 요청 번호 불일치")
            with self._lock:
                if epoch != self._epoch or self._phase != "connected":
                    raise KfindError("연결이 닫혔습니다")
                self._generation = response.get("generation", self._generation)
            return response
        except Exception as exc:
            self._failed(exc, epoch)
            if isinstance(exc, KfindError):
                raise
            raise KfindError(str(exc)) from exc

    def query(self, q, scope=None, *, top=20, refs=3, filters=None,
              request_id, sources=None, split=None):
        return self._request("query", id=request_id, text=q, scope=scope, top=top,
                             refs=refs, filters=filters or {}, sources=sources, split=split)

    def count(self, q, scope=None, *, filters=None, request_id, sources=None, split=None):
        return self._request("count", id=request_id, text=q, scope=scope,
                             filters=filters or {}, sources=sources, split=split)

    def cancel(self, request_id):
        # 별도 연결이므로 질의 응답을 기다리는 잠금을 잡지 않는다.
        return self._request("cancel", id=request_id)

    def status(self):
        response = self._request("status")
        with self._lock:
            epoch = self._epoch
            if any(response.get(k) != v for k, v in self.versions.items()):
                exc = KfindError("version_mismatch")
            elif response.get("state") != "ready":
                exc = KfindError(response.get("state", "서비스 상태 오류"))
            else:
                return response
            self._failed(exc, epoch)
        raise exc

    def remove(self, keys):
        return self._request("remove", doc_keys=list(keys))

    def stale(self, scope, level_rule, *, after=None, limit=1000):
        return self._request("stale", scope=scope, level_rule=level_rule, after=after, limit=limit)

    def ingest(self, doc):
        header = {k: v for k, v in doc.items() if k != "paras"}
        # 가장 긴 part/parts 머리로 계산해 마지막 번호 확정 뒤에도 64KB를 지킨다.
        def size(paras):
            return len(_encode(dict(header, command="ingest", paras=paras,
                                    part=4095, parts=4096)))
        if size([]) >= MAX_REQUEST:
            raise ValueError("문서 머리가 너무 큽니다")
        groups, group = [], []
        total = 0
        for para in doc["paras"]:
            text = para["t"]
            total += len(text.encode("utf-8"))
            if total > 64 << 20:
                raise ValueError("문서가 64MB를 넘습니다")
            offset = 0
            while True:
                remaining = len(text) - offset
                # UTF-8은 글자당 적어도 1바이트. 긴 나머지를 매번 직렬화하거나
                # 복사하지 않고 프레임 하나에 들어갈 범위만 계산한다.
                if remaining <= MAX_REQUEST and size(group + [dict(para, t=text[offset:])]) <= MAX_REQUEST:
                    group.append(dict(para, t=text[offset:]))
                    break
                if group:
                    groups.append(group)
                    group = []
                    continue
                lo, hi = 0, min(remaining, MAX_REQUEST)
                while lo < hi:
                    mid = (lo + hi + 1) // 2
                    if size([dict(para, t=text[offset:offset + mid])]) <= MAX_REQUEST:
                        lo = mid
                    else:
                        hi = mid - 1
                if not lo:
                    raise ValueError("문단 머리가 너무 큽니다")
                groups.append([dict(para, t=text[offset:offset + lo])])
                offset += lo
        groups.append(group)
        if len(groups) > 4096:
            raise ValueError("문서 조각이 4096개를 넘습니다")
        for part, paras in enumerate(groups):
            response = self._request("ingest", **dict(header, paras=paras,
                                                       part=part, parts=len(groups)))
        return response
