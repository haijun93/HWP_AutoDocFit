"""Serialized JSON-lines transport for outliner; ambiguous writes never replay.

States: stopped -> ready -> busy -> ready; transport failure -> one restart
-> fallback; any state -> closed. Only explicitly read-only ops may replay.
"""
from __future__ import annotations

import json
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
import time
import uuid
import weakref

from .api_contract import ServiceError


READ_ONLY = frozenset({
    'load', 'parse', 'serialize', 'search', 'backlinks', 'daily_page',
    'resolve_block_ref', 'block_ref', 'block_backlinks',
    'check_page_references', 'page_references',
})


class _TransportError(Exception):
    def __init__(self, sent=False):
        self.sent = sent


def _io_loop(process, inbox):
    # This thread owns both pipe operations, including writes larger than the
    # pipe buffer. The caller's timeout therefore covers writing AND reading.
    while (job := inbox.get()) is not None:
        payload, reply = job
        try:
            process.stdin.write(payload)
            process.stdin.flush()
            line = process.stdout.readline()
            if not line:
                raise EOFError()
            reply.put(json.loads(line))
        except (OSError, ValueError, EOFError) as error:
            reply.put(error)
            return


def _stop(resources):
    process = resources.pop('process', None)
    inbox = resources.pop('inbox', None)
    worker = resources.pop('worker', None)
    if inbox is not None:
        inbox.put(None)
    if process is not None:
        try:
            if process.poll() is None:
                process.kill()
            process.wait(timeout=2)
        except (OSError, subprocess.TimeoutExpired):
            pass
    if worker is not None:
        worker.join(timeout=2)
    # Kill first: closing a pipe held by a blocking reader/writer can deadlock.
    if process is not None and (worker is None or not worker.is_alive()):
        for stream in (process.stdin, process.stdout):
            try:
                stream.close()
            except OSError:
                pass


def _dispose(resources):
    _stop(resources)
    directory = resources.pop('directory', None)
    if directory is not None:
        # Windows may signal process exit just before releasing its image map.
        # Give that release a bounded grace period; never block app shutdown
        # indefinitely on antivirus/file-system handles to a disposable copy.
        for attempt in range(41):
            try:
                directory.cleanup()
                break
            except OSError:
                if attempt == 40:
                    import logging
                    logging.getLogger(__name__).warning('아웃라이너 임시 실행 파일 정리를 미뤘습니다.')
                    break
                time.sleep(0.05)


