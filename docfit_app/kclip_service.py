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


class KClipService:
    def __init__(self, client=None, *, installer=kc.install, starter=kc.start, autostart=kc.set_autostart,
                 autostart_status=kc.autostart_enabled,
                 bundled=bundled_exe, installed=kc.installed_exe, excel=None, sleep=time.sleep,
                 command_runner=None, ffi_library=None, version_reader=None):
        self._excel_service = excel
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

    def _lifecycle(self, command, *args):
        exe = self._bundled()
        if exe is None:
            return dict(ok=False, code="KCLIP_MISSING", message="이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다.")
        try:
            completed = self._runner([str(exe), command, *args], capture_output=True, text=True, encoding="utf-8", timeout=20)
            result = json.loads(completed.stdout.strip().splitlines()[-1])
            if completed.returncode or not isinstance(result, dict):
                raise ValueError("invalid lifecycle response")
            return result
        except (OSError, ValueError, IndexError, subprocess.TimeoutExpired) as error:
            return dict(ok=False, code="KCLIP_COMMAND", message=str(error))

    def enable(self, payload):
        if payload.get("consent") is not True:
            return dict(ok=False, code="KCLIP_CONSENT", message="안내를 읽고 동의해 주세요.")
        if self._injected_client:
            exe = self._bundled()
            if exe is None:
                return dict(ok=False, code="KCLIP_MISSING", message="이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다.")
            sidecar = exe.with_name(exe.name + ".sha256")
            if sidecar.exists():
                import hashlib
                if hashlib.sha256(exe.read_bytes()).hexdigest() != sidecar.read_text(encoding="ascii").strip().lower():
                    return dict(ok=False, code="KCLIP_INTEGRITY", message="몽땅클립보드 프로그램 파일이 손상됐습니다. 다시 설치해 주세요.")
            if not self.client.running():
                installed = self._install(exe); self._start(installed)
                for _ in range(100):
                    if self.client.running(): break
                    self._sleep(0.02)
            else:
                result = self.upgrade_if_stale()
                if not result.get("ok"):
                    return result
            try: self._autostart(payload.get("autostart", True) is True)
            except OSError: pass
            return self.status()
        args = ["--consent"]
        if payload.get("autostart", True) is True:
            args.append("--autostart")
        return self._lifecycle("--enable", *args)

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
        if action == "enable":
            return self.enable(payload)
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
        if action in ("get", "restore", "delete", "pin"):
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
