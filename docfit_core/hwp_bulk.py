"""HWP → HWPX 일괄 변환(2.0.7): 파일 하나·폴더·로컬 드라이브 전체, 암호·배포용·기관 보안(DRM) 문서.

- 찾기: 시스템 폴더(Windows·Program Files·AppData 등)와 바로가기 연결(정션)은 건너뛴다.
- 결과: 원본 옆에 같은 이름의 .hwpx. 이미 있으면 건너뛰고 원본은 고치지 않는다.
- 암호 문서: 한/글 '문서 암호' 창에 사용자가 준 암호를 차례로 넣는다(창을 띄운 채 멈추지 않음).
  결과 HWPX도 같은 암호로 잠긴다(한/글 기본 동작, 보안 유지).
- 기관 보안(DRM): 이 PC의 문서보안 프로그램이 허용하면 그대로 열려 변환되고, 막히면 이유를 남긴다.
"""
from __future__ import annotations

import csv
import os
import stat
import subprocess
import threading
import time
import json
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

# 드라이브 전체를 찾을 때 들어가지 않는 폴더(소문자).
SKIP_DIRS = frozenset({
    "windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "$windows.~bt",
    "$windows.~ws", "$winreagent", "system volume information", "recovery", "appdata", "msocache",
    "perflogs", "node_modules", ".git", ".svn", "__pycache__", "onedrivetemp", "config.msi",
    # 프로그램 설치 폴더 및 서식 폴더 보강 (TODO 2026-11-15 ⑥)
    "hnc", "hncutils", "hancom", "hancom office", "hancomoffice", "서식", "서식예시", "template", "templates",
})
# 프로그램이 넣은 HWP 모듈 파일 등 기본 건너뛸 파일명
SKIP_FILES = frozenset({
    "hanwp.hwp",
})
OLE_SIGNATURE = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"
HWP3_SIGNATURE = b"HWP Document File"
MAX_FILES = 200_000
HWP_BULK_ACTIONS = frozenset({"drives", "status", "scan_all", "start_all", "start", "stop",
                              "retry_failed", "open_report", "show_report"})

# 결과 종류 분류 (TODO 2026-11-15 ④)
KIND_DRM = "DRM"
KIND_DISTRIBUTED = "배포용"
KIND_PASSWORD = "암호"
KIND_HWP3 = "HWP3"
KIND_CORRUPT = "손상"
KIND_NO_PERMISSION = "쓰기 권한 없음"
KIND_PATH_TOO_LONG = "경로 길이 초과"
KIND_EXISTING = "이미 있음"
KIND_NORMAL = "일반"
KIND_LOCKED = "잠김"
KIND_CLOUD = "클라우드 전용"

MSG_PASSWORD = "암호 문서: 입력한 암호가 맞지 않거나 암호를 넣지 않았습니다. '암호'칸에 문서 암호를 넣고 다시 변환하세요."
MSG_DRM = "이 PC의 보안 프로그램에서 해제한 뒤 다시 변환"
MSG_DISTRIBUTED = "작성자에게 일반 문서를 받거나 한/글에서 직접 열어 보기만 가능"
MSG_SAVE = "한/글이 HWPX 저장을 거부했습니다(배포용 문서처럼 저장이 막힌 문서일 수 있습니다)."
MSG_PERMISSION = "파일을 읽을 수 없거나 대상 폴더에 쓸 권한이 없습니다."
MSG_PATH_TOO_LONG = "파일 경로 길이가 제한을 초과했습니다."
MSG_CORRUPT = "파일이 손상되어 열 수 없습니다."
MSG_HWP3_FAIL = "HWP 3.0 문서: 한/글에서 열지 못했거나 변환할 수 없습니다."
MSG_EXISTING = "이미 HWPX가 있어 건너뛰었습니다."
MSG_LOCKED = "다른 프로그램(한/글 등)이 문서를 사용 중이어서 열 수 없습니다(잠김)."
MSG_CLOUD = "클라우드 전용(온라인 전용) 파일이어서 내려받지 않고 건너뛰었습니다."


