"""문서뷰·사진뷰 공용 파일 작업: 복사(클립보드·폴더로)·폴더 이동·삭제·이름 변경(FILEOPS1).

안전 규칙
- 삭제는 항상 휴지통이다. Windows 가 휴지통으로 보낼 수 없다고 알리면 지우지 않고 실패로 보고한다.
- 어떤 작업도 기존 파일을 덮어쓰지 않는다. 복사·이동에서 같은 이름이 있으면 ' (2)'부터 붙인다.
- 화면의 목록 대상 확장자인 실제 파일만 다룬다(폴더·바로가기·대체 데이터 스트림 제외).
- 결과는 파일별로 보고한다. 하나가 실패해도 나머지는 계속한다.

작업은 별도 스레드에서 돌고 화면은 진행 상태를 물어본다(네이티브 백엔드는 요청을 하나씩 처리한다).
"""
from __future__ import annotations

import errno
import logging
import os
import shutil
import stat
import struct
import sys
import threading
import time
import uuid
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

from docfit_core.log_mask import mask_log_text
from docfit_core.timemachine.rules import cloud_only

logger = logging.getLogger(__name__)

OPERATIONS = ("clipboard", "copy", "move", "delete", "rename")
MAX_FILES = 5000
MAX_NAME_LENGTH = 255
_MAX_SUFFIX_NUMBER = 10000
_INVALID_NAME_CHARS = frozenset('<>:"/\\|?*')
_RESERVED_NAMES = frozenset(
    ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$"]
    + [f"{prefix}{digit}" for prefix in ("COM", "LPT") for digit in "0123456789¹²³"]
)
_DRIVE_FIXED = 3
_ERROR_NOT_SAME_DEVICE = 17
_CF_HDROP = 15
_DROPEFFECT_COPY = 1


