"""메뉴 하나만 담은 앱 창(`--screen N`). 사이드바(sidebar.exe)가 Rust 창이 없는 메뉴를 열 때 쓴다.

이 프로세스의 상태는 하나다: 보통 앱(current() 가 None) 또는 메뉴 N 전용 창. 시작할 때 한 번 정하고 바꾸지 않는다.
환경변수로 전하지 않는다(변환 엔진 같은 자식 프로세스가 물려받으면 안 된다).

같은 앱을 메뉴마다 여러 번 띄우므로 여기서 갈라 놓는 것:
  - 단일 인스턴스: 보통 앱의 뮤텍스 대신 메뉴별 뮤텍스(`claim`)
  - 설정 저장: 시작 때 값에서 바뀐 항목만 쓴다(`remember`·`changed_only`) → 다른 창이 바꾼 값을 되돌리지 않는다
  - 작업 상태 파일: 메뉴별 이름(`job_state_name`)
"""
import os
import time

OPTION = '--screen'

# 사이드바 메뉴 이름. 번호는 앱 탭 번호. dock/src/sidebar.rs 의 label 과 같아야 한다.
# 7(몽땅검색)은 앱 화면이 아니라 ktm.exe 가 맡는다.
SCREENS = {
    0: '문서편집', 1: '표템플릿', 2: '표그림판', 3: '다이어그램', 4: '마인드맵', 5: '붙임사진 편집',
    6: '파일명 일괄변경', 8: '문서뷰', 9: '사진뷰', 10: '파일변환', 11: 'PDF 편집', 12: '가상 폴더',
    13: '업무자료함', 14: '아웃라이너', 15: '검색 벤치마크',
}

_current = None
_baseline = None
_mutex = None


def current():
    return _current


def parse(arguments):
    """`--screen N` 이 없으면 None, 번호가 틀리면 ValueError(전체 앱을 대신 열지 않는다)."""
    values = list(arguments or [])
    if not values or values[0] != OPTION:
        return None
    if len(values) != 2 or not (values[1].isascii() and values[1].isdigit()) or int(values[1]) not in SCREENS:
        raise ValueError(f'{OPTION} 번호를 확인해 주세요.')
    return int(values[1])


def activate(screen):
    global _current, _baseline
    _current = screen
    _baseline = None


def window_title(screen):
    """창 제목. 사이드바가 이 제목으로 창을 찾는다. 앱 본 창 검색어(앱 이름·"한글편집")는 넣지 않는다."""
    return f'{SCREENS[screen]} — DocFit'


def job_state_name():
    return 'desktop-jobs.json' if _current is None else f'desktop-jobs-screen-{_current}.json'


def remember(settings):
    """이 창이 처음 읽은 설정 값을 기억한다(첫 호출만)."""
    global _baseline
    if _current is not None and _baseline is None:
        _baseline = dict(settings)


def changed_only(settings):
    """전용 창에서는 이 창이 바꾼 항목만 돌려준다. 보통 앱에서는 그대로."""
    if _current is None or _baseline is None:
        return settings
    missing = object()
    patch = {key: value for key, value in settings.items() if _baseline.get(key, missing) != value}
    _baseline.update(patch)
    return patch


class _Win32:
    ALREADY_EXISTS = 183
    ACQUIRED = (0x00000000, 0x00000080)  # WAIT_OBJECT_0 / WAIT_ABANDONED

    def __init__(self):
        import ctypes
        from ctypes import wintypes
        self.ctypes = ctypes
        self.kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        self.user32 = ctypes.WinDLL('user32', use_last_error=True)
        self.kernel32.CreateMutexW.argtypes = (ctypes.c_void_p, wintypes.BOOL, wintypes.LPCWSTR)
        self.kernel32.CreateMutexW.restype = wintypes.HANDLE
        self.kernel32.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
        self.kernel32.WaitForSingleObject.restype = wintypes.DWORD
        self.kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        self.user32.FindWindowW.argtypes = (wintypes.LPCWSTR, wintypes.LPCWSTR)
        self.user32.FindWindowW.restype = wintypes.HWND
        self.user32.IsWindowVisible.argtypes = (wintypes.HWND,)
        self.user32.IsIconic.argtypes = (wintypes.HWND,)
        self.user32.ShowWindow.argtypes = (wintypes.HWND, ctypes.c_int)
        self.user32.SetForegroundWindow.argtypes = (wintypes.HWND,)

    def create_mutex(self, name):
        handle = self.kernel32.CreateMutexW(None, True, name)
        return handle, self.ctypes.get_last_error() == self.ALREADY_EXISTS

    def try_acquire(self, handle):
        return self.kernel32.WaitForSingleObject(handle, 0) in self.ACQUIRED

    def close(self, handle):
        self.kernel32.CloseHandle(handle)

    def activate_window(self, title):
        hwnd = self.user32.FindWindowW(None, title)
        if not hwnd or not self.user32.IsWindowVisible(hwnd):
            return False
        if self.user32.IsIconic(hwnd):
            self.user32.ShowWindow(hwnd, 9)  # SW_RESTORE
        self.user32.SetForegroundWindow(hwnd)
        return True


def claim(api=None, wait_seconds=60.0, sleep=time.sleep, clock=time.monotonic):
    """이 메뉴 창을 맡으면 True. 이미 떠 있으면 그 창을 앞으로 가져오고 False.

    먼저 뜬 쪽이 시작 중이면 창이 보일 때까지, 닫히는 중이면 뮤텍스를 넘겨받을 때까지 기다린다.
    """
    global _mutex
    if api is None:
        if os.name != 'nt':
            return True
        api = _Win32()
    handle, exists = api.create_mutex(f'Local\\HwpAutoDocFit.Screen.{_current}')
    if not handle:
        return True
    if not exists:
        _mutex = handle
        return True
    deadline = clock() + wait_seconds
    while True:
        if api.activate_window(window_title(_current)):
            break
        if api.try_acquire(handle):
            _mutex = handle
            return True
        if clock() >= deadline:
            break
        sleep(0.1)
    api.close(handle)
    return False
