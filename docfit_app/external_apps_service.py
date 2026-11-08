"""개발자 모드 외부 프로그램(사진첩·아웃라이너·사진관리) 공용 실행기.

세 프로그램은 본 앱에 이식하지 않은 별도 프로그램(별도 프로세스)이다. 자세한 라이선스·배포 설계는
docs/EXTERNAL_APPS.md.

| app_id        | 메뉴     | 포크(haijun93)     | 원본           | 실행 파일    |
| photo_album   | 사진첩   | mongdangview       | neelabo/NeeView | NeeView.exe |
| outliner      | 아웃라이너 | mongdangoutliner | logseq/logseq   | Logseq.exe  |
| photo_manager | 사진관리 | mondangpics        | KDE/digikam     | digikam.exe |

실행 파일 찾는 순서: ① 앱 폴더의 tools/<포크 이름>/<실행 파일> ② 설정 external_app_paths[app_id]
(사용자가 파일 선택 창으로 지정한 경로) ③ 둘 다 없으면 설치 안내(포크 저장소·원본 공식 배포처).
화면(웹뷰)에서 경로·URL 문자열은 받지 않는다. 실행은 인자 목록 subprocess(shell=False)로만 한다.
"""
from __future__ import annotations

import ntpath
import os
import stat
import subprocess
import sys
import uuid
import webbrowser
from dataclasses import dataclass
from pathlib import Path

from .api_contract import ServiceError
from .external_apps_download import ExtDlError, ExternalAppInstaller

SETTING_KEY = "external_app_paths"
# 파일 선택 창 용도(native_file_dialog_service.PURPOSES에 등록). 이름 검사는 validate_exe가 한다.
DIALOG_PURPOSE = "external_app_exe"
# 파일 속성: 재분석 지점(정션·심볼릭 링크 등).
_REPARSE_POINT = 0x400


@dataclass(frozen=True)
class ExternalApp:
    app_id: str
    label: str
    folder: str          # tools/<folder>/ — 포크 저장소 이름
    exe_name: str
    upstream: str        # 원본 프로그램 이름
    license: str
    fork_url: str
    official_url: str

    def install_hint(self):
        return (f"{self.label} 프로그램({self.upstream})이 설치되어 있지 않습니다. "
                f"앱 폴더의 tools\\{self.folder} 폴더에 {self.exe_name}을(를) 두거나, "
                f"'실행 파일 지정'으로 설치된 {self.exe_name}을(를) 골라 주세요.\n"
                f"받는 곳: 포크 저장소 {self.fork_url} 또는 원본 공식 배포처 {self.official_url}")


APPS = {app.app_id: app for app in (
    ExternalApp("photo_album", "사진첩", "mongdangview", "NeeView.exe", "NeeView", "MIT (7-Zip·SevenZipSharp는 LGPL)",
                "https://github.com/haijun93/mongdangview", "https://github.com/neelabo/NeeView/releases"),
    ExternalApp("outliner", "아웃라이너", "mongdangoutliner", "Logseq.exe", "Logseq", "AGPL-3.0",
                "https://github.com/haijun93/mongdangoutliner", "https://github.com/logseq/logseq/releases"),
    ExternalApp("photo_manager", "사진관리", "mondangpics", "digikam.exe", "digiKam", "GPL-2.0",
                "https://github.com/haijun93/mondangpics", "https://www.digikam.org/download/"),
    ExternalApp("pdf_editor", "PDF편집", "mongdangpdf", "pdfcraft.exe", "mongdangpdf (pdfcraft)", "Apache-2.0 OR MIT",
                "https://github.com/haijun93/mongdangpdf", "https://github.com/storytold/pdfcraft/releases"),
)}


class ExternalAppError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


def app_folders():
    """앱 폴더 후보: 배포판 exe 폴더 → PyInstaller 추출 폴더 → 소스 저장소 루트."""
    folders = []
    if getattr(sys, "frozen", False):
        folders.append(Path(sys.executable).resolve().parent)
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        folders.append(Path(meipass))
    folders.append(Path(__file__).resolve().parents[1])
    return list(dict.fromkeys(folders))