class OutlinerEngine:
    def __init__(self, executable, *, runner=subprocess.run, popen=subprocess.Popen,
                 timeout=30, runtime_parent=None):
        self.executable = Path(executable)
        self.runner = runner
        self.popen = popen
        self.timeout = timeout
        self.runtime_parent = runtime_parent
        self.state = 'stopped'
        self._lock = threading.RLock()
        self._closed = threading.Event()
        self._resources = {}
        # The I/O thread never retains self; GC and normal interpreter exit
        # both reap the child and its private executable copy.
        self._finalizer = weakref.finalize(self, _dispose, self._resources)

    def _start(self):
        if self._closed.is_set():
            raise _TransportError()
        if 'directory' not in self._resources:
            directory = tempfile.TemporaryDirectory(prefix='docfit-outliner-', dir=self.runtime_parent)
            self._resources['directory'] = directory
            # Finalizers run newest first at interpreter exit. Re-register ours
            # AFTER TemporaryDirectory's so the child exits before image deletion.
            self._finalizer.detach()
            self._finalizer = weakref.finalize(self, _dispose, self._resources)
            # Windows keeps a running EXE locked. Execute a private copy so an
            # installer can replace the packaged original even while we run.
            shutil.copy2(self.executable, Path(directory.name) / self.executable.name)
        executable = Path(self._resources['directory'].name) / self.executable.name
        process = self.popen(
            [str(executable)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, encoding='utf-8', close_fds=True,
            creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
        )
        self._resources['process'] = process
        if self._closed.is_set():
            _stop(self._resources)
            raise _TransportError()
        inbox = queue.Queue()
        worker = threading.Thread(target=_io_loop, args=(process, inbox),
                                  name='outliner-json', daemon=True)
        self._resources.update(inbox=inbox, worker=worker)
        worker.start()
        self.state = 'ready'

    @staticmethod
    def _validate(value, identifier):
        if (not isinstance(value, dict) or value.get('id') != identifier
                or type(value.get('ok')) is not bool
                or (value['ok'] and not isinstance(value.get('result'), dict))
                or (not value['ok'] and not isinstance(value.get('error'), dict))):
            raise _TransportError(sent=True)
        return value

    def _exchange(self, request):
        try:
            if 'process' not in self._resources:
                self._start()
            if self._resources['process'].poll() is not None:
                raise _TransportError()
        except OSError:
            raise _TransportError() from None
        self.state = 'busy'
        reply = queue.Queue(maxsize=1)
        self._resources['inbox'].put((json.dumps(request, ensure_ascii=False) + '\n', reply))
        try:
            value = reply.get(timeout=self.timeout)
        except queue.Empty:
            raise _TransportError(sent=True) from None
        if isinstance(value, Exception):
            raise _TransportError(sent=True)
        value = self._validate(value, request['id'])
        self.state = 'ready'
        return value

    def _single(self, request):
        try:
            result = self.runner(
                [str(self.executable)], input=json.dumps(request, ensure_ascii=False) + '\n',
                capture_output=True, text=True, encoding='utf-8', timeout=self.timeout,
                check=False, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
            )
            if result.returncode:
                raise ValueError()
            return self._validate(json.loads(result.stdout), request['id'])
        except OSError:
            raise ServiceError('OUTLINER_ENGINE_FAILED', '아웃라이너 엔진 응답을 읽지 못했습니다.') from None
        except (ValueError, subprocess.TimeoutExpired, _TransportError):
            if request.get('op') not in READ_ONLY:
                raise ServiceError('OUTLINER_ENGINE_UNCERTAIN',
                                   '처리 결과를 확인하지 못했습니다. 페이지를 다시 불러와 확인해 주세요.') from None
            raise ServiceError('OUTLINER_ENGINE_FAILED', '아웃라이너 엔진 응답을 읽지 못했습니다.') from None

    def request(self, request):
        with self._lock:
            if self._closed.is_set():
                raise ServiceError('OUTLINER_ENGINE_CLOSED', '아웃라이너 엔진이 종료되었습니다.')
            request = dict(request, id=uuid.uuid4().hex)
            if self.state == 'fallback':
                return self._single(request)
            for attempt in range(2):
                try:
                    return self._exchange(request)
                except _TransportError as error:
                    _stop(self._resources)
                    self.state = 'stopped'
                    if self._closed.is_set():
                        raise ServiceError('OUTLINER_ENGINE_CLOSED', '아웃라이너 엔진이 종료되었습니다.') from None
                    if error.sent and request.get('op') not in READ_ONLY:
                        # Repair the transport with a harmless probe, never the
                        # ambiguous edit/save/rename. A second failure degrades.
                        self.state = 'fallback'
                        if attempt == 0:
                            try:
                                self._exchange({'id': uuid.uuid4().hex, 'op': 'parse', 'document': ''})
                            except _TransportError:
                                _stop(self._resources)
                                self.state = 'fallback'
                        raise ServiceError(
                            'OUTLINER_ENGINE_UNCERTAIN',
                            '처리 결과를 확인하지 못했습니다. 중복 적용을 막기 위해 다시 실행하지 않았습니다. '
                            '페이지를 다시 불러와 내용을 확인해 주세요.',
                        ) from None
            self.state = 'fallback'
            return self._single(request)

    def close(self):
        self._closed.set()
        # Interrupt an in-flight pipe wait before acquiring the request lock.
        process = self._resources.get('process')
        if process is not None:
            try:
                process.kill()
            except OSError:
                pass
        with self._lock:
            self._finalizer()
            self.state = 'closed'