@dataclass
class HwpHeaderInfo:
    version: int = 0
    flags: int = 0
    is_compressed: bool = False
    is_encrypted: bool = False
    is_distributed: bool = False
    is_drm: bool = False


def read_hwp5_header(path: str | Path) -> HwpHeaderInfo | None:
    """HWP 5.0 OLE 파일에서 FileHeader 스트림 플래그를 읽는다."""
    try:
        with open(path, "rb") as f:
            data = f.read(1048576)
    except OSError:
        return None

    target = b"HWP Document File"
    pos = 0
    import struct
    while True:
        idx = data.find(target, pos)
        if idx == -1:
            break
        # 64바이트(미니섹터) 또는 512바이트 섹터 경계
        if idx % 64 == 0 and len(data) >= idx + 256:
            buf = data[idx:idx + 256]
            version = struct.unpack_from("<I", buf, 32)[0]
            major = (version >> 24) & 0xFF
            if major >= 5:
                flags = struct.unpack_from("<I", buf, 36)[0]
                return HwpHeaderInfo(
                    version=version,
                    flags=flags,
                    is_compressed=bool(flags & 0x01),
                    is_encrypted=bool(flags & 0x02),
                    is_distributed=bool(flags & 0x04),
                    is_drm=bool(flags & 0x10),
                )
        pos = idx + len(target)
    return None


def is_cloud_placeholder(path: str | Path) -> bool:
    """OneDrive 등 클라우드 전용(온라인 전용, 자리표시자) 파일인지 확인."""
    try:
        p = Path(path)
        info = p.stat(follow_symlinks=False)
    except OSError:
        return False
    attrs = getattr(info, "st_file_attributes", 0)
    # 0x00400000: FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
    # 0x00040000: FILE_ATTRIBUTE_RECALL_ON_OPEN
    # 0x00001000: FILE_ATTRIBUTE_OFFLINE
    if attrs & (0x00400000 | 0x00040000 | 0x00001000):
        return True
    return False


def check_read_error(path: str | Path, exc: Exception | None = None) -> tuple[str, str]:
    """파일 읽기/접근 실패 시 구체적인 종류(kind)와 안내 문구(message)를 판정한다.

    원인 후보 (TODO 2026-11-15 ④): 잠김·권한·클라우드·경로 길이.
    """
    p = Path(path)
    if is_cloud_placeholder(p):
        return KIND_CLOUD, MSG_CLOUD

    if exc is None:
        try:
            with open(p, "rb") as stream:
                stream.read(32)
            return KIND_NORMAL, ""
        except Exception as e:
            exc = e

    winerror = getattr(exc, "winerror", None)
    if winerror in (32, 33):  # ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        return KIND_LOCKED, MSG_LOCKED

    errno_val = getattr(exc, "errno", None)
    import errno
    if errno_val in (errno.EACCES,):
        return KIND_NO_PERMISSION, MSG_PERMISSION
    if errno_val in (errno.EBUSY, getattr(errno, "ETXTBSY", None)):
        return KIND_LOCKED, MSG_LOCKED

    msg = str(exc).lower() if exc else ""
    if "sharing violation" in msg or "locked" in msg or "busy" in msg or "used by another process" in msg:
        return KIND_LOCKED, MSG_LOCKED
    if "permission" in msg or "access is denied" in msg:
        return KIND_NO_PERMISSION, MSG_PERMISSION
    if "cloud" in msg or "offline" in msg or "recall" in msg:
        return KIND_CLOUD, MSG_CLOUD

    return KIND_CORRUPT, MSG_CORRUPT


def classify(path: str | Path) -> str:
    """'hwp'(HWP 5), 'hwp3'(HWP 3.x), 'protected'(DRM 등으로 감싼 파일)."""
    try:
        with open(path, "rb") as stream:
            head = stream.read(32)
    except OSError:
        return "unreadable"
    if head.startswith(OLE_SIGNATURE):
        return "hwp"
    if head.startswith(HWP3_SIGNATURE):
        return "hwp3"
    return "protected"


def target_for(source: str | Path) -> Path:
    return Path(source).with_suffix(".hwpx")


