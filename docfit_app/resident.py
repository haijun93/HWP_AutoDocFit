"""상주 작업의 주인. 두 곳에서만 부른다.

  보통 앱(run_webview)      : `start_app_pipe`·`upgrade_helpers`·`sync_search`
  사이드바만 쓸 때(`--resident`): 화면·Tk·COM 없는 프로세스 하나가 `main` 에서 맡는다. 사이드바가 띄우고,
                               사이드바가 끝나면 함께 끝난다(설치 전 안전 종료가 따로 할 일이 없다).

`--resident` 상태: 시작 → (이미 있으면 끝) → 점검·파이프 → 부모 대기(주기적으로 새 버전 확인) → 부모 종료 → 끝.
맡지 않는 것: 변환 파이프(mdm-ctx 가 없으면 앱을 직접 실행한다), 검색 색인 사전 준비, 하단 Dock, 업데이트 설치
(새 버전은 `update_notice.json` 으로 사이드바에 알리고, 설치는 앱이 묻고 한다).
"""
import json
import logging
import os
from pathlib import Path
import threading
import time

OPTION = '--resident'
MUTEX = 'Local\\HwpAutoDocFit.Resident'
APP_MUTEXES = ('Global\\HWP_AutoDocFit_SingleInstance_Mutex', 'Local\\HWP_AutoDocFit_SingleInstance_Mutex')
NOTICE = 'update_notice.json'
UPDATE_FIRST_SECONDS = 20
UPDATE_EVERY_SECONDS = 6 * 60 * 60

_log = logging.getLogger(__name__)


def start_app_pipe(photo_edit_handler):
    """몽땅클립보드(kagent)가 엑셀 표를 '목표 결과'로 붙일 때 부르는 앱 서비스 파이프. 실패해도 계속한다."""
    try:
        from docfit_app.kagent_app_service import start_background, set_photo_edit_handler
        start_background()
        set_photo_edit_handler(photo_edit_handler)
    except Exception:
        _log.exception("Could not start kagent app service pipe")


def upgrade_helpers():
    """앱을 업데이트한 뒤에도 떠 있는 예전 몽땅클립보드·가상 폴더 도우미를 새 파일로 바꾼다. 시작한 스레드를 돌려준다."""
    def upgrade_kagent():
        try:
            from docfit_app.kclip_service import KClipService
            result = KClipService().upgrade_if_stale()
            if result.get("upgraded") or not result.get("ok", True):
                _log.info("몽땅클립보드 에이전트 점검: %s", result)
        except Exception:
            _log.exception("몽땅클립보드 에이전트 업그레이드 점검 실패")

    def upgrade_kvf():
        from docfit_app.kvf_client import KvfClient
        result = KvfClient().sync()
        if not result.get("ok"):
            _log.warning("가상 폴더 창 점검 실패: %s", result)

    threads = [threading.Thread(target=upgrade_kagent, name="kclip-upgrade", daemon=True),
               threading.Thread(target=upgrade_kvf, name="kvf-upgrade", daemon=True)]
    for thread in threads:
        thread.start()
    return threads


def sync_search(read_settings):
    """몽땅검색(ktm.exe, 기본 켬): 설치·갱신·자동 실행을 설정에 맞춘다. 할 일이 없으면 아무것도 띄우지 않는다."""
    try:
        from docfit_app.ktm_service import SETTING_KEY, sync_in_background
        sync_in_background(bool(read_settings().get(SETTING_KEY, True)))
    except Exception:
        _log.exception("몽땅검색 점검 시작 실패")


def write_notice(folder, state):
    """새 버전이 있으면 알림 파일을 쓰고, 최신이면 지운다. 확인에 실패하면 그대로 둔다."""
    path = Path(folder) / NOTICE
    step = state.get('step')
    if step == 'available' and state.get('latest'):
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_suffix('.tmp')
        temporary.write_text(json.dumps({'latest': str(state['latest'])}), encoding='utf-8')
        temporary.replace(path)
    elif step == 'latest':
        path.unlink(missing_ok=True)


def _read_settings(folder):
    try:
        value = json.loads((Path(folder) / 'settings.json').read_text(encoding='utf-8'))
        return value if isinstance(value, dict) else {}
    except (OSError, ValueError):
        return {}


