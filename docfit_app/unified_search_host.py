"""앱 안 몽땅검색 통합 보기: ``ktm.exe --search-host``(JSONL 표준 입출력) 연결.

검색·단 배정·0건 안내는 호스트(Rust)가 한다. 여기서는 프로세스를 띄우고, 가장 최근 응답만
보관해 화면이 조회(poll)하게 한다. 통합 보기를 닫으면 프로세스도 끝낸다(상주하지 않음).
"""
from __future__ import annotations

import json
import logging
import subprocess
import threading
import time
from pathlib import Path

from .ktm_service import bundled_exe

_log = logging.getLogger(__name__)
HOST_ARGUMENT = "--search-host"
SEARCH_TIMEOUT_SECONDS = 60.0
PAGE_TIMEOUT_SECONDS = 15.0


def failure(generation, reason):
    """검색하지 못한 상태. 문구는 호스트(QMSBUG1)와 같다."""
    return {"event": "error", "generation": generation, "message": reason, "summary": "검색 못함",
            "notice": {"title": "검색을 완료하지 못했습니다",
                       "detail": f"0건은 '파일이 없다'는 뜻이 아닙니다. {reason}"}}


def supports_search_host(path):
    """옛 ktm.exe는 모르는 인자를 무시하고 단축키 창을 띄우므로 실행 전에 확인한다."""
    try:
        return HOST_ARGUMENT.encode("ascii") in Path(path).read_bytes()
    except OSError:
        return False


class UnifiedSearchHost:
    def __init__(self, *, executable=bundled_exe, popen=subprocess.Popen, overrides=None,
                 supported=supports_search_host, clock=time.monotonic):
        self._executable = executable
        self._popen = popen
        self._overrides = dict(overrides or {})
        self._supported = supported
        self._clock = clock
        self._condition = threading.Condition()
        self._send_lock = threading.Lock()
        self._process = None
        self._generation = 0
        self._latest = None
        self._sequence = 0
        self._started = 0.0

    def _publish(self, event):
        """현재 세대의 응답만 남긴다(늦게 온 옛 검색은 버린다). 호출자가 잠금을 잡는다."""
        if event.get("generation") != self._generation:
            return
        self._latest = event
        self._sequence += 1
        self._condition.notify_all()

    def _read(self, process):
        try:
            for line in process.stdout:
                try:
                    event = json.loads(line.decode("utf-8"))
                except ValueError:
                    continue
                if isinstance(event, dict):
                    with self._condition:
                        if process is self._process:
                            self._publish(event)
        except (OSError, ValueError):
            _log.debug("통합 검색 호스트 읽기 종료", exc_info=True)
        close = getattr(process.stdout, "close", None)
        if close:
            close()
        with self._condition:
            if process is self._process:
                self._process = None
                self._publish(failure(self._generation, "통합 검색 프로그램이 끝났습니다. 다시 검색해 주세요."))

    def _ensure_process(self):
        if self._process is not None and self._process.poll() is None:
            return self._process
        exe = self._executable()
        if exe is None:
            raise OSError("통합 검색 실행 파일(ktm.exe)이 앱에 없습니다.")
        if not self._supported(exe):
            raise OSError("통합 검색을 지원하지 않는 이전 ktm.exe입니다.")
        process = self._popen([str(exe), HOST_ARGUMENT], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              stderr=subprocess.DEVNULL, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self._process = process
        threading.Thread(target=self._read, args=(process,), name="unified-search-host", daemon=True).start()
        return process

    @staticmethod
    def _write(process, message):
        process.stdin.write((json.dumps(message, ensure_ascii=False) + "\n").encode("utf-8"))
        process.stdin.flush()

    def start(self, request):
        """새 검색. 이전 검색은 호스트가 취소한다. 세대 번호를 돌려준다."""
        with self._send_lock:
            with self._condition:
                self._generation += 1
                generation = self._generation
                self._latest = None
                self._started = self._clock()
                self._condition.notify_all()
                try:
                    process = self._ensure_process()
                except OSError as exc:
                    self._publish(failure(generation, str(exc)))
                    return generation
            try:
                self._write(process, {**request, **self._overrides, "command": "start", "generation": generation})
            except (OSError, ValueError):
                with self._condition:
                    self._publish(failure(generation, "통합 검색 프로그램에 요청을 보내지 못했습니다."))
        return generation

    def poll(self, generation):
        """가장 최근 응답. 아직 없으면 None. 지난 세대는 {"event": "stale"}."""
        with self._condition:
            if generation != self._generation:
                return {"event": "stale", "generation": generation}
            final = self._latest is not None and self._latest.get("event") != "part"
            expired = not final and self._clock() - self._started > SEARCH_TIMEOUT_SECONDS
        if expired:
            self.cancel(generation)
            return failure(generation, "검색 시간이 초과되었습니다.")
        with self._condition:
            return self._latest

    def page(self, generation, cursor):
        """보관 중인 세션의 다른 페이지. 같은 커서는 같은 결과다."""
        with self._send_lock:
            with self._condition:
                process = self._process
                if generation != self._generation or process is None:
                    return failure(generation, "만료된 검색 세션입니다. 다시 검색해 주세요.")
                before = self._sequence
            try:
                self._write(process, {"command": "page", "generation": generation, "cursor": cursor})
            except (OSError, ValueError):
                return failure(generation, "통합 검색 프로그램에 요청을 보내지 못했습니다.")
        with self._condition:
            answered = self._condition.wait_for(
                lambda: self._sequence != before or generation != self._generation, PAGE_TIMEOUT_SECONDS)
            if generation != self._generation:
                return {"event": "stale", "generation": generation}
            return self._latest if answered else failure(generation, "다음 결과를 받는 시간이 초과되었습니다.")

    def cancel(self, generation):
        with self._send_lock:
            with self._condition:
                if generation != self._generation or self._process is None:
                    return
                process = self._process
                self._latest = None
                # 취소 뒤 늦게 온 응답이 화면에 반영되지 않게 세대를 넘긴다.
                self._generation += 1
                self._condition.notify_all()
            try:
                self._write(process, {"command": "cancel", "generation": generation})
            except (OSError, ValueError):
                _log.debug("통합 검색 취소 전달 실패", exc_info=True)

    def release(self):
        """검색어를 비웠을 때: 세션·색인 핸들만 놓고 프로세스는 다음 입력을 위해 둔다."""
        with self._send_lock:
            with self._condition:
                process, generation = self._process, self._generation
                self._latest = None
                self._generation += 1
                self._condition.notify_all()
            if process is None:
                return
            try:
                self._write(process, {"command": "close", "generation": generation})
            except (OSError, ValueError):
                _log.debug("통합 검색 세션 해제 전달 실패", exc_info=True)

    def close(self):
        """프로세스를 끝내 세션·색인 핸들을 놓는다. 다음 검색 때 다시 띄운다."""
        with self._condition:
            process, self._process = self._process, None
            self._latest = None
            self._generation += 1
            self._condition.notify_all()
        if process is None:
            return
        try:
            process.stdin.close()
        except (OSError, ValueError):
            pass
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)