def _skip_dir(entry: os.DirEntry) -> bool:
    name = entry.name.lower()
    if name in SKIP_DIRS or name.startswith("$"):
        return True
    try:
        info = entry.stat(follow_symlinks=False)
    except OSError:
        return True
    attributes = getattr(info, "st_file_attributes", 0)
    # 정션·바로가기 연결(재분석 지점)과 시스템 숨김 폴더는 들어가지 않는다(같은 폴더를 두 번 돌지 않게).
    return bool(entry.is_symlink() or attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
                or (attributes & stat.FILE_ATTRIBUTE_SYSTEM and attributes & stat.FILE_ATTRIBUTE_HIDDEN))


def find_hwp(roots, *, recursive=True, skip_existing=True, stop=None, on_progress=None, limit=MAX_FILES):
    """roots(파일·폴더·드라이브) 안의 .hwp 목록과 건너뛴 수(이미 HWPX 있음)."""
    found, skipped, seen_dirs, last = [], 0, 0, 0.0
    stack = []
    for root in roots:
        path = Path(root)
        if path.is_file():
            if path.suffix.lower() == ".hwp" and not path.is_symlink() and path.name.lower() not in SKIP_FILES:
                if skip_existing and target_for(path).exists():
                    skipped += 1
                else:
                    found.append(path)
        elif path.is_dir():
            stack.append(path)
    while stack and len(found) < limit:
        if stop is not None and stop.is_set():
            break
        folder = stack.pop()
        seen_dirs += 1
        try:
            entries = list(os.scandir(folder))
        except OSError:
            continue
        for entry in sorted(entries, key=lambda e: e.name.lower(), reverse=True):
            try:
                if entry.is_dir(follow_symlinks=False):
                    if recursive and not _skip_dir(entry):
                        stack.append(Path(entry.path))
                elif entry.name.lower().endswith(".hwp") and entry.is_file(follow_symlinks=False):
                    if entry.name.lower() in SKIP_FILES:
                        continue
                    source = Path(entry.path)
                    if skip_existing and target_for(source).exists():
                        skipped += 1
                    else:
                        found.append(source)
            except OSError:
                continue
        if on_progress and time.monotonic() - last > 0.3:
            last = time.monotonic()
            on_progress(dict(phase="scan", folders=seen_dirs, found=len(found), current=str(folder)))
    found.sort(key=lambda p: str(p).lower())
    return found, skipped


def local_drives():
    """고정 디스크 드라이브 목록(예: ['C:\\', 'D:\\']). 네트워크·CD 드라이브는 뺀다."""
    if os.name != "nt":
        return []
    import ctypes
    drives, mask = [], ctypes.windll.kernel32.GetLogicalDrives()
    for index in range(26):
        if mask & (1 << index):
            root = f"{chr(65 + index)}:\\"
            if ctypes.windll.kernel32.GetDriveTypeW(root) == 3:  # DRIVE_FIXED
                drives.append(root)
    return drives