def check_update(folder, service):
    if not _read_settings(folder).get('check_updates_on_start', True):
        # 확인을 끈 사용자에게 예전 알림을 남기지 않는다.
        (Path(folder) / NOTICE).unlink(missing_ok=True)
        return False
    service.check()
    write_notice(folder, service.state())
    return True


def open_photo_edit():
    from docfit_app import handoff
    try:
        handoff.send(handoff.PHOTO_PASTE_MENU, photo_paste=True)
    except Exception:
        _log.exception("붙임사진 편집 창을 열지 못했습니다")


class _Win32:
    def __init__(self):
        import ctypes
        from ctypes import wintypes
        self.ctypes = ctypes
        k32 = self.k32 = ctypes.WinDLL('kernel32', use_last_error=True)
        k32.CreateMutexW.argtypes = (ctypes.c_void_p, wintypes.BOOL, wintypes.LPCWSTR)
        k32.CreateMutexW.restype = wintypes.HANDLE
        k32.OpenMutexW.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.LPCWSTR)
        k32.OpenMutexW.restype = wintypes.HANDLE
        k32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        k32.OpenProcess.restype = wintypes.HANDLE
        k32.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
        k32.WaitForSingleObject.restype = wintypes.DWORD
        k32.CloseHandle.argtypes = (wintypes.HANDLE,)

    def claim(self, name):
        handle = self.k32.CreateMutexW(None, False, name)
        if not handle or self.ctypes.get_last_error() == 183:
            return False
        self.mutex = handle  # 프로세스가 끝날 때까지 쥔다.
        return True

    def mutex_exists(self, name):
        handle = self.k32.OpenMutexW(0x00100000, False, name)  # SYNCHRONIZE
        if handle:
            self.k32.CloseHandle(handle)
        return bool(handle)

    def open_parent(self, pid):
        return self.k32.OpenProcess(0x00100000, False, pid) or None

    def wait(self, handle, seconds):
        """부모가 끝났으면 True."""
        return self.k32.WaitForSingleObject(handle, int(seconds * 1000)) == 0


def parse(arguments):
    """`--resident --parent <pid>` 의 PID. 형식이 틀리면 ValueError."""
    values = list(arguments or [])
    if len(values) != 3 or values[0] != OPTION or values[1] != '--parent' or not values[2].isdigit() or not int(values[2]):
        raise ValueError(f'{OPTION} 인자를 확인해 주세요.')
    return int(values[2])


def main(arguments, version, *, api=None, folder=None, service_factory=None, clock=time.monotonic):
    try:
        parent_pid = parse(arguments)
    except ValueError:
        return 2
    api = api or _Win32()
    parent = api.open_parent(parent_pid)
    if parent is None or not api.claim(MUTEX):
        return 0
    if folder is None:
        from docfit_app.sidebar_launch import _data_folder
        folder = _data_folder()
    folder = Path(folder)
    threads = []
    # 보통 앱이 떠 있으면 점검은 그 앱이 이미 했다.
    if not any(api.mutex_exists(name) for name in APP_MUTEXES):
        try:
            from docfit_app.shell_menu import refresh_registered
            refresh_registered()
        except OSError:
            _log.warning("우클릭 메뉴 갱신 실패", exc_info=True)
        threads = upgrade_helpers()
        sync_search(lambda: _read_settings(folder))
    start_app_pipe(open_photo_edit)

    if service_factory is None:
        from docfit_app.update_service import UpdateService
        service_factory = lambda: UpdateService(version)
    next_check = clock() + UPDATE_FIRST_SECONDS
    checker = None
    while not api.wait(parent, max(1.0, next_check - clock())):
        if clock() < next_check:
            continue
        next_check = clock() + UPDATE_EVERY_SECONDS
        if checker is None or not checker.is_alive():
            def work():
                try:
                    check_update(folder, service_factory())
                except Exception:
                    _log.warning("새 버전 확인 실패", exc_info=True)
            checker = threading.Thread(target=work, daemon=True, name="resident-update-check")
            checker.start()
    # 도우미를 바꾸는 도중에 끊지 않는다.
    deadline = clock() + 30
    for thread in threads:
        thread.join(max(0.0, deadline - clock()))
    return 0
