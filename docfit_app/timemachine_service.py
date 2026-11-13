"""타임머신 화면 명령과 기존 KAgent 색인 저장소의 앱 측 연결."""
from __future__ import annotations

import base64
import json
import logging
import math
import os
import queue
import stat
import subprocess
import tempfile
import threading
import time
from pathlib import Path

from docfit_core.timemachine import thumbs
from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine.related import related_documents
from docfit_core.timemachine.rules import DEFAULT_EXTENSIONS, IMAGE_EXTENSIONS, KINDS, Rules, SUPPORTED_EXTENSIONS, excluded_name
from docfit_core.timemachine.scan import default_roots, scan_roots
from docfit_core.timemachine.store import Store
from .idle_gate import DEFAULT_IDLE_SECONDS, clamp_idle_seconds
from .kagent_client import KAgentClient, data_dir

THUMBNAIL_SLOTS = 2
THUMBNAIL_WAIT_SECONDS = 8.0
THUMBNAIL_TIME_LIMIT = 5.0
# 쉬는 시간 색인(IDLE1): 스케줄러 작업 이름과, 쉬는 시간마다 변경분을 다시 훑는 최소 간격(초)
CONTENT_IDLE_JOB = "timemachine-content"
CONTENT_IDLE_EVERY_SECONDS = 1800


ALLOWED_FEATURES = frozenset(("rename_batch", "convert", "document_edit"))


