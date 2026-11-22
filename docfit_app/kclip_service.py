"""몽땅클립보드 화면 서비스: 켜기(동의)·끄기·상태·목록·고정·지우기·되돌려 붙이기·설정.

기록과 빠른 붙여넣기는 독립 상주 프로그램 kagent.exe가 한다(기본 꺼짐). 이 서비스는
사용자가 켤 때만 에이전트를 설치·실행하고, 이후에는 파이프로 명령만 보낸다.
"""
from __future__ import annotations

import json
import logging
import threading
import os
import sys
import tempfile
import time
import subprocess
import webbrowser
from pathlib import Path

from . import kagent_client as kc

_UPGRADE_LOCK = threading.RLock()
_log = logging.getLogger(__name__)

CONSENT = ("몽땅클립보드는 이 PC에서 복사(Ctrl+C)한 글·그림·파일 경로를 기록해 다시 붙여넣을 수 있게 합니다. "
           "기록은 이 PC의 사용자 폴더에만 암호화해 저장하고 밖으로 보내지 않습니다. "
           "암호 관리자가 보내는 '기록 제외' 신호는 따릅니다. 언제든 일시정지·끄기·모두 지우기를 할 수 있습니다.")


def bundled_exe():
    """앱에 들어 있는 kagent.exe(배포판: resources, 개발: kagent/target/release)."""
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1]))
    for candidate in (base / "resources" / "kagent.exe", base / "kagent" / "target" / "release" / "kagent.exe"):
        if candidate.is_file():
            return candidate
    return None


def _norm_text(value):
    return str(value or "").replace("\r\n", "\n").replace("\r", "\n").strip()


def push_theme(mode, client=None):
    """이미 켜진 몽땅클립보드 상주 프로그램(kagent)에 화면 테마를 알린다.
    꺼져 있으면 켜지 않고 건너뛰며, 어떤 오류도 밖으로 내지 않는다. 전달했으면 True."""
    try:
        client = client or kc.KAgentClient()
        if not client.running():
            return False
        client.request("settings_set", settings={"theme": mode})
        return True
    except Exception:
        _log.debug("몽땅클립보드 테마 전달 건너뜀", exc_info=True)
        return False


class _Steps:
    """켜기 경로의 구간별 경과 시간(ms) 기록. 진행 표시용 '지금 구간' 이름도 알린다."""

    def __init__(self, on_phase=None, clock=time.monotonic):
        self._clock = clock
        self._on_phase = on_phase
        self.start = clock()
        self.rows = []

    def __call__(self, name):
        return _Step(self, name)

    def elapsed_ms(self):
        return int((self._clock() - self.start) * 1000)

    def table(self):
        return [dict(step=name, ms=ms) for name, ms in self.rows]

    def summary(self):
        return ", ".join(f"{name} {ms}ms" for name, ms in self.rows) or "-"


class _Step:
    def __init__(self, steps, name):
        self._steps, self._name = steps, name

    def __enter__(self):
        self._t0 = self._steps._clock()
        if self._steps._on_phase:
            self._steps._on_phase(self._name)
        return self

    def __exit__(self, *exc):
        self._steps.rows.append((self._name, int((self._steps._clock() - self._t0) * 1000)))
        return False


_REASON_MESSAGES = (
    ("update busy", "다른 몽땅클립보드 설치·갱신 작업이 끝나기를 기다리다 시간이 지났습니다. 잠시 뒤 다시 시도해 주세요."),
    ("integrity", "몽땅클립보드 프로그램 파일이 손상됐습니다. 다시 설치해 주세요."),
    ("still_running", "실행 중인 이전 몽땅클립보드가 종료되지 않아 새로 켜지 못했습니다."),
    ("start timeout", "몽땅클립보드 프로그램이 제때 시작되지 않았습니다. 백신 검사 등으로 느려졌을 수 있으니 잠시 뒤 다시 시도해 주세요."),
    ("start:", "몽땅클립보드 프로그램을 시작하지 못했습니다."),
    ("install", "몽땅클립보드 프로그램 파일을 교체하지 못했습니다. 다른 프로그램이 파일을 쓰고 있을 수 있습니다."),
    ("no_bundle", "이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다."),
    ("시험 실행", "시험 실행에서는 자동 실행 등록을 바꾸지 않습니다."),
)


