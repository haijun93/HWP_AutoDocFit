"""Windows event bridge for the Rust home buttons."""

FOCUS_HOME_EVENT_NAME = 'Local\\HwpAutoDocFit.FocusHome'
FOCUS_HOME_WINDOW_EVENT = 'docfit-focus-home'


def event_is_available(open_event):
    """Pure predicate used by the event bridge and its unit tests."""
    return bool(open_event(FOCUS_HOME_EVENT_NAME))


def start_focus_home_watcher(window, app_name):
    """Create the named event and forward each signal into the WebView."""
    import ctypes
    from ctypes import wintypes
    import threading

    kernel32 = ctypes.windll.kernel32
    user32 = ctypes.windll.user32
    user32.IsWindowVisible.argtypes = [wintypes.HWND]
    user32.IsWindowVisible.restype = wintypes.BOOL
    user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
    user32.GetWindowTextLengthW.restype = ctypes.c_int
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.GetWindowTextW.restype = ctypes.c_int
    user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.ShowWindow.restype = wintypes.BOOL
    user32.SetForegroundWindow.argtypes = [wintypes.HWND]
    user32.SetForegroundWindow.restype = wintypes.BOOL
    user32.EnumWindows.argtypes = [ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM), wintypes.LPARAM]
    user32.EnumWindows.restype = wintypes.BOOL
    kernel32.CreateEventW.argtypes = [wintypes.LPVOID, wintypes.BOOL, wintypes.BOOL, wintypes.LPCWSTR]
    kernel32.CreateEventW.restype = wintypes.HANDLE
    kernel32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel32.WaitForSingleObject.restype = wintypes.DWORD
    handle = kernel32.CreateEventW(None, False, False, FOCUS_HOME_EVENT_NAME)
    if not handle:
        raise OSError('Could not create home focus event')
    stop = threading.Event()

    def activate():
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            if not user32.IsWindowVisible(hwnd):
                return True
            size = user32.GetWindowTextLengthW(hwnd)
            title = ctypes.create_unicode_buffer(size + 1)
            user32.GetWindowTextW(hwnd, title, size + 1)
            if app_name and app_name in title.value:
                user32.ShowWindow(hwnd, 9)
                user32.SetForegroundWindow(hwnd)
                return False
            return True
        user32.EnumWindows(callback_type(visit), 0)

    def watch():
        while not stop.is_set():
            if kernel32.WaitForSingleObject(handle, 500) == 0:
                try:
                    window.evaluate_js("window.dispatchEvent(new Event('docfit-focus-home'))")
                    activate()
                except Exception:
                    pass

    threading.Thread(target=watch, name='focus-home-event', daemon=True).start()
    stop.handle = handle
    return stop