def scan_all(roots, *, runner=None):
    """Use klib scan-hwp to classify every eligible HWP without opening HWP files in COM."""
    from docfit_app.library_bridge import LibraryBridge
    executable = LibraryBridge().executable if runner is None else None
    if runner is None:
        completed = subprocess.run([str(executable), "scan-hwp", *map(str, roots)],
                                   capture_output=True, text=True, encoding="utf-8",
                                   errors="replace", timeout=None,
                                   creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        if completed.returncode:
            raise RuntimeError(completed.stderr.strip() or "klib scan-hwp 스캔에 실패했습니다.")
        lines = completed.stdout.splitlines()
    else:
        lines = runner(roots)
    counts = {"normal": 0, "password": 0, "distributed": 0, "drm": 0,
              "hwp3": 0, "corrupt": 0, "existing": 0}
    rows = []
    for line in lines:
        try:
            row = json.loads(line) if isinstance(line, str) else line
        except (ValueError, TypeError):
            continue
        if not isinstance(row, dict):
            continue
        raw_kind = str(row.get("kind") or "corrupt").casefold()
        kind = "existing" if raw_kind == "existing" else raw_kind
        if kind not in counts:
            kind = "corrupt"
        counts[kind] += 1
        rows.append(row)
    total = sum(counts.values())
    from docfit_core.hwp_bulk_parallel import worker_limit
    workers = worker_limit()
    return {"total": total, "counts": counts, "rows": rows,
            "roots": [str(x) for x in roots],
            "skipped_folders": sorted(SKIP_DIRS),
            "excluded_storage": ["네트워크 드라이브", "클라우드 전용 파일"],
            "workers": workers,
            # COM 저장·검증 개당 2초, 현재 메모리·CPU worker 상한을 반영한 추정치.
            "estimated_seconds": (counts["normal"] * 2 + workers - 1) // workers,
            "estimate_assumption": "정상 문서당 약 2초, 병렬 작업자 수 반영"}


def hwp_pids():
    """실행 중인 한/글(Hwp.exe) 프로세스 번호."""
    if os.name != "nt":
        return set()
    try:
        out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq Hwp.exe", "/FO", "CSV", "/NH"],
                             capture_output=True, text=True, timeout=15,
                             creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)).stdout
    except (OSError, subprocess.SubprocessError):
        return set()
    pids = set()
    for row in csv.reader(out.splitlines()):
        if len(row) > 1 and row[1].isdigit():
            pids.add(int(row[1]))
    return pids


class DialogResponder:
    """변환용 한/글 프로세스(pid)의 '문서 암호' 창에 암호를 차례로 넣고, 저장 확인 창은 기본 단추로 넘긴다.

    사용자의 다른 한/글 창은 건드리지 않는다(pid가 다르다). 확인 창은 열기·저장 중(phase)일 때만 답한다.
    """

    def __init__(self, pid, passwords=()):
        self.pid = pid
        self.passwords = [p for p in passwords if p]
        self.phase = None
        self.current = []
        self.attempts = 0
        self.gave_up = False
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="hwp-dialog-responder", daemon=True)

    def __enter__(self):
        if self.pid:
            self._thread.start()
        return self

    def __exit__(self, *exc):
        self._stop.set()

    def begin(self, phase, passwords=None):
        """phase: 'open'(이번 열기에 넣을 암호 passwords) · 'save' · None(답하지 않음)."""
        self.phase = phase
        if phase == "open":
            self.current = list(passwords or [])
            self.attempts = 0
            self.gave_up = False

    def _dialogs(self):
        import win32gui
        import win32process
        found = []

        def collect(hwnd, _):
            try:
                if (win32process.GetWindowThreadProcessId(hwnd)[1] == self.pid
                        and win32gui.GetClassName(hwnd) == "HNC_DIALOG" and win32gui.IsWindowVisible(hwnd)):
                    found.append((hwnd, win32gui.GetWindowText(hwnd)))
            except Exception:
                pass
            return True
        win32gui.EnumWindows(collect, None)
        return found

    @staticmethod
    def _focus(hwnd):
        import ctypes
        import win32process

        class GuiThreadInfo(ctypes.Structure):
            _fields_ = [("cbSize", ctypes.c_uint32), ("flags", ctypes.c_uint32), ("hwndActive", ctypes.c_void_p),
                        ("hwndFocus", ctypes.c_void_p), ("hwndCapture", ctypes.c_void_p),
                        ("hwndMenuOwner", ctypes.c_void_p), ("hwndMoveSize", ctypes.c_void_p),
                        ("hwndCaret", ctypes.c_void_p), ("rcCaret", ctypes.c_long * 4)]
        info = GuiThreadInfo(cbSize=ctypes.sizeof(GuiThreadInfo))
        thread = win32process.GetWindowThreadProcessId(hwnd)[0]
        if ctypes.windll.user32.GetGUIThreadInfo(thread, ctypes.byref(info)) and info.hwndFocus:
            return info.hwndFocus
        return hwnd

    @staticmethod
    def _key(hwnd, vk):
        import win32con
        import win32gui
        win32gui.PostMessage(hwnd, win32con.WM_KEYDOWN, vk, 0)
        win32gui.PostMessage(hwnd, win32con.WM_KEYUP, vk, 0)

    def _run(self):
        import win32con
        import win32gui
        while not self._stop.wait(0.2):
            if self.phase is None:
                continue
            try:
                dialogs = self._dialogs()
            except Exception:
                continue
            for hwnd, title in dialogs:
                if "암호" in title:
                    target = self._focus(hwnd)
                    if self.phase == "open" and self.attempts < len(self.current):
                        password = self.current[self.attempts]
                        self.attempts += 1
                        for char in password:
                            win32gui.PostMessage(target, win32con.WM_CHAR, ord(char), 0)
                        time.sleep(0.2)
                        self._key(target, win32con.VK_RETURN)
                    else:
                        self.gave_up = True
                        self._key(target, win32con.VK_ESCAPE)
                    time.sleep(0.8)
                    break
                if title == "한글" and self.phase in {"open", "save"}:
                    # 열기: '암호가 틀립니다' 알림, 저장: '암호가 걸린 문서를 이 형식으로 저장할까요?' → 기본 단추(확인·저장).
                    time.sleep(0.2)
                    self._key(hwnd, win32con.VK_RETURN)
                    time.sleep(0.6)
                    break


