"""사이드바 실행 → 조작 가능 시간, 파일 탐색기 새 창 시간, 메뉴별 창이 뜨는 시간을 같은 방법으로 잰다(RUSTSIDE1).

조작 가능 = 새 최상위 창이 보이고(가려지지 않음) 메시지(WM_NULL)에 응답한 첫 시각. 모든 대상에 같은 판정을 쓴다.
메뉴 창은 사이드바에 그 메뉴 누름(WM_COMMAND)을 보낸 때부터 잰다. 앱 화면 창은 '창이 뜬' 시각이며 화면 내용이
다 그려진 시각은 아니다. 이어서 같은 메뉴를 한 번 더 눌러 창이 늘지 않는지, 사이드바를 닫아도 창이 남는지 본다.

사용자 설정은 건드리지 않는다: APPDATA·LOCALAPPDATA·USERPROFILE 을 임시 폴더로 바꿔 실행하고,
이 스크립트가 만든 창만 닫는다. 파일 캐시를 비우지 않으므로 '따뜻한' 실행 값이다. 몽땅검색(상주 프로그램)은 열지 않는다.

사용: python scripts/measure_sidebar.py --bin <Rust exe 폴더> [--menus 16,8,9] [--python <pythonw.exe>]
      [--runs 15] [--lock <시험 잠금 파일>] [--shots <그림 폴더>] [--json 결과.json]
"""
import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time
import uuid

ENUM_PROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM) if os.name == 'nt' else None
WM_CLOSE = 0x0010
WM_COMMAND = 0x0111
SMTO_ABORTIFHUNG = 0x0002
DWMWA_CLOAKED = 14
SIDEBAR_CLASS = 'DocFitSidebar'
MENU_COMMAND_BASE = 200
ROOT = Path(__file__).resolve().parents[1]
NATIVE_CLASSES = {6: 'DocFitRenameWin2', 8: 'DocViewWinMain', 9: 'PhotoViewWinMain', 10: 'ConvertWinStage1',
                  14: 'OutlinerWinMain', 16: 'SettingsWinMain'}


def summarize(values):
    """밀리초 목록의 횟수·최소·중앙값·최대."""
    if not values:
        return {'count': 0, 'min': None, 'median': None, 'max': None}
    return {'count': len(values), 'min': round(min(values), 1), 'median': round(statistics.median(values), 1),
            'max': round(max(values), 1)}


def screen_titles():
    sys.path.insert(0, str(ROOT))
    from docfit_app import screen_mode
    return {number: screen_mode.window_title(number) for number in screen_mode.SCREENS}


def _text(function, hwnd):
    buffer = ctypes.create_unicode_buffer(512)
    function(hwnd, buffer, len(buffer))
    return buffer.value