def validate_exe(app, value):
    """app의 실행 파일 경로를 검사해 정규화한 Path를 돌려준다. 맞지 않으면 ExternalAppError.

    - 로컬 드라이브의 절대 경로만(네트워크 경로·장치 경로·대체 데이터 스트림 거절)
    - 파일 이름은 그 프로그램의 실행 파일 이름만(다른 프로그램을 띄우는 통로가 되지 않게)
    - 경로 구성요소에 심볼릭 링크·정션이 없어야 하고, 실제 파일이어야 한다
    """
    invalid = f"{app.label} 실행 파일 경로가 올바르지 않습니다."
    if not isinstance(value, (str, os.PathLike)):
        raise ExternalAppError("APP_INVALID_PATH", invalid)
    text = os.fspath(value)
    if not text or len(text) > 32767 or "\x00" in text:
        raise ExternalAppError("APP_INVALID_PATH", invalid)
    if text.startswith(("\\\\", "//")):
        raise ExternalAppError("APP_INVALID_PATH", f"네트워크 경로의 {app.label} 프로그램은 실행하지 않습니다.")
    _drive, tail = ntpath.splitdrive(text)
    if ":" in tail:
        raise ExternalAppError("APP_INVALID_PATH", invalid)
    path = Path(text)
    if not path.is_absolute():
        raise ExternalAppError("APP_INVALID_PATH", f"{app.label} 실행 파일은 전체 경로로 지정해 주세요.")
    if path.name.casefold() != app.exe_name.casefold():
        raise ExternalAppError("APP_WRONG_EXE", f"{app.label} 실행 파일은 {app.exe_name}이어야 합니다.")
    try:
        for part in (path, *path.parents):
            if part.exists() or part.is_symlink():
                info = part.lstat()
                if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & _REPARSE_POINT:
                    raise ExternalAppError("APP_INVALID_PATH", f"링크·정션을 거치는 경로의 {app.label} 프로그램은 실행하지 않습니다.")
        resolved = path.resolve(strict=True)
    except ExternalAppError:
        raise
    except (OSError, ValueError, RuntimeError):
        raise ExternalAppError("APP_MISSING", app.install_hint()) from None
    if not resolved.is_file():
        raise ExternalAppError("APP_MISSING", app.install_hint())
    return resolved