def explain_failure(result):
    """kagent 가 돌려준 reason 만 있는 실패에 사람이 읽을 사유를 붙인다(이미 message 가 있으면 그대로)."""
    if not isinstance(result, dict) or result.get("ok") is not False or result.get("message"):
        return result
    reason = str(result.get("reason") or result.get("code") or "")
    for key, text in _REASON_MESSAGES:
        if key in reason:
            return dict(result, message=text, diagnostic=result.get("diagnostic") or reason)
    return dict(result, message="몽땅클립보드를 켜지 못했습니다.", diagnostic=result.get("diagnostic") or reason or "unknown")


class KClipService:
    def __init__(self, client=None, *, installer=kc.install, starter=kc.start, autostart=kc.set_autostart,
                 autostart_status=kc.autostart_enabled,
                 bundled=bundled_exe, installed=kc.installed_exe, excel=None, sleep=time.sleep,
                 command_runner=None, ffi_library=None, version_reader=None, lifecycle_timeout=15,
                 enable_timeout=40):
        self._excel_service = excel
        self._enable_timeout = enable_timeout
        self._enable_job = None
        self._enable_job_lock = threading.Lock()
        self._enable_job_seq = 0
        self._sleep = sleep
        self.client = client or kc.KAgentClient()
        self._injected_client = client is not None
        self._install = installer
        self._start = starter
        self._autostart = autostart
        self._autostart_status = autostart_status
        self._bundled = bundled
        self._installed = installed
        self._version_reader = version_reader or (lambda: self._lifecycle("--version").get("version"))
        self._runner = command_runner or subprocess.run
        self._lifecycle_timeout = lifecycle_timeout
        self._ffi_library = (Path(ffi_library) if ffi_library is not None else
                             Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1])) / "resources" / "kclip_ffi.dll")
        self._pause_file = Path(tempfile.gettempdir()) / f"docfit-kclip-pause-{os.getpid()}.json"

    def _pause_until(self, until=None):
        try:
            data = json.loads(self._pause_file.read_text(encoding="utf-8"))
            expiry = float(data.get("until", 0))
        except (OSError, ValueError, TypeError):
            expiry = 0
        if until is not None:
            expiry = until
            self._pause_file.write_text(json.dumps({"until": expiry}), encoding="utf-8")
        if expiry and expiry <= time.time():
            self._pause_file.unlink(missing_ok=True)
            if self.client.running():
                try:
                    self.client.request("resume")
                except kc.KAgentError:
                    pass
            return 0
        return expiry

    def status(self):
        if self._injected_client:
            expiry = self._pause_until()
            status = self.client.status_file() if self.client.running() else None
            if status is not None and expiry:
                status["paused"] = True
                status["pause_until"] = expiry
            try:
                autostart = bool(self._autostart_status())
            except (OSError, ImportError):
                autostart = False
            folder = getattr(self.client, "folder", None)
            return dict(ok=True, running=bool(status), available=self._bundled() is not None,
                        autostart=autostart, consent=CONSENT, status=status or {},
                        db_path=str(Path(folder) / "clip.db") if folder is not None else None,
                        ffi_library=str(self._ffi_library) if self._ffi_library.is_file() else None)
        result = self._lifecycle("--status")
        if not result.get("ok"):
            return result
        status = result.get("status") or {}
        expiry = self._pause_until()
        if status and expiry:
            status["paused"] = True
            status["pause_until"] = expiry
        folder = getattr(self.client, "folder", None)
        return dict(ok=True, running=bool(result.get("running")), available=bool(result.get("available")),
                    autostart=bool(result.get("autostart")), consent=CONSENT, status=status,
                    db_path=result.get("db_path") or (str(Path(folder) / "clip.db") if folder is not None else None),
                    ffi_library=str(self._ffi_library) if self._ffi_library.is_file() else None)

    def _lifecycle(self, command, *args, timeout=None):
        exe = self._bundled()
        if exe is None:
            return dict(ok=False, code="KCLIP_MISSING", message="이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다.")
        try:
            completed = self._runner([str(exe), command, *args], capture_output=True, text=True, encoding="utf-8",
                                     timeout=timeout if timeout is not None else self._lifecycle_timeout)
            result = json.loads(completed.stdout.strip().splitlines()[-1])
            if completed.returncode or not isinstance(result, dict):
                raise ValueError("invalid lifecycle response")
            return result
        except subprocess.TimeoutExpired as error:
            return dict(ok=False, code="KCLIP_TIMEOUT", message="몽땅클립보드 명령 응답 시간이 초과됐습니다.", diagnostic=str(error))
        except (OSError, ValueError, IndexError) as error:
            return dict(ok=False, code="KCLIP_COMMAND", message=str(error), diagnostic=type(error).__name__)

    def _integrity_ok(self, exe):
        """번들 옆 .sha256 이 있으면 내용과 맞는지 본다(없으면 통과). 켜기 때 한 번만 읽는다."""
        sidecar = exe.with_name(exe.name + ".sha256")
        if not sidecar.exists():
            return True
        import hashlib
        return hashlib.sha256(exe.read_bytes()).hexdigest() == sidecar.read_text(encoding="ascii").strip().lower()

    def _already_current_and_running(self, exe):
        """같은 버전이 설치돼 있고 이미 실행 중이면 True. 이때는 재설치·재시작을 하지 않는다.
        설치본과 번들의 크기가 다르면(같은 버전 재빌드 가능성) 보수적으로 False 로 두어 정식 경로를 탄다."""
        if not self.client.running():
            return False
        running_version = (self.client.status_file() or {}).get("version")
        if not isinstance(running_version, str) or not running_version:
            return False
        bundled_version = self._version_reader()
        if not isinstance(bundled_version, str) or bundled_version.strip() != running_version.strip():
            return False
        try:
            return Path(self._installed()).stat().st_size == Path(exe).stat().st_size
        except OSError:
            return False

    def enable(self, payload, *, _steps=None):
        """동기식 켜기. 결과에 구간별 시간(timings, ms)을 붙이고 로그로 남긴다."""
        steps = _steps or _Steps()
        try:
            result = self._enable_blocking(payload, steps)
        finally:
            _log.info("몽땅클립보드 켜기 구간: %s (합계 %dms)", steps.summary(), steps.elapsed_ms())
        if isinstance(result, dict):
            result = dict(explain_failure(result), timings=steps.table(), elapsed_ms=steps.elapsed_ms())
        return result

    def _enable_blocking(self, payload, steps):
        if payload.get("consent") is not True:
            return dict(ok=False, code="KCLIP_CONSENT", message="안내를 읽고 동의해 주세요.")
        want_autostart = payload.get("autostart", True) is True
        if self._injected_client:
            exe = self._bundled()
            if exe is None:
                return dict(ok=False, code="KCLIP_MISSING", message="이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다.")
            with steps("무결성 확인"):
                if not self._integrity_ok(exe):
                    return dict(ok=False, code="KCLIP_INTEGRITY", message="몽땅클립보드 프로그램 파일이 손상됐습니다. 다시 설치해 주세요.")
            if not self.client.running():
                with steps("설치"):
                    installed = self._install(exe)
                with steps("시작"):
                    self._start(installed)
                with steps("시작 확인"):
                    for _ in range(75):
                        if self.client.running(): break
                        self._sleep(0.02)
                    else:
                        return dict(ok=False, code="KCLIP_START_TIMEOUT", message="몽땅클립보드가 시작되지 않았습니다.", diagnostic="agent_not_running")
            else:
                with steps("버전 확인·필요 시 갱신"):
                    result = self.upgrade_if_stale()
                if not result.get("ok"):
                    return result
            with steps("자동 실행 등록"):
                try: self._autostart(want_autostart)
                except OSError: pass
            with steps("상태 조회"):
                return self.status()
        exe = self._bundled()
        if exe is None:
            return dict(ok=False, code="KCLIP_MISSING", message="이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다.")
        with steps("무결성 확인"):
            if not self._integrity_ok(exe):
                return dict(ok=False, code="KCLIP_INTEGRITY", message="몽땅클립보드 프로그램 파일이 손상됐습니다. 다시 설치해 주세요.")
        with steps("실행 중 버전 확인"):
            try:
                current = self._already_current_and_running(exe)
            except (OSError, ValueError, kc.KAgentError):
                current = False
        if current:
            # 이미 같은 버전이 실행 중: 해시 재계산·재설치·재시작 없이 자동 실행 등록만 맞춘다.
            with steps("자동 실행 등록"):
                try: self._autostart(want_autostart)
                except (OSError, ImportError): pass
            with steps("상태 조회"):
                status = self.status()
            return dict(status, reason="already_running") if status.get("ok") else status
        args = ["--consent"]
        if want_autostart:
            args.append("--autostart")
        with steps("kagent --enable"):
            res = self._lifecycle("--enable", *args, timeout=self._enable_timeout)
        if not res.get("ok"):
            if res.get("code") == "KCLIP_TIMEOUT":
                # 응답은 늦었지만 에이전트는 이미 떠 있을 수 있다: 실제 상태를 한 번 더 확인한다.
                with steps("시간 초과 뒤 상태 재확인"):
                    if self.client.running():
                        status = self.status()
                        if status.get("ok") and status.get("running"):
                            return dict(status, reason="recovered_after_timeout")
            return res
        with steps("시작 확인"):
            for _ in range(75):
                st = self.status()
                if st.get("ok") and st.get("running"):
                    return st
                self._sleep(0.02)
            status = self.status()
        if not status.get("running"):
            return dict(ok=False, code="KCLIP_START_TIMEOUT", message="몽땅클립보드가 시작되지 않았습니다.", diagnostic="agent_not_running")
        return status

    # ---- 비동기 켜기: 요청은 곧바로 '시작 중'을 돌려주고, 화면은 enable_progress 로 짧게 묻는다 ----
    def enable_async(self, payload):
        if payload.get("consent") is not True:
            return dict(ok=False, code="KCLIP_CONSENT", message="안내를 읽고 동의해 주세요.")
        with self._enable_job_lock:
            job = self._enable_job
            if job is not None and job["state"] == "running":
                return self._job_view(job)
            self._enable_job_seq += 1
            job = self._enable_job = dict(id=self._enable_job_seq, state="running", phase="준비 중",
                                           started=time.monotonic(), result=None)
            threading.Thread(target=self._run_enable_job, args=(job, dict(payload)), daemon=True,
                             name="kclip-enable").start()
            return self._job_view(job)

    def _run_enable_job(self, job, payload):
        def phase(name):
            job["phase"] = name
        steps = _Steps(on_phase=phase)
        try:
            result = self.enable(payload, _steps=steps)
        except Exception as error:  # 어떤 오류도 '진행 중'으로 남기지 않는다.
            _log.warning("몽땅클립보드 켜기 작업 실패", exc_info=True)
            result = dict(ok=False, code="KCLIP_ERROR", message="몽땅클립보드를 켜지 못했습니다.",
                          diagnostic=f"{type(error).__name__}: {error}", failed_phase=job.get("phase"))
        if isinstance(result, dict) and result.get("ok") is False:
            result.setdefault("failed_phase", job.get("phase"))
        job["result"] = result
        job["state"] = "done" if isinstance(result, dict) and result.get("ok") is not False else "failed"

    def _job_view(self, job):
        elapsed = int((time.monotonic() - job["started"]) * 1000)
        if job["state"] == "running":
            return dict(ok=True, starting=True, state="running", phase=job["phase"], elapsed_ms=elapsed,
                        running=False, job=job["id"])
        return dict(job["result"], starting=False, state=job["state"], job=job["id"])

    def enable_progress(self):
        job = self._enable_job
        if job is None:
            return dict(ok=True, starting=False, state="idle")
        return self._job_view(job)

    def upgrade_if_stale(self):
        """앱 시작·켜기 때 번들과 실행 중인 버전을 확인한다. 데이터·설정은 건드리지 않는다."""
        with _UPGRADE_LOCK:
            if not self._injected_client:
                result = self._lifecycle("--upgrade-if-stale")
            else:
                try:
                    result = self._upgrade_injected()
                except (OSError, ValueError, kc.KAgentError) as error:
                    result = dict(ok=False, upgraded=False, reason=str(error))
            if not result.get("ok"):
                _log.warning("몽땅클립보드 에이전트 갱신 실패: %s", result)
            return result

    def _wait_running(self, expected):
        for _ in range(150):
            if bool(self.client.running()) == expected:
                return True
            self._sleep(0.02)
        return bool(self.client.running()) == expected

    def _upgrade_injected(self):
        exe = self._bundled()
        if exe is None:
            return dict(ok=True, upgraded=False, reason="no_bundle")
        installed = Path(self._installed())
        was_running = self.client.running()
        if not installed.is_file() and not was_running:
            return dict(ok=True, upgraded=False, reason="not_installed")
        import hashlib
        sidecar = exe.with_name(exe.name + ".sha256")
        current = exe.read_bytes()
        if sidecar.exists() and hashlib.sha256(current).hexdigest() != sidecar.read_text(encoding="ascii").strip().lower():
            return dict(ok=False, upgraded=False, reason="integrity")
        bundled_version = self._version_reader()
        if not isinstance(bundled_version, str) or not bundled_version.strip():
            return dict(ok=False, upgraded=False, reason="version_unavailable")
        old = installed.read_bytes() if installed.is_file() else None
        running_version = (self.client.status_file() or {}).get("version") if was_running else None
        stale = old != current or (was_running and running_version != bundled_version)
        if not stale:
            return dict(ok=True, upgraded=False, reason="current")
        if was_running:
            try:
                self.client.request("quit")
            except kc.KAgentError:
                pass
            if not self._wait_running(False):
                return dict(ok=False, upgraded=False, reason="still_running")
        try:
            target = self._install(exe)
        except (OSError, kc.KAgentError) as error:
            if was_running:
                try:
                    self._start(installed)
                except (OSError, kc.KAgentError):
                    _log.exception("이전 에이전트 재시작 실패")
            return dict(ok=False, upgraded=False, reason=str(error))
        if was_running:
            try:
                self._start(target)
                if not self._wait_running(True):
                    raise kc.KAgentError("agent start timeout")
            except (OSError, kc.KAgentError) as error:
                # 새 프로세스가 응답하지 않으면 종료 확인 없이 덮어쓰지 않는다.
                if self.client.running():
                    try:
                        self.client.request("quit")
                    except kc.KAgentError:
                        pass
                if self._wait_running(False):
                    try:
                        if old is not None:
                            with tempfile.TemporaryDirectory() as folder:
                                backup = Path(folder) / "kagent.exe"
                                backup.write_bytes(old)
                                kc.install(backup, installed.parent)
                            self._start(installed)
                        else:
                            installed.unlink(missing_ok=True)
                    except (OSError, kc.KAgentError):
                        _log.exception("이전 에이전트 복구 실패")
                return dict(ok=False, upgraded=False, reason=str(error))
        return dict(ok=True, upgraded=True, restarted=was_running)

    def disable(self, payload):
        if self._injected_client:
            if self.client.running():
                try: self.client.request("quit")
                except kc.KAgentError: pass
            try: self._autostart(False)
            except OSError: pass
            return self.status()
        return self._lifecycle("--disable")

    def _forward(self, cmd, payload, keys):
        try:
            response = self.client.request(cmd, **{k: payload[k] for k in keys if k in payload})
        except kc.KAgentError as error:
            return dict(ok=False, code="KCLIP_OFFLINE", message=str(error))
        return response

    def _excel(self):
        if self._excel_service is None:
            from .excel_table_service import ExcelTableService
            self._excel_service = ExcelTableService()
        return self._excel_service

    def _table_item(self, item_id):
        """항목이 엑셀 표(HTML Format + 탭/줄 구분 글)인지 본다. (정보, 오류 응답) 반환."""
        try:
            info = self.client.request("get", id=item_id)
        except kc.KAgentError as error:
            return None, dict(ok=False, code="KCLIP_OFFLINE", message=str(error))
        if not isinstance(info, dict) or info.get("ok") is False:
            return None, info if isinstance(info, dict) else dict(ok=False, message="항목을 확인해 주세요.")
        formats = [str(f).lower() for f in (info.get("formats") or [])]
        data = str(info.get("data") or "")
        table = info.get("kind") == "text" and "html format" in formats and "\t" in data
        return dict(info, table=table), None

    def _item_id(self, payload):
        try:
            item_id = int(payload.get("id"))
            if isinstance(payload.get("id"), bool) or item_id <= 0:
                raise ValueError
        except (TypeError, ValueError):
            return None
        return item_id

    def _copy_no_shade(self, payload):
        """표 항목을 클립보드로 되돌린 뒤 셀 배경만 지워 같은 클립보드에 다시 올린다(kagent는 그대로 쓴다)."""
        item_id = self._item_id(payload)
        if item_id is None:
            return dict(ok=False, code="KCLIP_ITEM", message="항목을 확인해 주세요.")
        info, error = self._table_item(item_id)
        if error:
            return error
        if not info["table"]:
            return dict(ok=False, code="KCLIP_NOT_TABLE", message="엑셀 표(HTML 서식 포함) 항목이 아닙니다.")
        restored = self._forward("restore", dict(id=item_id), ("id",))
        if isinstance(restored, dict) and restored.get("ok") is False:
            return restored
        excel = self._excel()
        # 에이전트가 엑셀 표를 '목표 결과'로 되돌렸으면 글도 같은 공백 규칙으로 정리돼 있다(설정 excel_target_paste).
        from docfit_core.excel_paste_target import clean_tsv
        raw = str(info.get("data") or "")
        wants = {_norm_text(raw), _norm_text(clean_tsv(raw))}
        for _ in range(40):  # 되돌리기는 kagent 창 스레드에서 비동기로 일어난다(최대 약 2초).
            html, tsv = excel.clipboard()
            if html and _norm_text(tsv) in wants:
                break
            self._sleep(0.05)
        else:
            return dict(ok=False, code="KCLIP_RESTORE", message="항목을 클립보드로 되돌리지 못해 그대로 두었습니다. 다시 눌러 주세요.")
        return excel.command("strip_shading")

    def command(self, action, payload=None):
        payload = payload if isinstance(payload, dict) else {}
        if action == "is_table":
            item_id = self._item_id(payload)
            if item_id is None:
                return dict(ok=False, code="KCLIP_ITEM", message="항목을 확인해 주세요.")
            info, error = self._table_item(item_id)
            return error or dict(ok=True, table=info["table"])
        if action == "copy_no_shade":
            return self._copy_no_shade(payload)
        if action == "status":
            return self.status()
        if action == "settings_window":
            exe = self._bundled()
            if exe is None:
                return dict(ok=False, code="KCLIP_MISSING", message="몽땅클립보드 설정 창 프로그램을 찾지 못했습니다.")
            try:
                status = self.client.status_file() or {}
                args = [str(exe), "--settings"]
                pipe = status.get("pipe")
                if isinstance(pipe, str) and pipe.startswith(r"\\.\pipe\HwpAutoDocFit.kagent."):
                    args.extend(["--pipe", pipe])
                subprocess.Popen(args, close_fds=True)
                return dict(ok=True)
            except OSError as error:
                return dict(ok=False, code="KCLIP_SETTINGS", message=f"설정 창을 열지 못했습니다: {error}")
        if action == "enable":
            return self.enable_async(payload) if payload.get("async") is True else self.enable(payload)
        if action == "enable_progress":
            return self.enable_progress()
        if action == "disable":
            return self.disable(payload)
        if action == "autostart":
            enabled = payload.get("enabled") is True
            try:
                self._autostart(enabled)
            except OSError as error:
                return dict(ok=False, code="KCLIP_AUTOSTART", message=f"자동 시작 설정을 바꾸지 못했습니다: {error}")
            return self.status()
        forwards = {
            "add_text": ("add_text", ("text",)),
            "add_files": ("add_files", ("paths",)),
            "list": ("list", ("limit", "query")),
            "get": ("get", ("id",)),
            "thumb": ("thumb", ("id", "size")),
            "restore": ("restore", ("id",)),
            "delete": ("delete", ("id",)),
            "pin": ("pin", ("id", "pinned")),
            "clear": ("clear", ("keep_pinned",)),
            "pause": ("pause", ("minutes",)),
            "resume": ("resume", ()),
            "settings_get": ("settings_get", ()),
            "settings_set": ("settings_set", ("settings",)),
        }
        if action == "open_web":
            try:
                item_id = int(payload.get("id"))
            except (TypeError, ValueError):
                return dict(ok=False, code="KCLIP_WEB", message="웹 링크 항목을 확인해 주세요.")
            try:
                result = self.client.request("get", id=item_id)
            except kc.KAgentError as error:
                return dict(ok=False, code="KCLIP_OFFLINE", message=str(error))
            from urllib.parse import urlsplit
            url = str(result.get("data") or "").strip()
            parsed = urlsplit(url)
            if parsed.scheme.lower() not in ("http", "https") or not parsed.netloc:
                return dict(ok=False, code="KCLIP_WEB", message="HTTP 또는 HTTPS 웹 주소가 아닙니다.")
            return dict(ok=bool(webbrowser.open(url)), message="기본 브라우저에서 웹 링크를 열었습니다.")
        if action not in forwards:
            return dict(ok=False, code="NOT_AVAILABLE", message="지원하지 않는 명령입니다.")
        if action == "pause":
            minutes = payload.get("minutes")
            if minutes is not None:
                try:
                    minutes = max(1, min(1440, int(minutes)))
                except (TypeError, ValueError):
                    return dict(ok=False, code="KCLIP_PAUSE", message="일시정지 시간을 확인해 주세요.")
                self._pause_until(time.time() + minutes * 60)
                if not self._injected_client:
                    result = self._lifecycle("--pause", "--minutes", str(minutes))
                    if not result.get("ok"):
                        return result
        elif action == "resume":
            self._pause_file.unlink(missing_ok=True)
            if not self._injected_client:
                result = self._lifecycle("--resume")
                if not result.get("ok"):
                    return result
        cmd, keys = forwards[action]
        if action in ("get", "thumb", "restore", "delete", "pin"):
            try:
                item_id = int(payload.get("id"))
                if isinstance(payload.get("id"), bool) or item_id <= 0:
                    raise ValueError
            except (TypeError, ValueError):
                return dict(ok=False, code="KCLIP_ITEM", message="항목을 확인해 주세요.")
            payload = dict(payload, id=item_id)
        if action == "pin":
            payload = dict(payload, pinned=payload.get("pinned") is not False)
        if action == "clear":
            # 고정 항목은 기본으로 보존한다. 고정까지 지우려면 keep_pinned=False를 명시해야 한다.
            keep_pinned = payload.get("keep_pinned") is not False
            result = self._forward(cmd, dict(payload, keep_pinned=keep_pinned), keys)
            if isinstance(result, dict) and result.get("ok") is not False:
                result = dict(result, keep_pinned=keep_pinned)
                result.setdefault("message", "고정한 항목은 남기고 지웠습니다." if keep_pinned
                                  else "고정한 항목까지 모두 지웠습니다.")
            return result
        return self._forward(cmd, payload, keys)