def windows():
    """보이는 최상위 창 (hwnd, 클래스, 제목)."""
    user32 = ctypes.windll.user32
    found = []

    def visit(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            found.append((hwnd, _text(user32.GetClassNameW, hwnd), _text(user32.GetWindowTextW, hwnd)))
        return True

    user32.EnumWindows(ENUM_PROC(visit), 0)
    return found


def menu_windows(menu, titles):
    return [h for h, cls, title in windows()
            if cls == NATIVE_CLASSES.get(menu) or title == titles.get(menu)]


def process_of(hwnd):
    pid = wintypes.DWORD(0)
    ctypes.windll.user32.GetWindowThreadProcessId(wintypes.HWND(hwnd), ctypes.byref(pid))
    return pid.value


def usable(hwnd):
    cloaked = wintypes.DWORD(0)
    ctypes.windll.dwmapi.DwmGetWindowAttribute(wintypes.HWND(hwnd), DWMWA_CLOAKED, ctypes.byref(cloaked),
                                               ctypes.sizeof(cloaked))
    result = ctypes.c_size_t(0)
    return not cloaked.value and bool(ctypes.windll.user32.SendMessageTimeoutW(
        wintypes.HWND(hwnd), 0, 0, 0, SMTO_ABORTIFHUNG, 2000, ctypes.byref(result)))


def wait_for(probe, timeout=30.0):
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        value = probe()
        if value:
            return value
        time.sleep(0.001)
    return None


def close(hwnd):
    user32 = ctypes.windll.user32
    user32.PostMessageW(wintypes.HWND(hwnd), WM_CLOSE, 0, 0)
    return bool(wait_for(lambda: not user32.IsWindow(wintypes.HWND(hwnd)), 15.0))


def screenshot(hwnd, path):
    """가려져 있어도 그 창만 담는다(PrintWindow, 전체 화면은 찍지 않는다)."""
    from PIL import Image
    user32, gdi32 = ctypes.windll.user32, ctypes.windll.gdi32
    for function in (user32.GetWindowDC, gdi32.CreateCompatibleDC, gdi32.CreateCompatibleBitmap, gdi32.SelectObject):
        function.restype = ctypes.c_void_p
    rect = wintypes.RECT()
    user32.GetWindowRect(wintypes.HWND(hwnd), ctypes.byref(rect))
    width, height = rect.right - rect.left, rect.bottom - rect.top
    window_dc = ctypes.c_void_p(user32.GetWindowDC(wintypes.HWND(hwnd)))
    memory = ctypes.c_void_p(gdi32.CreateCompatibleDC(window_dc))
    bitmap = ctypes.c_void_p(gdi32.CreateCompatibleBitmap(window_dc, width, height))
    gdi32.SelectObject(memory, bitmap)
    user32.PrintWindow(wintypes.HWND(hwnd), memory, 2)

    class Header(ctypes.Structure):
        _fields_ = [('size', wintypes.DWORD), ('width', wintypes.LONG), ('height', wintypes.LONG),
                    ('planes', wintypes.WORD), ('bits', wintypes.WORD), ('compression', wintypes.DWORD),
                    ('image', wintypes.DWORD), ('x', wintypes.LONG), ('y', wintypes.LONG),
                    ('used', wintypes.DWORD), ('important', wintypes.DWORD)]

    header = Header(ctypes.sizeof(Header), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = ctypes.create_string_buffer(width * height * 4)
    gdi32.GetDIBits(memory, bitmap, 0, height, pixels, ctypes.byref(header), 0)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(memory)
    user32.ReleaseDC(wintypes.HWND(hwnd), window_dc)
    Image.frombuffer('RGB', (width, height), pixels, 'raw', 'BGRX', 0, 1).save(path)


class Sandbox:
    """사용자 폴더를 대신하는 임시 폴더."""

    def __init__(self, folder):
        self.root = Path(folder)
        self.appdata = self.root / 'roaming'
        self.settings = self.appdata / 'HwpAutoDocFit' / 'settings.json'
        self.state = self.root / 'sidebar-state.json'
        self.settings.parent.mkdir(parents=True)
        (self.root / 'local').mkdir()
        (self.root / 'home').mkdir()
        # legacy 검색: 색인 서비스를 띄우지 않는다. 업데이트 확인도 끈다.
        self.settings.write_text(json.dumps({'search_engine': 'legacy', 'check_updates_on_start': False}),
                                 encoding='utf-8')
        self.env = {**os.environ, 'APPDATA': str(self.appdata), 'LOCALAPPDATA': str(self.root / 'local'),
                    'USERPROFILE': str(self.root / 'home'), 'RENAME_WIN_DATA': str(self.root / 'rename')}

    def sidebar_command(self, args):
        command = [str(args.bin / 'sidebar.exe'), '--settings', str(self.settings), '--resources', str(args.bin),
                   '--state-file', str(self.state)]
        if args.python:
            command += ['--python', str(args.python), '--source-root', str(ROOT)]
        return command


def start_sidebar(sandbox, args):
    start = time.perf_counter()
    process = subprocess.Popen(sandbox.sidebar_command(args), env=sandbox.env)
    hwnd = wait_for(lambda: next((h for h, cls, _ in windows()
                                  if cls == SIDEBAR_CLASS and process_of(h) == process.pid and usable(h)), None))
    elapsed = (time.perf_counter() - start) * 1000
    if not hwnd:
        process.kill()
        raise RuntimeError('사이드바가 뜨지 않았습니다')
    return process, hwnd, elapsed


def measure_explorer(sandbox):
    name = 'docfit-measure-' + uuid.uuid4().hex[:12]
    folder = sandbox.root / name
    folder.mkdir()
    explorer = lambda: [(h, t) for h, cls, t in windows() if cls == 'CabinetWClass']
    before = {h for h, _ in explorer()}
    start = time.perf_counter()
    subprocess.Popen(['explorer.exe', str(folder)])
    first = wait_for(lambda: next((h for h, _ in explorer() if h not in before and usable(h)), None))
    usable_ms = (time.perf_counter() - start) * 1000
    if not first:
        raise RuntimeError('탐색기 새 창을 찾지 못했습니다(탭으로 열렸을 수 있음)')
    ours = wait_for(lambda: next((h for h, t in explorer() if h not in before and name in t), None))
    folder_ms = (time.perf_counter() - start) * 1000
    if not ours:
        raise RuntimeError('연 폴더의 탐색기 창을 확인하지 못해 닫지 않았습니다')
    close(ours)
    return {'usable_ms': usable_ms, 'folder_ms': folder_ms}


def click(sidebar, menu):
    ctypes.windll.user32.PostMessageW(wintypes.HWND(sidebar), WM_COMMAND, MENU_COMMAND_BASE + menu, 0)


def measure_menu(sandbox, args, menu, titles, shot):
    if menu_windows(menu, titles):
        return {'skipped': '같은 창이 이미 떠 있어 건드리지 않음'}
    process, sidebar, _ = start_sidebar(sandbox, args)
    hwnd = None
    try:
        start = time.perf_counter()
        click(sidebar, menu)
        hwnd = wait_for(lambda: next((h for h in menu_windows(menu, titles) if usable(h)), None), 60.0)
        row = {'window_ms': round((time.perf_counter() - start) * 1000, 1) if hwnd else None}
        if not hwnd:
            return row
        time.sleep(args.settle)
        click(sidebar, menu)  # 두 번째 누름: 새 창 없이 앞으로
        time.sleep(2.0)
        row['windows_after_second_click'] = len(menu_windows(menu, titles))
        try:
            row['state'] = json.loads(sandbox.state.read_text(encoding='utf-8'))['menus'].get(str(menu))
        except (OSError, ValueError, KeyError):
            row['state'] = None
        if shot:
            screenshot(hwnd, shot)
        close(sidebar)
        process.wait(timeout=10)
        time.sleep(0.5)
        row['window_survives_sidebar_close'] = bool(ctypes.windll.user32.IsWindow(wintypes.HWND(hwnd)))
        return row
    finally:
        if hwnd and ctypes.windll.user32.IsWindow(wintypes.HWND(hwnd)):
            pid = process_of(hwnd)
            if not close(hwnd):
                subprocess.run(['taskkill', '/PID', str(pid), '/T', '/F'], capture_output=True)
        if process.poll() is None:
            process.kill()
            process.wait()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--bin', required=True, type=Path)
    parser.add_argument('--runs', type=int, default=15, help='사이드바·탐색기 반복 횟수')
    parser.add_argument('--menus', default='', help='창 시간을 잴 메뉴 번호(쉼표)')
    parser.add_argument('--menu-runs', type=int, default=3)
    parser.add_argument('--python', type=Path, help='앱 화면 메뉴를 소스에서 열 때의 pythonw.exe')
    parser.add_argument('--settle', type=float, default=1.0, help='창이 뜬 뒤 두 번째 누름까지 초')
    parser.add_argument('--lock', type=Path, help='이 파일이 있으면 재지 않는다(전체 시험 진행 중)')
    parser.add_argument('--shots', type=Path)
    parser.add_argument('--json', type=Path)
    args = parser.parse_args(argv)
    if os.name != 'nt':
        parser.error('Windows 전용입니다')
    locked = lambda: args.lock is not None and args.lock.exists()
    if locked():
        print('시험 잠금 파일이 있어 재지 않았습니다.')
        return 3
    args.bin = args.bin.resolve()
    if not (args.bin / 'sidebar.exe').is_file():
        parser.error('--bin 에 sidebar.exe 가 없습니다')
    ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    menus = [int(value) for value in args.menus.split(',') if value]
    titles = screen_titles()
    if args.shots:
        args.shots.mkdir(parents=True, exist_ok=True)
    result = {'sidebar': [], 'explorer': [], 'menus': {}}
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as folder:
        sandbox = Sandbox(folder)
        # 번갈아 재서 PC 부하가 한쪽에만 쏠리지 않게 한다.
        for index in range(args.runs):
            if locked():
                print('측정 중 시험 잠금 파일이 생겨 중단했습니다.')
                return 3
            process, hwnd, elapsed = start_sidebar(sandbox, args)
            close(hwnd)
            process.wait(timeout=10)
            result['sidebar'].append({'usable_ms': elapsed})
            time.sleep(0.7)
            row = measure_explorer(sandbox)
            result['explorer'].append(row)
            print(f"run={index + 1} sidebar={elapsed:.1f} explorer_usable={row['usable_ms']:.1f} "
                  f"explorer_folder={row['folder_ms']:.1f}", flush=True)
            time.sleep(0.7)
        for menu in menus:
            rows = []
            for index in range(args.menu_runs):
                if locked():
                    print('측정 중 시험 잠금 파일이 생겨 중단했습니다.')
                    return 3
                shot = args.shots / f'menu-{menu}.png' if args.shots and index == args.menu_runs - 1 else None
                rows.append(measure_menu(sandbox, args, menu, titles, shot))
                print(f'menu={menu} run={index + 1} {rows[-1]}', flush=True)
                time.sleep(2.0)
            result['menus'][menu] = rows
    for key in ('sidebar', 'explorer'):
        if result[key]:
            for metric in result[key][0]:
                s = summarize([row[metric] for row in result[key]])
                print(f"summary target={key} metric={metric} count={s['count']} min={s['min']} "
                      f"median={s['median']} max={s['max']}")
    for menu, rows in result['menus'].items():
        s = summarize([row['window_ms'] for row in rows if row.get('window_ms') is not None])
        print(f"summary menu={menu} count={s['count']} min={s['min']} median={s['median']} max={s['max']}")
    if args.json:
        args.json.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding='utf-8')
    return 0


if __name__ == '__main__':
    sys.exit(main())