def default_popen(args, cwd):
    """외부 프로그램을 앱과 분리된 프로세스로 띄운다(앱을 닫아도 남는다). 셸을 거치지 않는다."""
    if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
        raise TypeError("args must be a list of str")
    flags = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
    return subprocess.Popen(args, cwd=cwd, shell=False, close_fds=True, creationflags=flags,
                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


class ExternalAppsService:
    """화면 명령 external_app(action, {"app": app_id, ...}).

    action: list·status·launch·choose(파일 선택 창)·clear(지정 경로 지우기)·open_download({"target": fork|official})
    ·install(없으면 자동 내려받기·설치, 백그라운드)·cancel(설치 취소)·progress(설치 진행 상태).
    "app"에는 app_id 또는 포크 폴더 이름(mongdangview 등)을 쓸 수 있다.
    """

    ACTIONS = ("list", "status", "launch", "choose", "clear", "open_download", "install", "cancel", "progress")

    def __init__(self, settings, *, dialogs=None, folders=app_folders, popen=default_popen,
                 browser=webbrowser.open, installer=None):
        self._settings = settings
        self._dialogs = dialogs
        self._folders = folders
        self._popen = popen
        self._browser = browser
        self._installer = installer

    @property
    def installer(self):
        if self._installer is None:
            self._installer = ExternalAppInstaller()
        return self._installer

    # ── 찾기 ──────────────────────────────────────────────────────────────
    def bundled_exe(self, app):
        for folder in self._folders():
            try:
                return validate_exe(app, Path(folder) / "tools" / app.folder / app.exe_name)
            except ExternalAppError:
                continue
        return None

    def _custom_path(self, app):
        try:
            paths = self._settings.read().get(SETTING_KEY)
        except Exception:
            return None
        value = paths.get(app.app_id) if isinstance(paths, dict) else None
        return value if isinstance(value, str) and value else None

    def find_exe(self, app):
        """(경로, 출처) — 출처는 bundled/custom. 없으면 (None, None)."""
        bundled = self.bundled_exe(app)
        if bundled is not None:
            return bundled, "bundled"
        custom = self._custom_path(app)
        if custom:
            try:
                return validate_exe(app, custom), "custom"
            except ExternalAppError:
                pass
        downloaded = self.installer.installed_exe(app)
        if downloaded is not None:
            try:
                return validate_exe(app, downloaded), "downloaded"
            except ExternalAppError:
                pass
        return None, None

    # ── 명령 ──────────────────────────────────────────────────────────────
    def list_apps(self):
        apps = []
        for item in APPS.values():
            status = self.status(item)
            apps.append(dict(status["app_info"], available=status["available"], source=status["source"]))
        return dict(ok=True, apps=apps)

    def status(self, app, payload=None):
        exe, source = self.find_exe(app)
        custom = self._custom_path(app)
        return dict(ok=True, app=app.app_id, available=exe is not None, source=source,
                    path=str(exe) if exe else None, custom_path=custom,
                    custom_valid=bool(custom) and source == "custom",
                    install_hint=app.install_hint(), downloadable=True,
                    install=self.installer.progress(app),
                    app_info=dict(app=app.app_id, label=app.label, upstream=app.upstream, license=app.license,
                                  tools_folder=f"tools\\{app.folder}", exe_name=app.exe_name,
                                  fork_url=app.fork_url, official_url=app.official_url))

    def launch(self, app, payload=None):
        exe, source = self.find_exe(app)
        if exe is None:
            message = app.install_hint()
            if self._custom_path(app):
                message = f"지정한 {app.label} 실행 파일을 찾을 수 없습니다. 다시 지정해 주세요.\n" + message
            return dict(ok=False, app=app.app_id, code="APP_MISSING", message=message,
                        fork_url=app.fork_url, official_url=app.official_url)
        try:
            process = self._popen([str(exe)], str(exe.parent))
        except OSError as exc:
            return dict(ok=False, app=app.app_id, code="APP_LAUNCH_FAILED",
                        message=f"{app.label} 프로그램을 실행하지 못했습니다: {exc}")
        return dict(ok=True, app=app.app_id, launched=True, source=source, path=str(exe),
                    pid=getattr(process, "pid", None))

    def choose(self, app, payload=None):
        """파일 선택 창으로 실행 파일을 골라 설정에 저장한다(경로는 호스트에서만 다룬다)."""
        if self._dialogs is None:
            return dict(ok=False, app=app.app_id, code="DIALOG_UNAVAILABLE", message="파일 선택 창을 열 수 없습니다.")
        result = self._dialogs.select_files(uuid.uuid4().hex, DIALOG_PURPOSE, "open", False)
        if result.get("cancelled"):
            return dict(self.status(app), cancelled=True)
        picked = self._dialogs.resolve_selection(result["selection_id"], DIALOG_PURPOSE)[0]
        exe = validate_exe(app, picked)

        def save(current):
            paths = current.get(SETTING_KEY)
            paths = dict(paths) if isinstance(paths, dict) else {}
            paths[app.app_id] = str(exe)
            current[SETTING_KEY] = paths
        self._settings.update(save)
        return dict(self.status(app), cancelled=False)

    def clear(self, app, payload=None):
        def drop(current):
            paths = current.get(SETTING_KEY)
            if isinstance(paths, dict):
                paths = {k: v for k, v in paths.items() if k != app.app_id}
                if paths:
                    current[SETTING_KEY] = paths
                else:
                    current.pop(SETTING_KEY, None)
        self._settings.update(drop)
        return self.status(app)

    def open_download(self, app, payload=None):
        # 등록부의 고정 URL만 연다(화면이 보낸 URL은 쓰지 않는다).
        url = app.official_url if payload.get("target") == "official" else app.fork_url
        self._browser(url)
        return dict(ok=True, app=app.app_id, url=url)

    def install(self, app, payload=None):
        """실행 파일이 없으면 출처 목록에서 받아 사용자 폴더에 설치(백그라운드). 있으면 건너뛴다."""
        exe, source = self.find_exe(app)
        if exe is not None:
            return dict(ok=True, app=app.app_id, skipped=True, available=True, source=source, path=str(exe))
        return dict(ok=True, app=app.app_id, skipped=False, install=self.installer.start(app))

    def cancel(self, app, payload=None):
        return dict(ok=True, app=app.app_id, install=self.installer.cancel(app))

    def progress(self, app, payload=None):
        exe, source = self.find_exe(app)
        return dict(ok=True, app=app.app_id, available=exe is not None, source=source,
                    install=self.installer.progress(app))

    def command(self, action, payload=None):
        if action not in self.ACTIONS:
            return dict(ok=False, code="INVALID_ACTION", message="허용되지 않은 외부 프로그램 명령입니다.")
        payload = payload if isinstance(payload, dict) else {}
        if action == "list":
            return self.list_apps()
        name = payload.get("app") if isinstance(payload.get("app"), str) else None
        app = APPS.get(name) or next((a for a in APPS.values() if a.folder == name), None)
        if app is None:
            return dict(ok=False, code="UNKNOWN_APP", message="알 수 없는 외부 프로그램입니다.")
        try:
            return getattr(self, action)(app, payload)
        except ExternalAppError as error:
            return dict(ok=False, app=app.app_id, code=error.code, message=error.message)
        except ExtDlError as error:
            return dict(ok=False, app=app.app_id, code=error.code, message=error.message)
        except ServiceError as error:
            # 파일 선택 창·설정 저장 오류(선택 만료·접근 거부 등).
            return dict(ok=False, app=app.app_id, code=error.code, message=str(error))