class TimemachineService:
    def __init__(self, settings_root, *, db_path=None, agent=None, opener=None,
                 revealer=None, scanner=scan_roots, thumb_dir=None, feature_api=None,
                 developer_mode=None, idle_scheduler=None):
        self.settings_root = Path(settings_root)
        self.settings_path = self.settings_root / "timemachine.json"
        self.store = Store(db_path or data_dir() / "timeline.db")
        excluded_rows = self.store.purge_excluded()
        content_db = (Path(db_path).with_suffix(".content.sqlite") if db_path else data_dir() / "timeline-content.sqlite")
        self.content_index = ContentSearchIndex(content_db)
        self.agent = agent if agent is not None else KAgentClient()
        self.opener = opener if opener is not None else self._open_file
        self.revealer = revealer if revealer is not None else self._reveal_file
        self.scanner = scanner
        self.thumbnails = thumbs.ThumbnailCache(thumb_dir or data_dir() / "thumbs")
        thumbs.purge_excluded_cache(self.thumbnails, excluded_rows)
        self.feature_api = feature_api
        self.developer_mode = developer_mode or (lambda: False)
        self._pdf_child_target = None
        self._thumbnail_slots = threading.BoundedSemaphore(THUMBNAIL_SLOTS)
        self._settings_lock = threading.RLock()
        self._scan_lock = threading.Lock()
        self._content_lock = threading.RLock()
        self._content_cancel = threading.Event()
        self._closing = False
        self._scan_thread = None
        self._content_thread = None
        self._content_process = None
        self._content_pending_roots = None
        self._content_state = self._new_content_state(running=False, phase="idle", trigger=None)
        self._scan_state = {"running": False, "seen": 0, "written": 0, "removed": 0,
                            "errors": [], "started_at": None, "finished_at": None}
        self._rescan = False
        self._content_idle_ctx = None
        self._settings = self._load_settings()
        # 쉬는 시간 색인(IDLE1): 스케줄러가 주어졌을 때만(앱). 시험·다른 진입점은 예전처럼 곧바로 자동 재색인.
        self.idle_scheduler = idle_scheduler
        if idle_scheduler is not None:
            self._configure_idle()
            idle_scheduler.register(_ContentIdleJob(self), every=CONTENT_IDLE_EVERY_SECONDS)
        if self.content_index.needs_reindex_count(roots=self._settings["roots"]) > 0:
            self._schedule_content_refresh()

    @staticmethod
    def _open_file(path):
        os.startfile(path)

    @staticmethod
    def _reveal_file(path):
        subprocess.Popen(["explorer", "/select," + path])

    def _defaults(self):
        return {"roots": default_roots(), "time_mode": "max", "enabled": False, "extensions": list(DEFAULT_EXTENSIONS), "rust_content_index": True,
                "idle_indexing": True, "idle_seconds": DEFAULT_IDLE_SECONDS}

    @staticmethod
    def _idle_values(source, fallback):
        """쉬는 시간 색인 설정 두 값. 잘못된 값은 그 값만 fallback으로(조사 폴더 설정을 잃지 않게)."""
        enabled = source.get("idle_indexing", fallback["idle_indexing"])
        if type(enabled) is not bool:
            enabled = fallback["idle_indexing"]
        seconds = source.get("idle_seconds", fallback["idle_seconds"])
        try:
            seconds = clamp_idle_seconds(seconds)
        except ValueError:
            seconds = fallback["idle_seconds"]
        return {"idle_indexing": enabled, "idle_seconds": seconds}

    def _configure_idle(self):
        if self.idle_scheduler is not None:
            self.idle_scheduler.configure(enabled=self._settings.get("idle_indexing", True),
                                          idle_seconds=self._settings.get("idle_seconds", DEFAULT_IDLE_SECONDS))

    def _idle_indexing_on(self):
        return self.idle_scheduler is not None and self._settings.get("idle_indexing", True)

    def _schedule_content_refresh(self):
        """자동 재색인 요청 한 곳: 쉬는 시간 색인이 켜져 있으면 스케줄러에 맡기고, 아니면 예전처럼 곧바로."""
        if self._idle_indexing_on():
            self.idle_scheduler.request(CONTENT_IDLE_JOB)
        else:
            self._refresh_content_index_async()

    def run_idle_content_refresh(self, ctx):
        """스케줄러 스레드에서: 변경분 자동 재색인을 쉬는 시간 모드로 돌리고 끝날 때까지 기다린다."""
        if not self._refresh_content_index_async(idle_ctx=ctx):
            return
        with self._content_lock:
            worker = self._content_thread
        while worker is not None and worker.is_alive():
            worker.join(0.2)
            if ctx.stopped and worker.is_alive():
                self.cancel_content_index(wait=True, timeout=2.0)
                break

    def _klib_executable(self):
        env_exe = os.environ.get("KLIB_EXE")
        if env_exe and os.path.isfile(env_exe):
            return Path(env_exe)
        candidates = []
        if getattr(__import__('sys'), "frozen", False):
            candidates.append(Path(__import__('sys')._MEIPASS) / "resources" / "klib.exe")
        candidates.extend((Path(__file__).resolve().parents[1] / "resources" / "klib.exe",
                           Path(__file__).resolve().parents[1] / "klib" / "target" / "debug" / "klib.exe",
                           Path(__file__).resolve().parents[1] / "klib" / "target" / "release" / "klib.exe"))
        return next((path for path in candidates if path.is_file()), None)

    def _run_klib_index(self, paths, cancel_event, seen, indexed, errors):
        """Index one supported batch; failed files are returned for Python fallback."""
        executable = self._klib_executable()
        if not executable or not paths:
            return paths, seen, indexed, errors
        with self.content_index._lock:
            if getattr(self.content_index.db, "in_transaction", False):
                self.content_index.db.commit()
        process = None
        outcomes = {}
        try:
            process = subprocess.Popen([str(executable), "index-content", self.content_index.path, *map(str, paths)],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, encoding="utf-8", errors="replace", bufsize=1)
            with self._content_lock:
                self._content_process = process
            lines = queue.Queue()
            def read_output():
                for output_line in process.stdout:
                    lines.put(output_line)
                lines.put(None)
            threading.Thread(target=read_output, name="klib-content-progress", daemon=True).start()
            output_done = False
            while not output_done:
                if cancel_event.is_set():
                    process.terminate()
                    try: process.wait(timeout=2)
                    except subprocess.TimeoutExpired: process.kill()
                    return [], seen, indexed, errors
                try:
                    line = lines.get(timeout=.05)
                except queue.Empty:
                    continue
                if line is None:
                    output_done = True
                    continue
                try:
                    event = json.loads(line)
                    path = os.path.normcase(os.path.abspath(event.get("path", "")))
                    status = event.get("status")
                    if path:
                        outcomes[path] = status
                    if status in ("indexed", "skipped"):
                        seen += 1
                        indexed += status == "indexed"
                        with self._content_lock:
                            self._content_state.update(seen=seen, indexed=indexed, errors=errors)
                except (ValueError, TypeError):
                    continue
            returncode = process.wait()
            failed = [path for path in paths if returncode != 0 or outcomes.get(os.path.normcase(os.path.abspath(str(path)))) in (None, "error")]
            return failed, seen, indexed, errors
        except (OSError, ValueError, subprocess.SubprocessError):
            if process and process.poll() is None:
                process.terminate()
                try: process.wait(timeout=2)
                except subprocess.TimeoutExpired: process.kill()
            return paths, seen, indexed, errors
        finally:
            if process and process.stdout:
                process.stdout.close()
            with self._content_lock:
                self._content_process = None

    def close(self):
        with self._content_lock:
            if self._closing:
                return
            self._closing = True
        if self.idle_scheduler is not None:
            self.idle_scheduler.unregister(CONTENT_IDLE_JOB)
        self.cancel_content_index(wait=False)
        with self._scan_lock:
            scan_worker = self._scan_thread
        for worker in (scan_worker, self._content_thread):
            if worker and worker.ident is not None and worker is not threading.current_thread():
                worker.join()
        self.content_index.close()
        self.store.close()

    def content_status(self):
        """시작·취소·조회가 공유하는 작업 상태와 저장 상태 응답."""
        with self._content_lock:
            state = dict(self._content_state)
            ctx = self._content_idle_ctx
        if ctx is not None and state.get("running"):
            state["yielded"] = bool(ctx.yielded)
        idle = self.idle_scheduler.status() if self.idle_scheduler is not None else None
        return {"ok": True, **state, "idle": idle,
                **self.content_index.status_summary(roots=self._settings["roots"])}

    @staticmethod
    def _new_content_state(*, running, phase, trigger):
        """작업 상태 한 벌. trigger: manual(사용자 시작) | auto(곧바로 자동) | idle(쉬는 시간 자동) | None.
        total은 처리할 문서 수가 정해지면(목록을 다 훑은 뒤) 채운다. yielded는 쉬는 시간 모드에서 사용자에게 양보 중."""
        return {"running": running, "seen": 0, "indexed": 0, "errors": 0, "cancelled": False,
                "started_at": time.time() if running else None, "finished_at": None,
                "phase": phase, "failure_reason": None, "trigger": trigger, "total": None,
                "processed": 0, "yielded": False}

    def _finish_content_index(self, cancel_event):
        with self._content_lock:
            self._content_state.update(running=False, cancelled=cancel_event.is_set(), finished_at=time.time(),
                                       phase="failed" if self._content_state.get("failure_reason") else
                                       "cancelled" if cancel_event.is_set() else "finished", yielded=False)
            self._content_idle_ctx = None

    def _content_index_worker(self, roots, extensions, cancel_event=None, use_rust=True, idle_ctx=None):
        cancel_event = cancel_event or self._content_cancel
        seen = indexed = errors = 0
        processed = 0
        total = None

        def proceed():
            """문서(또는 폴더) 하나를 시작하기 직전. 쉬는 시간 모드면 사용자가 쓰는 동안 여기서 기다린다."""
            if cancel_event.is_set():
                return False
            if idle_ctx is None:
                return True
            if not idle_ctx.checkpoint(cancel_event):
                cancel_event.set()
                return False
            return True

        def advance():
            nonlocal processed
            processed += 1
            with self._content_lock:
                self._content_state.update(processed=processed)
            if idle_ctx is not None:
                idle_ctx.progress(processed, total)

        rules = Rules(extensions=extensions, extra_extensions=extensions)  # 본문 색인 확장자(예: txt)는 타임라인 종류에 없어도 색인한다
        try:
            rust_paths, python_paths, stale_paths = [], [], []
            for root in roots:
                for folder, dirs, files in os.walk(root, followlinks=False):
                    if not proceed(): break
                    dirs[:] = [d for d in dirs if not rules.skip_dir(d) and not cancel_event.is_set()]
                    for name in files:
                        if cancel_event.is_set():
                            break
                        ext = Path(name).suffix.lower().lstrip(".")
                        if excluded_name(name):
                            stale_paths.append(os.path.join(folder, name))
                            continue
                        if ext not in extensions:
                            continue
                        path = os.path.join(folder, name)
                        if not self._path_in_active_root(path):
                            continue
                        if cancel_event.is_set(): break
                        with self._content_lock:
                            if self._content_pending_roots is not None:
                                self._content_pending_roots.discard(os.path.normcase(os.path.abspath(path)))
                        if ext in IMAGE_EXTENSIONS:
                            # 설계 의도: 디스크에 존재하는 이미지 파일은 pending 색인 큐에서 정상 제거하여
                            # 스캔 종료 후 불필요한 삭제 검사를 생략하고, 본문 텍스트 추출 대상이 아니므로
                            # index_file 호출을 건너뛰어 오류 및 처리 카운트를 증가시키지 않는다.
                            continue
                        if rules.skip_file(name):
                            stale_paths.append(path)
                            continue
                        if use_rust and self._settings.get("rust_content_index", True) and ext in {"txt", "md", "hwpx", "docx", "hwp"}:
                            rust_paths.append(path)
                        else:
                            python_paths.append(path)
                    if cancel_event.is_set():
                        break
                if cancel_event.is_set():
                    break
            relevel_paths = []
            if not cancel_event.is_set() and rust_paths:
                fallback, seen, indexed, errors = self._run_klib_index(rust_paths, cancel_event, seen, indexed, errors)
                python_paths.extend(fallback)
                # Rust는 본문·문단만 빠르게 쓰고, 위계(level·level_rank·표 칸)는 Python 규칙이 유일한 기준이다.
                # Rust가 쓴 문서는 위계 버전이 낮으므로 그 문서만 위계를 다시 매긴다(진행 수는 Rust 단계에서 이미 셈).
                failed = {os.path.normcase(os.path.abspath(str(path))) for path in fallback}
                relevel_paths = self.content_index.stale_hierarchy_paths(
                    [path for path in rust_paths if os.path.normcase(os.path.abspath(str(path))) not in failed])
            total = len(relevel_paths) + len(python_paths)
            with self._content_lock:
                self._content_state.update(total=total)
            if idle_ctx is not None:
                idle_ctx.progress(processed, total)
            with self.content_index.batch():
                for path in relevel_paths:
                    if not proceed(): break
                    result = self.content_index.relevel_file(path)
                    errors += bool(result["error"])
                    with self._content_lock:
                        self._content_state.update(errors=errors)
                    advance()
                for path in python_paths:
                    if not proceed(): break
                    result = self.content_index.index_file(path)
                    seen += 1
                    indexed += bool(result["indexed"])
                    errors += bool(result["error"])
                    with self._content_lock:
                        self._content_state.update(seen=seen, indexed=indexed, errors=errors)
                    advance()
            # 취소가 끝나기 전에 건너뛴 파일을 지우지 않도록 색인 완료 후 정리한다.
            if not cancel_event.is_set():
                for path in stale_paths:
                    if cancel_event.is_set(): break
                    self.content_index.remove_file(path)
            # 연결이 끊긴 드라이브의 문서는 지워진 것으로 보지 않는다.
            available = [os.path.normcase(os.path.abspath(root)) + os.sep for root in roots if os.path.isdir(root)]
            active_roots = [os.path.normcase(os.path.abspath(root)) + os.sep
                            for root in self._settings["roots"] if os.path.isdir(root)]
            with self._content_lock:
                pending = tuple(self._content_pending_roots or ())
            for path in pending:
                if cancel_event.is_set():
                    break
                normalized = os.path.normcase(os.path.abspath(path))
                in_scan_roots = normalized.startswith(tuple(available))
                in_active_roots = normalized.startswith(tuple(active_roots))
                extension_allowed = Path(path).suffix.lower().lstrip(".") in self._settings["extensions"]
                if in_scan_roots and (not in_active_roots or not extension_allowed or not os.path.exists(path)):
                    self.content_index.remove_file(path)
        except Exception as exc:
            logging.getLogger(__name__).exception("타임머신 본문 색인 실패")
            with self._content_lock:
                self._content_state.update(phase="failed", failure_reason=f"{type(exc).__name__}: {exc}")
        finally:
            if idle_ctx is not None:
                idle_ctx.leave_background()  # 우선순위는 스레드 단위라 이 스레드에서 푼다
            self._finish_content_index(cancel_event)

    def _start_content_index(self):
        with self._content_lock:
            if self._closing:
                return self._error("TM_INDEX_FAILED", "색인 작업을 시작하지 못했습니다.")
            if self._content_state["running"] or (self._content_thread and self._content_thread.is_alive()):
                # 쉬는 시간 자동 색인이 돌던 중에 사용자가 직접 시작하면 기다리지 않고 바로 끝까지 돌린다.
                if self._content_idle_ctx is not None:
                    self._content_idle_ctx.promote()
                    self._content_state.update(trigger="manual", yielded=False)
                return self.content_status()
            self._content_pending_roots = set(self.content_index.indexed_paths(roots=self._settings["roots"]))
            cancel_event = threading.Event()
            self._content_cancel = cancel_event
            self._content_state = self._new_content_state(
                running=True, phase="reindexing" if self._content_pending_roots else "indexing", trigger="manual")
            self._content_thread = threading.Thread(target=self._content_index_worker,
                args=(list(self._settings["roots"]), frozenset(self._settings["extensions"]), cancel_event),
                name="timemachine-content-index", daemon=True)
            try:
                self._content_thread.start()
            except Exception as exc:
                self._content_state.update(running=False, finished_at=time.time(), phase="failed",
                                           failure_reason=f"{type(exc).__name__}: {exc}")
                return {**self.content_status(), **self._error("TM_INDEX_FAILED", f"색인 작업을 시작하지 못했습니다: {exc}")}
            return self.content_status()

    def cancel_content_index(self, *, wait=False, timeout=2.0):
        with self._content_lock:
            cancel_event = self._content_cancel
            worker = self._content_thread
            if not self._content_state["running"]:
                return self.content_status()
            cancel_event.set()
            self._content_state["phase"] = "cancel_requested"
            process = self._content_process
        if process and process.poll() is None:
            process.terminate()
        if wait and worker and worker is not threading.current_thread():
            worker.join(timeout=max(0.0, timeout))
            if worker.is_alive():
                with self._content_lock:
                    return {**self.content_status(), "ok": False, "code": "TM_CANCEL_TIMEOUT",
                            "message": "색인 작업 취소 대기 시간이 초과되었습니다."}
        with self._content_lock:
            return self.content_status()

    def _load_settings(self):
        defaults = self._defaults()
        try:
            data = json.loads(self.settings_path.read_text(encoding="utf-8"))
            roots = data["roots"]
            mode = data["time_mode"]
            enabled = data["enabled"]
            roots = self._validate_roots(roots)
            if mode not in ("max", "mtime") or type(enabled) is not bool:
                raise ValueError("settings")
            extensions = self._validate_extensions(data.get("extensions", list(DEFAULT_EXTENSIONS)))
            rust_content_index = data.get("rust_content_index", True)
            if type(rust_content_index) is not bool: raise ValueError("settings")
            return {"roots": roots, "time_mode": mode, "enabled": enabled, "extensions": extensions,
                    "rust_content_index": rust_content_index, **self._idle_values(data, defaults)}
        except (OSError, ValueError, TypeError, KeyError):
            return defaults

    @staticmethod
    def _root_key(path):
        return os.path.normcase(os.path.normpath(os.path.abspath(path))).casefold()

    def _parts_under_root(self, path):
        """조사 폴더 아래의 폴더 이름만 돌려준다. 조사 폴더 자체의 상위(예: AppData, Temp)는 제외 규칙에 넣지 않는다."""
        file_path = os.path.normcase(os.path.normpath(os.path.abspath(path)))
        for root in self._settings["roots"]:
            root_path = os.path.normcase(os.path.normpath(os.path.abspath(root)))
            try:
                if os.path.commonpath((root_path, file_path)) == root_path:
                    return Path(os.path.relpath(file_path, root_path)).parts[:-1]
            except ValueError:
                continue
        return Path(path).parts[:-1]

    def _path_in_active_root(self, path, stored_root=None):
        if not isinstance(path, str) or not os.path.isabs(path):
            return False
        normalized_stored_root = self._root_key(stored_root) if isinstance(stored_root, str) else None
        for root in self._settings["roots"]:
            if normalized_stored_root is not None and self._root_key(root) != normalized_stored_root:
                continue
            try:
                root_path = os.path.normcase(os.path.normpath(os.path.abspath(root))).casefold()
                file_path = os.path.normcase(os.path.normpath(os.path.abspath(path)))
                file_path = file_path.casefold()
                if os.path.commonpath((root_path, file_path)) == root_path:
                    return True
            except (OSError, ValueError):
                continue
        return False

    @staticmethod
    def _real_path_under_root(path, root):
        try:
            path, root = (os.path.normcase(value).casefold() for value in (path, root))
            return os.path.commonpath((path, root)) == root
        except (OSError, ValueError):
            return False

    @staticmethod
    def _validate_roots(roots):
        if not isinstance(roots, list) or len(roots) > 20:
            raise ValueError("조사 폴더는 최대 20개까지 지정할 수 있습니다.")
        result, seen = [], set()
        for raw in roots:
            if not isinstance(raw, str) or not os.path.isabs(raw):
                raise ValueError("조사 폴더는 절대 경로로 지정해 주세요.")
            path = os.path.abspath(raw)
            if any(part.casefold() == "$recycle.bin" for part in Path(path).parts):
                raise ValueError("Windows 휴지통($Recycle.Bin)은 타임머신 조사 폴더로 선택할 수 없습니다.")
            try:
                info = os.lstat(path)
            except OSError as exc:
                raise ValueError("존재하는 폴더를 선택해 주세요.") from exc
            attributes = getattr(info, "st_file_attributes", 0)
            if (not stat.S_ISDIR(info.st_mode) or os.path.islink(path) or
                    attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)):
                raise ValueError("심볼릭 링크나 정션이 아닌 폴더를 선택해 주세요.")
            key = TimemachineService._root_key(path)
            if key not in seen:
                result.append(path)
                seen.add(key)
        return result

    @staticmethod
    def _validate_extensions(values):
        allowed = set(SUPPORTED_EXTENSIONS)
        if not isinstance(values, list) or not values or any(not isinstance(v, str) for v in values):
            raise ValueError("적용 확장자를 확인해 주세요.")
        result = []
        for value in values:
            ext = value.lower().lstrip(".")
            if ext not in allowed:
                raise ValueError("지원하지 않는 확장자입니다.")
            if ext not in result:
                result.append(ext)
        return result

    def _save_settings(self, settings):
        self.settings_root.mkdir(parents=True, exist_ok=True)
        fd, temporary = tempfile.mkstemp(prefix="timemachine-", suffix=".tmp", dir=self.settings_root)
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
                json.dump(settings, stream, ensure_ascii=False, indent=2)
                stream.write("\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, self.settings_path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    @staticmethod
    def _error(code, message):
        return {"ok": False, "code": code, "message": message}

    def _require_developer_mode(self):
        enabled = bool(self.developer_mode()) if callable(self.developer_mode) else False
        if not enabled:
            return self._error("DEVELOPER_MODE_REQUIRED", "본문검색은 개발자 모드에서만 사용할 수 있습니다.")
        return None

    def _agent_running(self):
        return bool(self.agent.running())

    def _filter_args(self, payload, *, range_required=False):
        kinds = payload.get("kinds")
        roots = payload.get("roots", self._settings["roots"])
        query = payload.get("query", "")
        if kinds is not None and (not isinstance(kinds, list) or any(k not in KINDS for k in kinds)):
            raise ValueError("종류 필터를 확인해 주세요.")
        if roots is not None and (not isinstance(roots, list) or any(
                not isinstance(root, str) or self._root_key(root) not in
                {self._root_key(item) for item in self._settings["roots"]} for root in roots)):
            raise ValueError("현재 조사 폴더 안에서 선택해 주세요.")
        if not isinstance(query, str) or len(query) > 200:
            raise ValueError("이름 검색은 200자 이하로 입력해 주세요.")
        filters = {"kinds": kinds, "roots": roots, "query": query or None}
        if not range_required:
            return filters
        start, end = payload.get("start"), payload.get("end")
        if (isinstance(start, bool) or isinstance(end, bool) or
                not isinstance(start, (int, float)) or not isinstance(end, (int, float)) or
                not math.isfinite(start) or not math.isfinite(end) or start >= end):
            raise ValueError("조회 기간을 확인해 주세요.")
        return start, end, filters

    def _status(self):
        with self._scan_lock:
            scan = dict(self._scan_state, errors=list(self._scan_state["errors"]))
        return {"ok": True, **self._settings, "stats": self.store.stats(),
                "agent_running": self._agent_running(), "watch": self._watch_mode(), "scan": scan}

    def _watch_mode(self):
        """화면 표시용: 에이전트가 실제로 조사 폴더를 맡았을 때만 'agent'(켜져만 있으면 안 됨)."""
        roots = self._settings["roots"]
        if not self._settings["enabled"] or not roots or not self._agent_running():
            return "app"
        status = self.agent.status_file()
        return "agent" if status and status.get("timeline_roots") == len(roots) else "app"

    @classmethod
    def _same_roots(cls, left, right):
        return (isinstance(left, list) and isinstance(right, list) and
                sorted(cls._root_key(str(item)) for item in left) ==
                sorted(cls._root_key(str(item)) for item in right))

    @staticmethod
    def _agent_settings(settings):
        # 꺼진 타임머신의 폴더를 에이전트가 계속 감시하면 안 되므로 빈 목록을 보낸다.
        return {"timeline_roots": settings["roots"] if settings["enabled"] else [],
                "timeline_mtime_only": settings["time_mode"] == "mtime"}

    def _sync_agent(self, *, force=False):
        """에이전트에 조사 폴더를 맡긴다. 맡았으면 True, 아니면 앱이 색인해야 하므로 False.

        에이전트를 타임머신보다 나중에 켜면 에이전트 설정에 폴더가 없으므로, 보내기 전에 비교한다.
        """
        wanted = self._agent_settings(self._settings)
        try:
            if not self._agent_running():
                return False
            if not force:
                current = self.agent.request("settings_get")
                current = current.get("settings") if isinstance(current, dict) else None
                if (isinstance(current, dict) and
                        self._same_roots(current.get("timeline_roots"), wanted["timeline_roots"]) and
                        current.get("timeline_mtime_only") == wanted["timeline_mtime_only"]):
                    return True
            reply = self.agent.request("settings_set", settings=wanted)
            applied = reply.get("settings") if isinstance(reply, dict) else None
            # 에이전트가 거른 폴더(네트워크 경로 등)가 있으면 앱이 색인한다.
            return (isinstance(applied, dict) and
                    self._same_roots(applied.get("timeline_roots"), wanted["timeline_roots"]))
        except Exception:
            logging.getLogger(__name__).debug("KAgent 설정 알림을 전달하지 못했습니다.", exc_info=True)
            return False

    def _scan_worker(self, roots, mode):
        def progress(result):
            with self._scan_lock:
                self._scan_state.update(seen=result.seen, written=result.written,
                                        removed=result.removed,
                                        errors=[str(item[1]) for item in result.errors])
        try:
            result = self.scanner(self.store, roots, Rules(time_mode=mode, extensions=self._settings["extensions"]), progress=progress)
            with self._settings_lock:
                current = list(self._settings["roots"])
            retained = {self._root_key(root) for root in current}
            for root in roots:
                if self._root_key(root) not in retained:
                    self.store.drop_root(root)
            with self._scan_lock:
                self._scan_state.update(seen=result.seen, written=result.written,
                                        removed=result.removed,
                                        errors=[str(item[1]) for item in result.errors])
            self._schedule_content_refresh()
        except Exception as exc:
            logging.getLogger(__name__).exception("타임머신 색인 갱신 실패")
            with self._scan_lock:
                self._scan_state["errors"] = [type(exc).__name__]
        finally:
            with self._scan_lock:
                self._scan_state.update(running=False, finished_at=time.time())
                again, self._rescan = self._rescan, False
            if again:
                self._start_scan()

    def _start_scan(self, *, agent_ready=None, restart=False):
        if not self._settings["enabled"]:
            return self._error("TM_DISABLED", "타임머신을 켠 뒤 다시 훑어 주세요.")
        if agent_ready is None:
            agent_ready = self._sync_agent()
        with self._scan_lock:
            if self._closing:
                return self._error("TM_SCAN_FAILED", "색인 작업을 시작하지 못했습니다.")
            if self._scan_state["running"]:
                # 진행 중인 훑기는 예전 폴더 목록이므로 설정이 바뀌었으면 끝난 뒤 한 번 더 훑는다.
                self._rescan = self._rescan or restart
                return {"ok": True, "by": "app", "running": True}
            if agent_ready:
                return {"ok": True, "by": "agent"}
            self._scan_state.update(running=True, seen=0, written=0, removed=0,
                                    errors=[], started_at=time.time(), finished_at=None)
            roots, mode = list(self._settings["roots"]), self._settings["time_mode"]
            worker = threading.Thread(target=self._scan_worker, args=(roots, mode),
                                      name="timemachine-scan", daemon=True)
            self._scan_thread = worker
            try:
                worker.start()
            except Exception as exc:
                self._scan_state.update(running=False, finished_at=time.time(),
                                        errors=[type(exc).__name__])
                return self._error("TM_SCAN_FAILED", "색인 작업을 시작하지 못했습니다.")
        return {"ok": True, "by": "app", "running": True}

    def _refresh_content_index_async(self, idle_ctx=None):
        """변경분 자동 재색인을 시작한다. 시작했으면 True. idle_ctx가 있으면 쉬는 시간 모드(양보·낮은 우선순위)."""
        with self._content_lock:
            if self._closing:
                return False
            if self._content_state["running"] or (self._content_thread and self._content_thread.is_alive()):
                return False
            # 본문 색인을 쓴 적이 없으면(색인이 비어 있으면) 자동으로 시작하지 않는다.
            pending = set(self.content_index.indexed_paths(roots=self._settings["roots"]))
            if not pending:
                return False
            self._content_pending_roots = pending
            cancel_event = threading.Event()
            self._content_cancel = cancel_event
            self._content_state = self._new_content_state(running=True, phase="reindexing",
                                                          trigger="idle" if idle_ctx is not None else "auto")
            self._content_idle_ctx = idle_ctx
            self._content_thread = threading.Thread(target=self._content_index_worker,
                args=(list(self._settings["roots"]), frozenset(self._settings["extensions"]), cancel_event, False),
                kwargs={"idle_ctx": idle_ctx}, name="timemachine-content-index", daemon=True)
            try:
                self._content_thread.start()
            except Exception as exc:
                self._content_state.update(running=False, finished_at=time.time(), phase="failed",
                                           failure_reason=f"{type(exc).__name__}: {exc}")
                self._content_idle_ctx = None
                logging.getLogger(__name__).exception("타임머신 본문 자동 색인 시작 실패")
                return False
            return True

    # 본문검색과 위계검색은 일반 모드에서도 허용한다.
    DEVELOPER_ONLY_ACTIONS = frozenset()

    RELATED_QUERY_SORTS = frozenset({"hierarchy_relevance", "mentions", "relevance"})

    def _related_query_search(self, query, payload):
        """검색어만으로 하는 위계검색. 결과 형태는 본문검색(content_search)과 같다(path·file_name 포함)."""
        limit = payload.get("limit", 100)
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("결과 개수를 확인해 주세요.")
        sort = payload.get("sort", "hierarchy_relevance")
        if sort not in self.RELATED_QUERY_SORTS:
            raise ValueError("정렬 방식을 확인해 주세요.")
        if not self._settings["roots"]:
            # search()는 roots가 비면 전체 색인을 찾으므로, 조사 폴더가 없으면 아무것도 돌려주지 않는다.
            return {"ok": True, "mode": "query", "items": []}
        rows = self.content_index.search(query, roots=self._settings["roots"], limit=limit, sort=sort)
        for row in rows:
            row["file_name"] = row["name"]
        return {"ok": True, "mode": "query", "items": rows}

    def _command(self, action, payload):
        if action in self.DEVELOPER_ONLY_ACTIONS:
            denied = self._require_developer_mode()
            if denied is not None:
                return denied
        if action == "status":
            return self._status()
        if action == "settings_get" or (action == "settings" and not payload):
            return {"ok": True, "settings": dict(self._settings, roots=list(self._settings["roots"]))}
        if action in ("settings_set", "settings"):
            roots = self._validate_roots(payload.get("roots"))
            mode, enabled = payload.get("time_mode"), payload.get("enabled")
            if mode not in ("max", "mtime") or type(enabled) is not bool:
                raise ValueError("설정 값을 확인해 주세요.")
            extensions = self._validate_extensions(payload.get("extensions", self._settings["extensions"]))
            rust_content_index = payload.get("rust_content_index", self._settings.get("rust_content_index", True))
            if type(rust_content_index) is not bool: raise ValueError("설정 값을 확인해 주세요.")
            idle_enabled = payload.get("idle_indexing", self._settings.get("idle_indexing", True))
            if type(idle_enabled) is not bool: raise ValueError("설정 값을 확인해 주세요.")
            idle_seconds = clamp_idle_seconds(payload.get("idle_seconds", self._settings.get("idle_seconds", DEFAULT_IDLE_SECONDS)))
            settings = {"roots": roots, "time_mode": mode, "enabled": enabled, "extensions": extensions,
                        "rust_content_index": rust_content_index, "idle_indexing": idle_enabled,
                        "idle_seconds": idle_seconds}
            with self._settings_lock:
                old_roots = list(self._settings["roots"])
                changed = self._settings != settings
                removed = {self._root_key(root) for root in old_roots} - {self._root_key(root) for root in roots}
                self._save_settings(settings)
                self._settings = settings
                for root in old_roots:
                    if self._root_key(root) in removed:
                        self.store.drop_root(root)
            self._configure_idle()
            agent_ready = self._sync_agent(force=True)
            if enabled and changed:
                self._start_scan(agent_ready=agent_ready, restart=True)
            return {"ok": True, "settings": dict(settings, roots=list(roots))}
        if action == "scan":
            return self._start_scan()
        if action == "content_index":
            path = payload.get("path")
            content_rules = Rules(extensions=self._settings["extensions"], extra_extensions=self._settings["extensions"])
            if (not isinstance(path, str) or not self._path_in_active_root(path) or
                    Path(path).suffix.lower().lstrip(".") not in self._settings["extensions"] or
                    any(content_rules.skip_dir(part) for part in self._parts_under_root(path)) or
                    content_rules.skip_file(Path(path).name)):
                return self._error("INVALID_INPUT", "현재 조사 폴더의 허용된 문서를 선택해 주세요.")
            result = self.content_index.index_file(path)
            return {"ok": True, **result}
        if action in ("content_index_all", "content_index_start"):
            return self._start_content_index()
        if action in ("content_index_status", "content_index_progress"):
            return self.content_status()
        if action == "content_index_cancel":
            return self.cancel_content_index(wait=bool(payload.get("wait", False)), timeout=float(payload.get("timeout", 2.0)))
        if action == "content_search":
            query = payload.get("query")
            rows = self.content_index.keyword_search(query, roots=self._settings["roots"],
                limit=payload.get("limit", 100), offset=payload.get("offset", 0))
            for row in rows:
                row["file_name"] = row["name"]
            return {"ok": True, "items": rows}
        if action == "content_export_markdown":
            target = payload.get("path")
            if not isinstance(target, str) or not target.strip():
                raise ValueError("Markdown 저장 경로를 확인해 주세요.")
            query = payload.get("query")
            rows = self.content_index.keyword_search(query, roots=self._settings["roots"],
                limit=payload.get("limit", 500), offset=payload.get("offset", 0))
            target_abs = os.path.normcase(os.path.abspath(target))
            if any(target_abs == os.path.normcase(os.path.abspath(item["path"])) for item in rows):
                return self._error("INVALID_INPUT", "검색 대상 문서 경로에는 결과를 저장할 수 없습니다.")
            from datetime import datetime
            lines = ["# 본문검색 결과", "", f"- 검색어: {query}", f"- 검색 일시: {datetime.now().astimezone().isoformat(timespec='seconds')}", ""]
            for item in rows:
                lines.extend([f"## {item['name']}", f"- 경로: `{item['path']}`"])
                for location in item["locations"]:
                    lines.extend([f"- 위치: 문단 {location['paragraph_index']}", f"- 미리보기: {location['snippet']}"])
                lines.append("")
            Path(target).write_text("\n".join(lines), encoding="utf-8")
            return {"ok": True, "path": os.path.abspath(target), "count": len(rows)}
        if action == "file_search":
            query = payload.get("query")
            return {"ok": True, "items": self.content_index.search_files(query, roots=self._settings["roots"], limit=payload.get("limit", 500))}
        if action == "related_search":
            path = payload.get("path")
            query = payload.get("query")
            if path is None and isinstance(query, str) and query.strip():
                # 검색어 위계검색(ISUI2): 화면 검색창은 기준 문서 없이 검색어만 보낸다.
                # 문단 위계(제목 > 항목 > 본문)가 높은 곳에서 찾은 문서가 앞에 온다.
                return self._related_query_search(query, payload)
            if not isinstance(path, str) or not self._path_in_active_root(path):
                return self._error("INVALID_INPUT", "현재 조사 폴더의 문서를 선택해 주세요.")
            limit = payload.get("limit", 100)
            if type(limit) is not int or not 1 <= limit <= 200:
                raise ValueError("결과 개수를 확인해 주세요.")
            exclusions = self.store.related_exclusions()
            result = related_documents(self.content_index, path, roots=self._settings["roots"],
                                       excluded_pairs=exclusions,
                                       included_pairs=self.store.related_links(), limit=limit)
            # 경로는 내부 판정에만 쓰고 서비스 응답에는 노출하지 않는다.
            # 화면의 '관련 없음' 단추가 문서를 지정할 수 있게 range 응답처럼 folder만 준다.
            for item in result["items"]:
                item_path = item.pop("path", None)
                if isinstance(item_path, str):
                    item["folder"] = os.path.dirname(item_path)
            series = result.get("series")
            if series:
                for group in ("previous", "next", "same", "undated"):
                    for item in series[group]:
                        item.pop("path", None)
            return {"ok": True, **result}
        if action == "related_exclude":
            left, right = payload.get("selected_path"), payload.get("related_path")
            excluded = payload.get("excluded", True)
            if (not isinstance(left, str) or not isinstance(right, str) or type(excluded) is not bool or
                    not self._path_in_active_root(left) or not self._path_in_active_root(right)):
                return self._error("INVALID_INPUT", "현재 조사 폴더의 두 문서를 선택해 주세요.")
            self.store.exclude_related(left, right, excluded)
            return {"ok": True, "excluded": excluded}
        if action == "related_include":
            left, right = payload.get("selected_path"), payload.get("related_path")
            included = payload.get("included", True)
            if (not isinstance(left, str) or not isinstance(right, str) or type(included) is not bool or
                    not self._path_in_active_root(left) or not self._path_in_active_root(right)):
                return self._error("INVALID_INPUT", "현재 조사 폴더의 두 문서를 선택해 주세요.")
            self.store.set_related_link(left, right, included)
            return {"ok": True, "included": included}
        if action == "content_errors":
            return {"ok": True, "items": self.content_index.errors(self._settings["roots"])}
        if action == "range":
            start, end, filters = self._filter_args(payload, range_required=True)
            newest = payload.get("newest_first", True)
            limit, offset = payload.get("limit", 200), payload.get("offset", 0)
            if type(newest) is not bool or type(limit) is not int or not 1 <= limit <= 200 or type(offset) is not int or offset < 0:
                raise ValueError("페이지 범위를 확인해 주세요.")
            if not filters["roots"]:
                return {"ok": True, "items": []}
            filters["extensions"] = self._settings["extensions"]
            rows = self.store.range(start, end, **filters, newest_first=newest, limit=limit, offset=offset)
            rows = [row for row in rows if self._path_in_active_root(row["path"], row.get("root"))]
            items = [{"id": row["id"], "name": row["name"], "kind": row["kind"],
                      "size": row["size"], "mtime": row["mtime"], "ctime": row["ctime"],
                      "work_time": row["work_time"], "flags": row["flags"], "cloud": row["cloud"],
                      "folder": os.path.dirname(row["path"])} for row in rows]
            return {"ok": True, "items": items}
        if action in ("day_counts", "hour_density"):
            filters = self._filter_args(payload)
            if action == "day_counts":
                year, month = payload.get("year"), payload.get("month")
                if type(year) is not int or type(month) is not int or not 1 <= year <= 9999 or not 1 <= month <= 12:
                    raise ValueError("달력 날짜를 확인해 주세요.")
                if not filters["roots"]:
                    return {"ok": True, "days": {}}
                filters["extensions"] = self._settings["extensions"]
                return {"ok": True, "days": self.store.day_counts(year, month, **filters)}
            start, end, range_filters = self._filter_args(payload, range_required=True)
            if not range_filters["roots"]:
                return {"ok": True, "hours": {}}
            range_filters["extensions"] = self._settings["extensions"]
            return {"ok": True, "hours": self.store.hour_density(start, end, **range_filters)}
        if action == "hide":
            file_id = payload.get("id")
            if type(file_id) is not int or file_id <= 0:
                raise ValueError("파일 항목을 확인해 주세요.")
            if not self.store.hide(file_id):
                return self._error("TM_NOT_FOUND", "파일 항목을 찾지 못했습니다.")
            return {"ok": True}
        if action in ("open", "reveal"):
            file_id = payload.get("id")
            row = None
            if "id" in payload:
                if type(file_id) is not int or file_id <= 0:
                    raise ValueError("파일 항목을 확인해 주세요.")
                row = self.store.get(file_id)
                if not row:
                    return self._error("TM_NOT_FOUND", "파일 항목을 찾지 못했습니다.")
                path = row["path"]
            else:
                path = payload.get("path")
                if not isinstance(path, str) or not path or not os.path.isabs(path):
                    raise ValueError("파일 항목을 확인해 주세요.")
            if not self._path_in_active_root(path, row.get("root") if row else None):
                return self._error("TM_NOT_AVAILABLE", "현재 조사 폴더 안의 문서만 열 수 있습니다.")
            # 정션·심볼릭 링크를 통해 조사 폴더 밖으로 나가는 경로도 거부한다.
            real_path = os.path.realpath(path)
            real_roots = [os.path.realpath(root) for root in self._settings["roots"]]
            if not any(self._real_path_under_root(real_path, root) for root in real_roots):
                return self._error("TM_NOT_AVAILABLE", "현재 조사 폴더 밖으로 연결된 문서입니다.")
            rules = Rules()
            if (rules.kind(Path(path).name) not in KINDS or rules.skip_file(Path(path).name) or
                    any(rules.skip_dir(part) for part in self._parts_under_root(path)) or
                    (row and row["hidden"])):
                return self._error("TM_NOT_AVAILABLE", "지원하지 않는 파일입니다.")
            if not os.path.isfile(path):
                if row:
                    self.store.hide(file_id)
                return self._error("TM_MISSING", "파일이 존재하지 않습니다. 이동하거나 삭제했는지 확인해 주세요.")
            try:
                (self.opener if action == "open" else self.revealer)(path)
            except Exception as exc:
                logging.getLogger(__name__).exception("타임머신 파일 작업 실패")
                return self._error("TM_OPEN_FAILED" if action == "open" else "TM_REVEAL_FAILED",
                                   f"파일을 열지 못했습니다: {exc}" if action == "open" else f"폴더를 열지 못했습니다: {exc}")
            return {"ok": True}
        if action == "thumbnail":
            return self._thumbnail(payload.get("id"))
        if action == "thumbnail_cache_stats":
            return {"ok": True, **self.thumbnails.stats()}
        if action == "thumbnail_cache_clear":
            target_bytes = payload.get("target_bytes")
            if isinstance(target_bytes, int) and not isinstance(target_bytes, bool) and target_bytes >= 0:
                result = self.thumbnails.prune(target_bytes)
                return {"ok": True, "removed": result["removed_count"], "freed_bytes": result["removed_bytes"]}
            return {"ok": True, "removed": self.thumbnails.clear()}
        if action == "send_to_feature":
            return self._send_to_feature(payload.get("feature"), payload.get("ids"))
        return self._error("NOT_AVAILABLE", "지원하지 않는 타임머신 명령입니다.")

    def _send_to_feature(self, feature, file_ids):
        if feature not in ALLOWED_FEATURES:
            raise ValueError("지원하지 않는 기능입니다.")
        if (not isinstance(file_ids, list) or not (1 <= len(file_ids) <= 200) or
                any(isinstance(fid, bool) or not isinstance(fid, int) or fid <= 0 for fid in file_ids) or
                len(file_ids) != len(set(file_ids))):
            raise ValueError("파일 항목을 확인해 주세요.")

        if feature in ("convert", "document_edit") and len(file_ids) > 1:
            return self._error("FEATURE_LIMIT", "문서편집/파일변환은 한 번에 한 파일만 지원합니다.")

        paths = []
        for fid in file_ids:
            row = self.store.get(fid)
            if not row:
                return self._error("TM_NOT_FOUND", "파일 항목을 찾지 못했습니다.")
            if row["hidden"]:
                return self._error("TM_NOT_AVAILABLE", "지원하지 않는 파일입니다.")
            if Rules().kind(row["name"]) not in KINDS:
                return self._error("TM_NOT_AVAILABLE", "지원하지 않는 파일입니다.")
            path = row["path"]
            if not self._path_in_active_root(path, row.get("root")):
                return self._error("TM_NOT_AVAILABLE", "지원하지 않는 파일입니다.")
            if row["cloud"]:
                return self._error("TM_NOT_AVAILABLE", "클라우드 전용 파일은 지원하지 않습니다.")
            if not os.path.isfile(path):
                self.store.hide(fid)
                return self._error("TM_MISSING", "파일이 없어 목록에서 정리했습니다.")
            paths.append(path)

        if self.feature_api is not None:
            try:
                res = self.feature_api(feature, list(paths))
                if isinstance(res, dict) and res.get("ok") is False:
                    return self._error(res.get("code", "FEATURE_FAILED"),
                                       res.get("message", "기능 작업을 완료하지 못했습니다."))
                return {"ok": True, "feature": feature, "accepted_ids": list(file_ids)}
            except Exception as exc:
                return self._error("FEATURE_FAILED", str(exc))

        return self._error("FEATURE_UNAVAILABLE", "해당 기능을 사용할 수 없습니다.")

    def _thumbnail_allowed(self, row):
        """색인에 있고 지금 조사 폴더 안(연결 경로로 밖을 가리키지 않음)의 파일인지 본다."""
        path = row["path"]
        if (excluded_name(row["name"]) or row["hidden"] or Rules().kind(row["name"]) not in KINDS or
                not self._path_in_active_root(path, row.get("root"))):
            return False
        if row["cloud"]:
            return True  # 내용을 열지 않고 아이콘으로 끝나므로 실제 경로를 확인할 필요가 없다.
        real = os.path.normcase(os.path.realpath(path)).casefold()
        for root in self._settings["roots"]:
            base = os.path.normcase(os.path.realpath(root)).casefold()
            try:
                if os.path.commonpath((base, real)) == base:
                    return True
            except ValueError:
                continue
        return False

    def _thumbnail(self, file_id):
        if type(file_id) is not int or file_id <= 0:
            raise ValueError("파일 항목을 확인해 주세요.")
        if not self._settings["enabled"]:
            return self._error("TM_DISABLED", "타임머신이 꺼져 있습니다.")
        row = self.store.get(file_id)
        if not row:
            return self._error("TM_NOT_FOUND", "파일 항목을 찾지 못했습니다.")
        if not self._thumbnail_allowed(row):
            return self._error("TM_NOT_AVAILABLE", "지원하지 않는 파일입니다.")
        path = row["path"]
        cached, key = thumbs.lookup(path, self.thumbnails, cloud=bool(row["cloud"]))
        if key is None or cached is not None:
            return self._thumbnail_result(cached)
        if not self._thumbnail_slots.acquire(timeout=THUMBNAIL_WAIT_SECONDS):
            return self._thumbnail_result(None, retry=True)
        if os.path.splitext(path)[1].lower() == ".pdf":
            proc = thumbs.PdfRenderProcess(path, target=self._pdf_child_target)
            try:
                proc.start()
                proc.join(THUMBNAIL_TIME_LIMIT)
                if proc.is_alive():
                    proc.terminate()
                    return self._thumbnail_result(None, retry=True)
                outcome = proc.get_result()
                if not outcome:
                    return self._thumbnail_result(None, retry=True)
                status, data = outcome
                if status == "busy":
                    return self._thumbnail_result(None, retry=True)
                if status == "ok" and data:
                    try:
                        self.thumbnails.put(key, data)
                    except OSError:
                        pass
                    return self._thumbnail_result(data)
                return self._thumbnail_result(None)
            except Exception:
                if proc.is_alive():
                    proc.terminate()
                return self._thumbnail_result(None, retry=True)
            finally:
                proc.close()
                self._thumbnail_slots.release()
        outcome = {}
        def work():
            # 멈춘 작업은 강제로 끝낼 수 없으므로 칸을 끝날 때까지 쥐고 있어 동시 2개 한도를 지킨다.
            try:
                outcome["data"] = thumbs.generate(path, key, self.thumbnails)
            except thumbs.ThumbnailBusy:
                outcome["busy"] = True
            finally:
                self._thumbnail_slots.release()
        worker = threading.Thread(target=work, name="timemachine-thumbnail", daemon=True)
        try:
            worker.start()
        except Exception:
            self._thumbnail_slots.release()
            return self._thumbnail_result(None, retry=True)
        worker.join(THUMBNAIL_TIME_LIMIT)
        return self._thumbnail_result(outcome.get("data"), retry=outcome.get("busy", False))

    @staticmethod
    def _thumbnail_result(data, *, retry=False):
        result = {"ok": True, "data": base64.b64encode(data).decode("ascii") if data else None}
        if retry:
            result["retry"] = True
        return result

    def command(self, action, payload=None):
        if not isinstance(action, str) or not isinstance(payload, (dict, type(None))):
            return self._error("INVALID_INPUT", "명령 입력을 확인해 주세요.")
        try:
            return self._command(action, payload or {})
        except ValueError as exc:
            return self._error("INVALID_INPUT", str(exc))
        except Exception:
            logging.getLogger(__name__).exception("타임머신 명령 처리 실패")
            return self._error("TM_FAILED", "타임머신 작업을 완료하지 못했습니다.")


class _ContentIdleJob:
    """타임머신 본문 변경분 색인을 쉬는 시간 스케줄러 작업(IdleJob)으로 감싼 것."""

    name = CONTENT_IDLE_JOB

    def __init__(self, service):
        self._service = service

    def pending(self):
        service = self._service
        return not service._closing and bool(service._settings["roots"])

    def run(self, ctx):
        self._service.run_idle_content_refresh(ctx)
