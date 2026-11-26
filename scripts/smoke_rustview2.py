"""합성 파일·격리 설정으로 실제 Win32 컨트롤을 시험하고 창을 캡처한다.

사용: python scripts/smoke_rustview2.py --bin-dir <cargo target/debug> --out <작업 폴더 아래>
한/글 COM·설치·서비스 켜기·자동 실행 변경은 하지 않는다.
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
u, g = c.WinDLL('user32', use_last_error=True), c.WinDLL('gdi32', use_last_error=True)
u.SetProcessDpiAwarenessContext.argtypes = [c.c_void_p]
u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
u.FindWindowW.argtypes = [w.LPCWSTR, w.LPCWSTR]
u.FindWindowW.restype = w.HWND
u.GetDlgItem.argtypes = [w.HWND, c.c_int]
u.GetDlgItem.restype = w.HWND
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = w.LPARAM
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SetWindowTextW.argtypes = [w.HWND, w.LPCWSTR]
u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.IsWindowEnabled.argtypes = [w.HWND]
u.GetWindowDC.argtypes = [w.HWND]
u.GetWindowDC.restype = w.HDC
u.ReleaseDC.argtypes = [w.HWND, w.HDC]
u.PrintWindow.argtypes = [w.HWND, w.HDC, w.UINT]
g.CreateCompatibleDC.argtypes = [w.HDC]
g.CreateCompatibleDC.restype = w.HDC
g.CreateCompatibleBitmap.argtypes = [w.HDC, c.c_int, c.c_int]
g.CreateCompatibleBitmap.restype = w.HBITMAP
g.SelectObject.argtypes = [w.HDC, w.HGDIOBJ]
g.SelectObject.restype = w.HGDIOBJ
g.DeleteObject.argtypes = [w.HGDIOBJ]
g.DeleteDC.argtypes = [w.HDC]
g.GetDIBits.argtypes = [w.HDC, w.HBITMAP, w.UINT, w.UINT, c.c_void_p, c.c_void_p, w.UINT]


def wait(check, message, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(.03)
    raise AssertionError(message)


def find(cls, pid):
    handle = u.FindWindowW(cls, None)
    actual = w.DWORD()
    u.GetWindowThreadProcessId(handle, c.byref(actual))
    return handle if actual.value == pid else None


def field(h, identifier):
    return u.GetDlgItem(h, identifier)


def text(h):
    buf = c.create_unicode_buffer(65536)
    u.SendMessageW(h, 0xD, len(buf), c.cast(buf, c.c_void_p).value)
    return buf.value


def set_text(h, value):
    # 다른 프로세스의 EDIT는 SetWindowText가 아니라 WM_SETTEXT로 바꿔야 한다.
    buf = c.create_unicode_buffer(value)
    return u.SendMessageW(h, 0xC, 0, c.cast(buf, c.c_void_p).value)


def command(h, identifier, notification=0):
    u.PostMessageW(h, 0x111, identifier | (notification << 16), field(h, identifier))


def choose(h, identifier, index):
    u.SendMessageW(field(h, identifier), 0x14e, index, 0)
    command(h, identifier, 1)


def click(h, x, y):
    u.PostMessageW(h, 0x202, 0, (y << 16) | x)


def status(h):
    return text(field(h, 180))


def confirm(pid, answer=6):
    seen = []
    callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
    @callback_type
    def visit(handle, _):
        actual = w.DWORD()
        u.GetWindowThreadProcessId(handle, c.byref(actual))
        name = c.create_unicode_buffer(256)
        u.GetClassNameW(handle, name, len(name))
        if actual.value == pid and name.value == '#32770' and field(handle, 6):
            seen.append(handle)
            command(handle, answer)
        return True
    u.EnumWindows(visit, 0)
    return bool(seen)


def drop(h, paths):
    kernel = c.WinDLL('kernel32', use_last_error=True)
    kernel.GlobalAlloc.argtypes = [w.UINT, c.c_size_t]
    kernel.GlobalAlloc.restype = w.HGLOBAL
    kernel.GlobalLock.argtypes = [w.HGLOBAL]
    kernel.GlobalLock.restype = c.c_void_p
    kernel.GlobalUnlock.argtypes = [w.HGLOBAL]
    class DROPFILES(c.Structure):
        _fields_ = [('pFiles', w.DWORD), ('pt', w.POINT), ('fNC', w.BOOL), ('fWide', w.BOOL)]
    payload = ('\0'.join(map(str, paths)) + '\0\0').encode('utf-16-le')
    header = DROPFILES(c.sizeof(DROPFILES), w.POINT(0, 0), 0, 1)
    data = bytes(header) + payload
    memory = kernel.GlobalAlloc(0x42, len(data))
    pointer = kernel.GlobalLock(memory)
    c.memmove(pointer, data, len(data))
    kernel.GlobalUnlock(memory)
    u.PostMessageW(h, 0x233, memory, 0)


def capture(h, path):
    from PIL import Image
    rect = w.RECT()
    u.GetWindowRect(h, c.byref(rect))
    width, height = rect.right - rect.left, rect.bottom - rect.top
    dc = u.GetWindowDC(h)
    memory = g.CreateCompatibleDC(dc)
    bitmap = g.CreateCompatibleBitmap(dc, width, height)
    old = g.SelectObject(memory, bitmap)
    class Header(c.Structure):
        _fields_ = [('size', w.DWORD), ('width', w.LONG), ('height', w.LONG), ('planes', w.WORD), ('bits', w.WORD),
                    ('compression', w.DWORD), ('image', w.DWORD), ('x', w.LONG), ('y', w.LONG), ('used', w.DWORD), ('important', w.DWORD)]
    header = Header(c.sizeof(Header), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
    data = c.create_string_buffer(width * height * 4)
    try:
        assert u.PrintWindow(h, memory, 2)
        assert g.GetDIBits(memory, bitmap, 0, height, data, c.byref(header), 0)
        Image.frombuffer('RGB', (width, height), data.raw, 'raw', 'BGRX', 0, 1).save(path)
    finally:
        g.SelectObject(memory, old)
        g.DeleteObject(bitmap)
        g.DeleteDC(memory)
        u.ReleaseDC(h, dc)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    a = parser.parse_args()
    out = a.out.resolve()
    assert out.is_relative_to(ROOT) and out != ROOT
    out.mkdir(parents=True, exist_ok=True)
    fixture = out / 'synthetic'
    fixture.mkdir(exist_ok=True)
    data = fixture / 'data'
    data.mkdir(exist_ok=True)
    env = dict(os.environ, APPDATA=str(data), LOCALAPPDATA=str(data), DOCFIT_DOCUMENT_ROOT=str(fixture),
        DOCFIT_SOURCE_ROOT=str(ROOT), DOCFIT_PYTHON=sys.executable, PYTHONIOENCODING='utf-8', DOCFIT_NATIVE_TRACE=str(out / 'native-trace.jsonl'))
    env.pop('DOCFIT_APP_EXE', None)
    processes = []
    results = []
    def start(exe, args, cls):
        process = subprocess.Popen([str(a.bin_dir.resolve() / exe), *args], cwd=ROOT, env=env)
        processes.append(process)
        return process, wait(lambda: find(cls, process.pid), f'{exe} 창 없음')
    try:
        process, h = start('rename_win.exe', ['--data-dir', str(data)], 'DocFitRenameWin2')
        wait(lambda: status(h) == '완료', '파일명 초기 목록 응답 없음')
        source = fixture / '합성보고서9.txt'
        source.write_text('합성 내용 보존', encoding='utf-8')
        drop(h, [source])
        wait(lambda: u.SendMessageW(field(h, 101), 0x18b, 0, 0) == 1, '드롭 파일 없음')
        choose(h, 103, 6)  # 번호 늘리기
        command(h, 120)
        wait(lambda: field(h, 300), '규칙 입력 폼 없음')
        wait(lambda: field(h, 301), '규칙 입력 폼 생성 중')
        time.sleep(.15)
        assert set_text(field(h, 300), '3')
        wait(lambda: text(field(h, 300)) == '3', '입력 값 반영 실패')
        command(h, 104)
        command(h, 130)
        wait(lambda: '미리보기' in status(h) and '변경 1' in status(h), '미리보기 실패')
        capture(h, out / 'rename-preview.png')
        command(h, 131)
        wait(lambda: (confirm(process.pid), (fixture / '합성보고서12.txt').is_file())[1], '이름 변경 실패')
        wait(lambda: status(h).startswith('완료') and u.IsWindowEnabled(field(h, 110)), '적용 후 목록 응답 없음')
        command(h, 134)
        wait(lambda: u.SendMessageW(field(h, 135), 0x146, 0, 0) == 1 and status(h) == '완료', '이력 없음')
        command(h, 133)
        wait(lambda: (confirm(process.pid), source.is_file())[1], '되돌리기 실패')
        assert source.read_text(encoding='utf-8') == '합성 내용 보존'
        wait(lambda: status(h).startswith('완료') and u.IsWindowEnabled(field(h, 110)), '되돌린 뒤 목록 없음')
        # 입력 폼 23개를 실제 컨트롤로 생성한다. 저장·적용은 하지 않는다.
        catalog = json.loads((ROOT / 'native_tasks/src/rules.json').read_text(encoding='utf-8'))
        def form_ready(rule):
            records = [json.loads(line) for line in (out / 'native-trace.jsonl').read_text(encoding='utf-8').splitlines()]
            forms = [r for r in records if r.get('phase') == 'rule_form']
            return forms and forms[-1]['kind'] == rule['kind'] and forms[-1]['fields'] == len(rule['defaults'])
        for index, rule in enumerate(catalog):
            choose(h, 103, index)
            command(h, 120)
            wait(lambda: form_ready(rule), f"규칙 폼 생성 실패: {rule['kind']}")
        capture(h, out / 'rename-rules.png')
        choose(h, 103, 0)
        command(h, 120)
        wait(lambda: form_ready(catalog[0]) and field(h, 304), '치환 입력 폼 없음')
        set_text(field(h, 300), '미저장 합성 입력')
        u.PostMessageW(h, 0x10, 0, 0)
        wait(lambda: confirm(process.pid, 7), '미저장 닫기 확인 없음')
        wait(lambda: u.IsWindowEnabled(h), '닫기 취소 후 창이 비활성 상태')
        assert process.poll() is None
        results.append('rename: 23 native rule forms + drop/preview/apply/history/undo + dirty close protection PASS')
        u.PostMessageW(h, 0x10, 0, 0)
        wait(lambda: (confirm(process.pid), process.poll() is not None)[1], '확인 후 창 종료 실패')

        settings = data / 'settings.json'
        settings.write_text('{}', encoding='utf-8')
        original = settings.read_bytes()
        process, h = start('settings_win.exe', ['--settings', str(settings)], 'SettingsWinMain')
        capture(h, out / 'settings.png')
        assert field(h, 110), '관리 진입 단추 없음'
        u.PostMessageW(h, 0x10, 0, 0)
        process.wait(timeout=10)
        # 한/글 사용자 상용구도 실제 사용자 폴더 대신 합성 IDO로만 저장한다.
        from docfit_core.hwp_idioms import Idiom, IdiomFile, dump, parse
        legacy = data / 'HNC/User/Hwp/60/HWP.IDO'
        legacy.parent.mkdir(parents=True, exist_ok=True)
        legacy.write_bytes(dump(IdiomFile(text=[Idiom('합성', '기존 합성 본말')])))
        process, h = start('settings_win.exe', ['--manage', '--settings', str(settings)], 'DocFitSettingsManage')
        wait(lambda: status(h) == '완료', '합성 IDO 응답 없음')
        u.SendMessageW(field(h, 101), 0x186, 0, 0)
        command(h, 101, 1)
        wait(lambda: field(h, 301), '상용구 입력 폼 없음')
        time.sleep(.1)
        set_text(field(h, 301), '바꾼 합성 본말')
        command(h, 162)
        wait(lambda: (confirm(process.pid), parse(legacy.read_bytes()).text[0].value == '바꾼 합성 본말')[1], '합성 상용구 저장 실패')
        wait(lambda: status(h) == '완료', '저장 후 상용구 응답 없음')
        capture(h, out / 'settings-idiom-edit.png')
        u.PostMessageW(h, 0x10, 0, 0)
        process.wait(timeout=10)
        from docfit_core.hwpx_idioms import create_template
        config = fixture / '글자상용구' / '글자상용구.hwpx'
        config.parent.mkdir(exist_ok=True)
        create_template(ROOT / 'resources/서식예시/기본 서식.hwpx', config, [dict(key='합성', value='시험 본말')])
        process, h = start('settings_win.exe', ['--manage', '--settings', str(settings)], 'DocFitSettingsManage')
        wait(lambda: status(h) == '완료', '설정 관리 응답 없음')
        capture(h, out / 'settings-idioms.png')
        choose(h, 160, 3)
        wait(lambda: status(h) == '완료', '앱 정보 응답 없음')
        capture(h, out / 'settings-about.png')
        choose(h, 160, 4)
        wait(lambda: field(h, 301) and status(h) == '완료', '창·검색 설정 조회 실패')
        capture(h, out / 'settings-runtime.png')
        assert settings.read_bytes() == original
        results.append('settings: isolated config unchanged + synthetic IDO save + HWPX readonly management/about/runtime settings read PASS')
        u.PostMessageW(h, 0x10, 0, 0)
        process.wait(timeout=10)

        from pypdf import PdfWriter, PdfReader
        paths = []
        before = []
        for index, width in enumerate([123, 234]):
            path = fixture / f'합성{index}.pdf'
            writer = PdfWriter()
            writer.add_blank_page(width=width, height=100)
            with path.open('wb') as stream:
                writer.write(stream)
            paths.append(path)
            before.append(path.read_bytes())
        process, h = start('convert_win.exe', list(map(str, paths)), 'ConvertWinStage1')
        click(h, 28 + 2 * 162 + 10, 110)  # PDF 병합
        time.sleep(.1)
        capture(h, out / 'convert-ready.png')
        # 창 실제 클라이언트 크기에 맞춘 시작 단추.
        rect = w.RECT()
        u.GetClientRect(h, c.byref(rect))
        click(h, rect.right - 100, rect.bottom - 50)
        merged = wait(lambda: next((p for p in fixture.glob('*.pdf') if p not in paths), None), '실제 PDF 병합 출력 없음', 30)
        wait(lambda: len(PdfReader(merged).pages) == 2, '병합 완료 대기')
        time.sleep(.3)
        capture(h, out / 'convert-complete.png')
        assert [int(p.mediabox.width) for p in PdfReader(merged).pages] == [123, 234]
        assert [p.read_bytes() for p in paths] == before
        results.append('convert: real native 12 targets + ordered PDF merge + sources unchanged PASS')
        u.PostMessageW(h, 0x10, 0, 0)
        process.wait(timeout=10)
    finally:
        for process in processes:
            if process.poll() is None:
                handle = next((find(cls, process.pid) for cls in ['DocFitRenameWin2', 'SettingsWinMain', 'DocFitSettingsManage', 'ConvertWinStage1'] if find(cls, process.pid)), None)
                if handle:
                    u.PostMessageW(handle, 0x10, 0, 0)
                    confirm(process.pid)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    # 시험에서 만든 UI 프로세스만 정리한다.
                    process.terminate()
    report = '\n'.join(results)
    (out / 'smoke.txt').write_text(report + '\n', encoding='utf-8')
    print(report)


if __name__ == '__main__':
    main()