class FileOpError(Exception):
    """파일 하나의 작업 실패(화면에 그대로 보여 줄 수 있는 문장)."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


# ---- 검사 ----

def check_source(path: Any, extensions: frozenset[str]) -> Path:
    """화면이 보낸 경로를 그대로 믿지 않고 작업 대상인지 확인한다."""
    if not isinstance(path, str) or not path or "\x00" in path or not os.path.isabs(path):
        raise FileOpError("INVALID_PATH", "파일 경로가 올바르지 않습니다.")
    target = Path(os.path.abspath(path))
    if os.name == "nt" and ":" in target.name:
        raise FileOpError("UNSUPPORTED_TYPE", "목록 대상 파일이 아닙니다.")
    if target.suffix.lower() not in extensions:
        raise FileOpError("UNSUPPORTED_TYPE", "목록 대상 형식이 아니어서 처리하지 않았습니다.")
    if target.is_symlink():
        raise FileOpError("UNSUPPORTED_TYPE", "바로 가기(링크) 파일은 처리하지 않습니다.")
    if not target.is_file():
        raise FileOpError("FILE_NOT_FOUND", "파일을 찾을 수 없습니다.")
    return target


def check_destination(folder: Any) -> Path:
    if not isinstance(folder, str) or not folder.strip() or "\x00" in folder or not os.path.isabs(folder):
        raise FileOpError("INVALID_FOLDER", "대상 폴더 경로가 올바르지 않습니다.")
    target = Path(os.path.abspath(folder))
    if not target.is_dir():
        raise FileOpError("INVALID_FOLDER", "대상 폴더를 찾을 수 없습니다.")
    return target


def renamed_file_name(source: Path, new_name: Any) -> str:
    """입력한 이름에 원래 확장자를 붙인 파일 이름. 확장자는 바꿀 수 없다."""
    if not isinstance(new_name, str) or not new_name.strip():
        raise FileOpError("INVALID_NAME", "새 이름을 입력해 주세요.")
    suffix = source.suffix
    stem = new_name.lstrip()
    if suffix and stem.lower().endswith(suffix.lower()):
        stem = stem[: -len(suffix)]
    if any(char in _INVALID_NAME_CHARS or ord(char) < 32 for char in stem):
        raise FileOpError("INVALID_NAME", '이름에는 \\ / : * ? " < > | 문자를 쓸 수 없습니다.')
    if not stem.strip(" .") or stem != stem.rstrip(" ."):
        raise FileOpError("INVALID_NAME", "이름은 비워 둘 수 없고 마침표나 공백으로 끝날 수 없습니다.")
    if stem.split(".")[0].rstrip(" ").upper() in _RESERVED_NAMES:
        raise FileOpError("INVALID_NAME", "Windows 에서 예약된 이름은 쓸 수 없습니다.")
    name = stem + suffix
    if len(name) > MAX_NAME_LENGTH:
        raise FileOpError("INVALID_NAME", "이름이 너무 깁니다.")
    return name


def _candidates(folder: Path, name: str):
    """`이름.ext`, `이름 (2).ext`, `이름 (3).ext` … 순서."""
    yield folder / name
    stem, suffix = os.path.splitext(name)
    for number in range(2, _MAX_SUFFIX_NUMBER):
        yield folder / f"{stem} ({number}){suffix}"


def _same_folder(first: Path, second: Path) -> bool:
    try:
        return os.path.samefile(first, second)
    except OSError:
        return False


# ---- 덮어쓰지 않는 복사·이동 ----

def _copy_exclusive(source: Path, target: Path) -> None:
    """`target`이 없을 때만 만든다. 이미 있으면 FileExistsError."""
    if os.name == "nt":
        import ctypes
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        copy_file = kernel32.CopyFileW
        copy_file.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_int]
        copy_file.restype = ctypes.c_int
        # CopyFileW 는 수정 시각·속성·대체 데이터 스트림을 보존하고, 실패하면 만들던 파일을 남기지 않는다.
        if not copy_file(str(source), str(target), 1):
            error = ctypes.get_last_error()
            if error in (80, 183):  # ERROR_FILE_EXISTS, ERROR_ALREADY_EXISTS
                raise FileExistsError(errno.EEXIST, "exists", str(target))
            raise ctypes.WinError(error)
        return
    with open(source, "rb") as reader, open(target, "xb") as writer:
        try:
            shutil.copyfileobj(reader, writer)
        except BaseException:
            writer.close()
            os.unlink(target)
            raise
    shutil.copystat(source, target)


def _rename_exclusive(source: Path, target: Path) -> None:
    """같은 볼륨 안에서 덮어쓰지 않고 옮긴다."""
    if os.name == "nt":
        os.rename(source, target)  # Windows 의 rename 은 대상이 있으면 실패한다.
        return
    os.link(source, target)  # POSIX rename 은 덮어쓰므로 link + unlink.
    os.unlink(source)


def copy_into(source: Path, folder: Path) -> Path:
    if cloud_only(getattr(source.stat(), "st_file_attributes", 0)):
        raise FileOpError("CLOUD_ONLY", "클라우드에만 있는 파일입니다. 먼저 내려받은 뒤 복사해 주세요.")
    for target in _candidates(folder, source.name):
        if os.path.lexists(target):
            continue
        try:
            _copy_exclusive(source, target)
            return target
        except FileExistsError:
            continue
    raise FileOpError("NAME_EXHAUSTED", "같은 이름의 파일이 너무 많습니다.")


def move_into(source: Path, folder: Path) -> Path:
    if _same_folder(source.parent, folder):
        raise FileOpError("SAME_FOLDER", "이미 대상 폴더에 있습니다.")
    for target in _candidates(folder, source.name):
        if os.path.lexists(target):
            continue
        try:
            _rename_exclusive(source, target)
            return target
        except FileExistsError:
            continue
        except OSError as exc:
            if exc.errno != errno.EXDEV and getattr(exc, "winerror", None) != _ERROR_NOT_SAME_DEVICE:
                raise
        return _move_across_volumes(source, folder)
    raise FileOpError("NAME_EXHAUSTED", "같은 이름의 파일이 너무 많습니다.")


def _move_across_volumes(source: Path, folder: Path) -> Path:
    """다른 드라이브: 복사가 끝난 뒤에만 원본을 지운다. 원본을 못 지우면 만든 사본을 되돌린다."""
    target = copy_into(source, folder)
    try:
        os.unlink(source)
    except OSError:
        os.chmod(target, stat.S_IREAD | stat.S_IWRITE)  # 읽기 전용 속성까지 복사됐을 수 있다.
        os.unlink(target)
        raise
    return target


def rename_file(source: Path, new_name: Any) -> Path:
    name = renamed_file_name(source, new_name)
    target = source.parent / name
    if name == source.name:
        raise FileOpError("SAME_NAME", "이름이 바뀌지 않았습니다.")
    if os.path.lexists(target):
        # 대소문자만 바꾸는 경우는 같은 파일이라 허용한다.
        if not (name.lower() == source.name.lower() and os.path.samefile(source, target)):
            raise FileOpError("NAME_EXISTS", f"같은 이름의 파일이 이미 있습니다: {name}")
        os.rename(source, target)
        return target
    try:
        _rename_exclusive(source, target)
    except FileExistsError as exc:
        raise FileOpError("NAME_EXISTS", f"같은 이름의 파일이 이미 있습니다: {name}") from exc
    return target


# ---- 휴지통 ----

def recycle_file(path: Path) -> None:
    """파일 하나를 Windows 휴지통으로 보낸다. 휴지통으로 갈 수 없으면 지우지 않는다."""
    if sys.platform != "win32":
        raise FileOpError("RECYCLE_UNAVAILABLE", "휴지통 삭제는 Windows 에서만 할 수 있습니다.")
    import ctypes

    drive = os.path.splitdrive(str(path))[0]
    # 네트워크·이동식 드라이브는 휴지통이 없어 Windows 가 영구 삭제할 수 있다.
    if len(drive) != 2 or ctypes.windll.kernel32.GetDriveTypeW(drive + "\\") != _DRIVE_FIXED:
        raise FileOpError("RECYCLE_UNAVAILABLE", "휴지통을 쓸 수 없는 위치(네트워크·이동식 드라이브)라 삭제하지 않았습니다.")

    import pythoncom
    from win32com.server.policy import DesignatedWrapPolicy
    from win32com.shell import shell, shellcon

    refused: list[bool] = []

    class _RecycleOnlySink(DesignatedWrapPolicy):
        _com_interfaces_ = [shell.IID_IFileOperationProgressSink]
        _public_methods_ = [
            "StartOperations", "FinishOperations", "PreRenameItem", "PostRenameItem", "PreMoveItem",
            "PostMoveItem", "PreCopyItem", "PostCopyItem", "PreDeleteItem", "PostDeleteItem",
            "PreNewItem", "PostNewItem", "UpdateProgress", "ResetTimer", "PauseTimer", "ResumeTimer",
        ]

        def __init__(self):
            self._wrap_(self)

        def PreDeleteItem(self, flags, item):
            # 이 표시가 없으면 Windows 는 영구 삭제하려는 것이다. 예외를 돌려 그 항목을 중단시킨다.
            if not flags & shellcon.TSF_DELETE_RECYCLE_IF_POSSIBLE:
                refused.append(True)
                from win32com.server.exception import COMException
                raise COMException(scode=-2147467260)  # E_ABORT

        def _ignore(self, *args):
            return None

        StartOperations = FinishOperations = PreRenameItem = PostRenameItem = _ignore
        PreMoveItem = PostMoveItem = PreCopyItem = PostCopyItem = PostDeleteItem = _ignore
        PreNewItem = PostNewItem = UpdateProgress = ResetTimer = PauseTimer = ResumeTimer = _ignore

    pythoncom.CoInitialize()
    try:
        operation = pythoncom.CoCreateInstance(
            shell.CLSID_FileOperation, None, pythoncom.CLSCTX_ALL, shell.IID_IFileOperation)
        operation.SetOperationFlags(
            shellcon.FOF_ALLOWUNDO | shellcon.FOF_NOCONFIRMATION | shellcon.FOF_SILENT
            | shellcon.FOF_NOERRORUI | shellcon.FOFX_EARLYFAILURE)
        item = shell.SHCreateItemFromParsingName(str(path), None, shell.IID_IShellItem)
        sink = pythoncom.WrapObject(_RecycleOnlySink(), shell.IID_IFileOperationProgressSink)
        operation.DeleteItem(item, sink)
        failure = None
        try:
            operation.PerformOperations()
            if operation.GetAnyOperationsAborted():
                failure = "작업이 중단되었습니다."
        except pythoncom.com_error as exc:
            failure = str(exc)
        item = operation = sink = None
    finally:
        pythoncom.CoUninitialize()
    if refused:
        raise FileOpError("RECYCLE_UNAVAILABLE", "휴지통으로 보낼 수 없는 파일이라 삭제하지 않았습니다.")
    if os.path.lexists(path):
        raise FileOpError("DELETE_FAILED", "휴지통으로 보내지 못했습니다. 다른 프로그램에서 열려 있는지 확인해 주세요.")
    if failure:
        logger.warning("휴지통 삭제 뒤 오류 표시: %s", mask_log_text(failure))


# ---- 클립보드 ----

def hdrop_payload(paths: Sequence[Path | str]) -> bytes:
    """탐색기에 붙여넣을 수 있는 CF_HDROP(DROPFILES + UTF-16 경로 목록)."""
    header = struct.pack("<IiiII", 20, 0, 0, 0, 1)
    body = "".join(f"{path}\0" for path in paths) + "\0"
    return header + body.encode("utf-16-le")


def set_clipboard_files(paths: Sequence[Path]) -> None:
    if sys.platform != "win32":
        raise FileOpError("CLIPBOARD_UNAVAILABLE", "파일 복사(클립보드)는 Windows 에서 쓸 수 있습니다.")
    import win32clipboard

    effect = win32clipboard.RegisterClipboardFormat("Preferred DropEffect")
    for attempt in range(5):
        try:
            win32clipboard.OpenClipboard()
            break
        except Exception as exc:
            if attempt == 4:
                raise FileOpError("CLIPBOARD_BUSY", "클립보드를 사용할 수 없습니다. 잠시 후 다시 시도해 주세요.") from exc
            time.sleep(0.03)
    try:
        win32clipboard.EmptyClipboard()
        win32clipboard.SetClipboardData(_CF_HDROP, hdrop_payload(paths))
        win32clipboard.SetClipboardData(effect, struct.pack("<I", _DROPEFFECT_COPY))
    finally:
        win32clipboard.CloseClipboard()


# ---- 실행기 ----

def _os_error_message(exc: OSError) -> str:
    if isinstance(exc, PermissionError) or getattr(exc, "winerror", None) in (5, 32, 33):
        return "권한이 없거나 다른 프로그램에서 열려 있습니다."
    return exc.strerror or "작업하지 못했습니다."


def pick_destination_folder() -> dict[str, Any]:
    """웹 화면(WebView)용 대상 폴더 선택창."""
    from .native_file_dialog_service import NativeFileDialogService

    service = NativeFileDialogService()
    try:
        path = service.pick_directory("file_ops_destination")
    finally:
        service.close()
    if path is None:
        return {"ok": True, "cancelled": True}
    return {"ok": True, "path": path}


class FileOps:
    """한 화면의 파일 작업을 한 번에 하나씩 배경 스레드에서 실행한다.

    `after_item(op, source, target, before)`은 성공한 파일마다 불리며(목록·DB·썸네일 캐시 갱신),
    반환한 dict 는 그 파일의 결과에 합쳐진다. `before`는 작업 전 원본의 stat 이다.
    `after_all(op, results)`은 끝에 한 번 불린다.
    """

    def __init__(
        self,
        extensions: frozenset[str],
        *,
        after_item: Callable[[str, Path, Path | None, os.stat_result], dict[str, Any]] | None = None,
        after_all: Callable[[str, list[dict[str, Any]]], None] | None = None,
        recycler: Callable[[Path], None] | None = None,
        clipboard: Callable[[Sequence[Path]], None] | None = None,
    ) -> None:
        self.extensions = extensions
        self.after_item = after_item
        self.after_all = after_all
        self.recycler = recycler or recycle_file
        self.clipboard = clipboard or set_clipboard_files
        self._lock = threading.Lock()
        self._state: dict[str, Any] = {"state": "idle"}
        self._thread: threading.Thread | None = None

    def start(self, payload: dict[str, Any]) -> dict[str, Any]:
        op = payload.get("op")
        paths = payload.get("paths")
        if op not in OPERATIONS:
            return {"ok": False, "code": "INVALID_INPUT", "message": "파일 작업 종류를 확인해 주세요."}
        if (not isinstance(paths, list) or not 1 <= len(paths) <= MAX_FILES
                or not all(isinstance(item, str) for item in paths)):
            return {"ok": False, "code": "INVALID_INPUT", "message": f"파일을 1~{MAX_FILES}개 선택해 주세요."}
        paths = list(dict.fromkeys(paths))
        if op == "rename" and len(paths) != 1:
            return {"ok": False, "code": "INVALID_INPUT", "message": "이름 변경은 파일 하나만 할 수 있습니다."}
        folder = None
        if op in ("copy", "move"):
            try:
                folder = check_destination(payload.get("destination"))
            except FileOpError as exc:
                return {"ok": False, "code": exc.code, "message": exc.message}
        with self._lock:
            if self._state.get("state") == "running":
                return {"ok": False, "code": "BUSY", "message": "이전 파일 작업이 끝난 뒤 다시 시도해 주세요."}
            task_id = uuid.uuid4().hex
            self._state = {"state": "running", "task_id": task_id, "op": op, "done": 0, "total": len(paths)}
            # 데몬이 아니다: 종료 중에도 옮기던 파일은 마저 처리한다.
            self._thread = threading.Thread(
                target=self._run, args=(op, paths, folder, payload.get("new_name")),
                name="docfit-file-ops")
            self._thread.start()
        return {"ok": True, "task_id": task_id, "state": "running", "total": len(paths)}

    def progress(self, task_id: Any = None) -> dict[str, Any]:
        with self._lock:
            if task_id is not None and task_id != self._state.get("task_id"):
                return {"ok": False, "code": "NOT_FOUND", "message": "파일 작업 정보를 찾을 수 없습니다."}
            return {"ok": True, **self._state}

    def wait(self, timeout: float | None = None) -> dict[str, Any]:
        thread = self._thread
        if thread is not None:
            thread.join(timeout)
        return self.progress()

    def _one(self, op: str, source: Path, folder: Path | None, new_name: Any) -> Path | None:
        if op == "copy":
            return copy_into(source, folder)
        if op == "move":
            return move_into(source, folder)
        if op == "rename":
            return rename_file(source, new_name)
        self.recycler(source)
        return None

    def _failure(self, path: str, exc: Exception) -> dict[str, Any]:
        if isinstance(exc, FileOpError):
            code, message = exc.code, exc.message
        elif isinstance(exc, OSError):
            code, message = "IO_ERROR", _os_error_message(exc)
        else:
            code, message = "FAILED", "작업하지 못했습니다."
        logger.warning("파일 작업 실패: %s", mask_log_text(f"{path}: {exc}"))
        return {"path": path, "name": Path(path).name, "ok": False, "code": code, "message": message}

    def _run(self, op: str, paths: list[str], folder: Path | None, new_name: Any) -> None:
        results: list[dict[str, Any]] = []
        try:
            if op == "clipboard":
                results = self._copy_to_clipboard(paths)
                return
            for path in paths:
                try:
                    source = check_source(path, self.extensions)
                    before = source.stat()
                    target = self._one(op, source, folder, new_name)
                except Exception as exc:
                    results.append(self._failure(path, exc))
                else:
                    result = {"path": path, "name": source.name, "ok": True}
                    if target is not None:
                        result.update(new_path=str(target), new_name=target.name)
                    if self.after_item is not None:
                        try:
                            result.update(self.after_item(op, source, target, before))
                        except Exception as exc:
                            logger.warning("파일 작업 뒤 목록 갱신 실패: %s", mask_log_text(exc))
                            result["warning"] = "파일은 처리했지만 목록 정보를 갱신하지 못했습니다."
                    results.append(result)
                with self._lock:
                    self._state["done"] = len(results)
        finally:
            if self.after_all is not None and op != "clipboard":
                try:
                    self.after_all(op, results)
                except Exception as exc:
                    logger.warning("파일 작업 뒤 저장된 목록 갱신 실패: %s", mask_log_text(exc))
            succeeded = sum(1 for item in results if item["ok"])
            with self._lock:
                self._state.update(state="complete", done=len(results), results=results,
                                   succeeded=succeeded, failed=len(results) - succeeded)

    def _copy_to_clipboard(self, paths: list[str]) -> list[dict[str, Any]]:
        results: list[dict[str, Any]] = []
        sources: list[Path] = []
        for path in paths:
            try:
                source = check_source(path, self.extensions)
            except Exception as exc:
                results.append(self._failure(path, exc))
            else:
                sources.append(source)
                results.append({"path": path, "name": source.name, "ok": True})
        if sources:
            try:
                self.clipboard(sources)
            except Exception as exc:
                results = [item if not item["ok"] else self._failure(item["path"], exc) for item in results]
        return results


def handle_command(ops: FileOps, action: str, payload: dict[str, Any]) -> dict[str, Any] | None:
    """문서뷰·사진뷰 `command` 공용 분기. 파일 작업 명령이 아니면 None."""
    if action == "file_op_start":
        return ops.start(payload)
    if action == "file_op_progress":
        return ops.progress(payload.get("task_id"))
    if action == "pick_destination":
        return pick_destination_folder()
    return None