@dataclass
class BulkResult:
    source: str
    status: str  # success | failed | skipped | protected
    output: str = ""
    message: str = ""
    kind: str = ""  # DRM | 배포용 | 암호 | HWP3 | 손상 | 쓰기 권한 없음 | 경로 길이 초과 | 이미 있음 | 일반


def convert_one(app, source: Path, responder: DialogResponder | None = None) -> BulkResult:
    from docfit_core.hwpx import HwpxLimits, validate_hwpx
    source_str = str(source)

    # 1) 경로 길이 한도 검사 (255자 초과)
    if len(source_str) > 255:
        return BulkResult(source_str, "failed", message=MSG_PATH_TOO_LONG, kind=KIND_PATH_TOO_LONG)

    target = target_for(source)
    if len(str(target)) > 255:
        return BulkResult(source_str, "failed", message=MSG_PATH_TOO_LONG, kind=KIND_PATH_TOO_LONG)

    # 2) 클라우드 전용(온라인 전용) 파일은 내용을 내려받지 않고 건너뜀 (TODO 2026-11-15 ④)
    if is_cloud_placeholder(source):
        return BulkResult(source_str, "skipped", message=MSG_CLOUD, kind=KIND_CLOUD)

    # 3) 이미 HWPX가 있는 경우 건너뜀
    try:
        if target.exists():
            return BulkResult(source_str, "skipped", str(target), MSG_EXISTING, kind=KIND_EXISTING)
    except OSError:
        pass

    # 4) 파일 분류: 한/글 COM을 띄우기 전에 미리 판별
    try:
        kind = classify(source)
    except OSError as exc:
        err_kind, err_msg = check_read_error(source, exc)
        if err_kind == KIND_CLOUD:
            return BulkResult(source_str, "skipped", message=err_msg, kind=err_kind)
        return BulkResult(source_str, "failed", message=err_msg, kind=err_kind)

    if kind == "unreadable":
        err_kind, err_msg = check_read_error(source)
        if err_kind == KIND_CLOUD:
            return BulkResult(source_str, "skipped", message=err_msg, kind=err_kind)
        return BulkResult(source_str, "failed", message=err_msg, kind=err_kind)

    # ① 기관 보안(DRM) 문서는 파일 머리로 미리 판별해 변환 시도 없이 '보안 문서(변환 불가)'로 분류
    if kind == "protected":
        return BulkResult(source_str, "protected", message=MSG_DRM, kind=KIND_DRM)

    # HWP 5.0인 경우 FileHeader 배포용 및 DRM 플래그 미리 확인
    if kind == "hwp":
        info = read_hwp5_header(source)
        if info:
            if info.is_drm:
                return BulkResult(source_str, "protected", message=MSG_DRM, kind=KIND_DRM)
            if info.is_distributed:
                return BulkResult(source_str, "protected", message=MSG_DISTRIBUTED, kind=KIND_DISTRIBUTED)

    temporary = target.with_name(f".~docfit_{os.getpid()}_{target.name}")
    try:
        temporary.unlink(missing_ok=True)
    except OSError:
        return BulkResult(source_str, "failed", message=MSG_PERMISSION, kind=KIND_NO_PERMISSION)

    opened = False
    try:
        opened = True
        fmt = "HWP" if kind != "hwp3" else ""
        # 1) 암호 창 없이 연다(일반·배포용·보안 프로그램이 풀어 주는 DRM 문서).
        ok = app.Open(str(source), fmt, "forceopen:true;suspendpassword:true")
        used_password = False
        if ok is False and kind == "hwp" and responder and responder.pid and responder.passwords:
            # 2) 암호 문서: 암호마다 다시 연다(틀리면 한/글이 열기를 끝내므로 한 번에 하나씩).
            for password in responder.passwords:
                try:
                    app.Clear(1)
                except Exception:
                    pass
                responder.begin("open", [password])
                ok = app.Open(str(source), fmt, "forceopen:true")
                responder.begin(None)
                if ok is not False:
                    used_password = True
                    break
        if ok is False:
            if kind == "hwp":
                return BulkResult(source_str, "failed", message=MSG_PASSWORD, kind=KIND_PASSWORD)
            elif kind == "hwp3":
                return BulkResult(source_str, "failed", message=MSG_HWP3_FAIL, kind=KIND_HWP3)
            return BulkResult(source_str, "failed", message="한/글에서 문서를 열지 못했습니다.", kind=KIND_CORRUPT)

        if responder:
            responder.begin("save")
        saved = app.SaveAs(str(temporary), "HWPX", "")
        if saved is False or not temporary.exists():
            return BulkResult(source_str, "protected", message=MSG_DISTRIBUTED, kind=KIND_DISTRIBUTED)

        # 한/글이 방금 만든 결과 HWPX 검증:
        # 방금 한/글이 생성한 결과물은 HwpxLimits 압축 해제 합계 한도를
        # max(512MB, min(4GB, 원본 HWP 크기 * 20))으로 넓혀 허용하되
        # 압축률·항목 수·경로 안전 검사는 유지한다.
        try:
            source_size = source.stat().st_size
        except OSError:
            source_size = 0
        total_size_limit = max(512 * 1024 * 1024, min(4 * 1024 * 1024 * 1024, source_size * 20))
        max_file_limit = max(128 * 1024 * 1024, min(2 * 1024 * 1024 * 1024, total_size_limit))
        bulk_limits = HwpxLimits(
            max_total_size=total_size_limit,
            max_file_size=max_file_limit,
        )
        validate_hwpx(temporary, limits=bulk_limits, parse_xml=True)
        try:
            os.link(temporary, target)
        except OSError:
            import shutil
            shutil.move(str(temporary), str(target))
        note = ""
        if used_password:
            note = "암호 문서 → 같은 암호의 HWPX"
        kind_result = KIND_PASSWORD if used_password else KIND_NORMAL
        return BulkResult(source_str, "success", str(target), note, kind=kind_result)
    except OSError as exc:
        err_kind, err_msg = check_read_error(source, exc)
        return BulkResult(source_str, "failed", message=err_msg, kind=err_kind)
    except Exception as exc:  # 한/글 COM 오류는 파일 하나만 실패로 남기고 계속한다.
        return BulkResult(source_str, "failed", message=f"변환 오류: {exc}", kind=KIND_CORRUPT)
    finally:
        if responder:
            responder.begin(None)
        if opened:
            try:
                app.Clear(1)  # 저장 묻지 않고 닫는다(원본을 덮어쓰지 않게).
            except Exception:
                pass
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            pass


@dataclass
class BulkState:
    phase: str = "idle"  # idle | scan | convert | done | stopped | error
    found: int = 0
    skipped_existing: int = 0
    protected: int = 0  # 변환 불가(보호 문서) 건너뜀 수
    done: int = 0
    succeeded: int = 0
    failed: int = 0
    current: str = ""
    report: str = ""
    error: str = ""
    failures: list = field(default_factory=list)
    rate_per_minute: float = 0.0
    remaining_seconds: float | None = None
    protected_total: int = 0
    scan_summary: dict = field(default_factory=dict)

    def as_dict(self):
        return dict(phase=self.phase, found=self.found, skipped_existing=self.skipped_existing,
                    protected=self.protected, done=self.done, succeeded=self.succeeded,
                    failed=self.failed, current=self.current, report=self.report,
                    error=self.error, failures=self.failures[-200:],
                    rate_per_minute=self.rate_per_minute, remaining_seconds=self.remaining_seconds,
                    protected_total=self.protected_total, scan_summary=self.scan_summary)


def write_report(results, folder: Path) -> Path:
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"HWPX변환_결과_{datetime.now():%Y%m%d_%H%M%S}.csv"
    with open(path, "w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["결과", "종류", "원본", "HWPX", "설명"])
        for item in results:
            if item.status == "success":
                label = "변환"
            elif item.status == "skipped":
                label = "건너뜀"
            elif item.status == "protected":
                label = "보안 문서(변환 불가)"
            elif item.status == "failed":
                label = "실패"
            else:
                label = item.status
            kind_str = item.kind or (
                "DRM" if item.status == "protected" else ("일반" if item.status == "success" else "기타")
            )
            writer.writerow([label, kind_str, item.source, item.output, item.message])
    return path


def run_bulk(roots, *, app_factory, state: BulkState, stop: threading.Event, passwords=(), skip_existing=True,
             report_dir: Path, recursive=True):
    """찾기 → 변환 → 결과 CSV. state를 계속 고쳐 화면이 진행 상황을 읽는다."""
    state.phase = "scan"

    def scanned(info):
        state.found, state.current = info["found"], info["current"]
    files, state.skipped_existing = find_hwp(roots, recursive=recursive, skip_existing=skip_existing,
                                             stop=stop, on_progress=scanned)
    state.found = len(files)
    results = []
    if not files or stop.is_set():
        state.phase = "stopped" if stop.is_set() else "done"
        return results
    state.phase = "convert"
    before = hwp_pids()
    app = app_factory()
    pid = next(iter(hwp_pids() - before), None)
    try:
        with DialogResponder(pid, passwords) as responder:
            locked_sources = []
            for source in files:
                if stop.is_set():
                    break
                state.current = str(source)
                result = convert_one(app, source, responder)
                results.append(result)
                state.done += 1
                if result.status == "success":
                    state.succeeded += 1
                elif result.status == "protected":
                    state.protected += 1
                elif result.status == "failed":
                    state.failed += 1
                    state.failures.append(dict(source=result.source, message=result.message, kind=result.kind))
                    if result.kind == KIND_LOCKED:
                        locked_sources.append(source)

            # 잠김(다른 프로그램 사용 중) 문서는 일괄 변환 끝에 한 번 자동 재시도 (TODO 2026-11-15 ④)
            if locked_sources and not stop.is_set():
                time.sleep(0.5)
                for source in locked_sources:
                    if stop.is_set():
                        break
                    state.current = f"[재시도] {source}"
                    retry_result = convert_one(app, source, responder)
                    if retry_result.status == "success":
                        for idx, res in enumerate(results):
                            if res.source == retry_result.source:
                                results[idx] = retry_result
                                break
                        state.succeeded += 1
                        state.failed -= 1
                        state.failures = [f for f in state.failures if f.get("source") != retry_result.source]
    finally:
        try:
            app.Quit()
        except Exception:
            pass
    state.report = str(write_report(results, report_dir))
    state.current = ""
    state.phase = "stopped" if stop.is_set() else "done"
    return results


def retry_failures(failures: list, *, app_factory, state: BulkState, stop: threading.Event, passwords=(),
                   report_dir: Path):
    """결과표의 실패 문서 목록으로 '실패만 다시 변환' (TODO 2026-11-15 우선순위)."""
    roots = [Path(f["source"] if isinstance(f, dict) else f) for f in failures]
    return run_bulk(roots, app_factory=app_factory, state=state, stop=stop, passwords=passwords,
                    skip_existing=False, report_dir=report_dir, recursive=False)

