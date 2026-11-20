"""현재 사용자 탐색기 메뉴. registry 인자로 가짜 winreg를 주입할 수 있다."""
import os
import sys
from pathlib import Path

ROOT = r'Software\Classes\SystemFileAssociations'
MENU = 'MongttangFileConvert'
# 예전 이름('한글편집 후처리' 메뉴). 등록 때 먼저 지우고, 해제 때도 함께 지운다.
LEGACY_MENU = 'HwpAutoDocFit'
MENU_TITLE = '몽땅파일변환'
EXTENSIONS = ('.hwp', '.hwpx', '.pdf', '.png', '.jpg', '.jpeg')


def launch_command():
    launcher = os.environ.get('DOCFIT_LAUNCHER_PATH')
    if launcher:
        return [launcher]
    if getattr(sys, 'frozen', False):
        return [sys.executable]
    return [sys.executable, str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py')]


def ctx_exe_for(command):
    """앱 실행 명령이 실행 파일 하나일 때만 꾸러미의 mdm-ctx.exe(경로 모음·전달)를 쓴다. 없으면 None(기존 직접 호출)."""
    if len(command) != 1:
        return None
    base = Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parents[1]))
    for path in (base / 'resources' / 'mdm-ctx.exe', base / 'mdm-ctx' / 'target' / 'release' / 'mdm-ctx.exe'):
        if path.is_file():
            return path
    return None


def _registry(registry):
    if registry is None:
        import winreg
        return winreg
    return registry


def _command(exe_path):
    parts = [os.fspath(exe_path)] if isinstance(exe_path, (str, os.PathLike)) else list(exe_path)
    if not parts or any('"' in part or '\x00' in part for part in parts):
        raise ValueError('실행 파일 경로를 확인해 주세요.')
    # 공백 없는 실행 파일도 항상 따옴표로 감싼다.
    return ' '.join('"' + part + '"' for part in parts)


def _menu_key(ext, name):
    return ROOT + '\\' + ext + '\\shell\\' + name


def menu_entries(exe_path, ctx_exe=None, *, menu=MENU):
    command = _command(exe_path)
    # ctx_exe 가 있으면 변환 명령은 mdm-ctx 가 받아 여러 파일을 모아 앱 convert 를 한 번만 띄운다(모음은 Rust 담당).
    ctx_prefix = f'{_command(ctx_exe)} --app {command} ' if ctx_exe else None
    result = {}
    for ext in EXTENSIONS:
        base = _menu_key(ext, menu)
        result[base] = {
            'MUIVerb': MENU_TITLE,
            'SubCommands': '',
            'DocFitCommand': command,
            'DocFitCtx': os.fspath(ctx_exe) if ctx_exe else '',
            'MultiSelectModel': 'Player',
        }
        if ext == '.hwp':
            actions = [
                ('pdf', 'PDF로 변환(여러 개)', 'pdf'),
                ('hwpx', 'HWPX로 변환(여러 개)', 'hwpx'),
                ('pdfm', 'PDF 하나로 합치기', 'pdfm'),
                ('png', '그림(PNG)으로 변환', None),
                ('open', '앱으로 열기', None),
            ]
        elif ext == '.hwpx':
            actions = [
                ('pdf', 'PDF로 변환', 'pdf'),
                ('pdfm', 'PDF 하나로 합치기', 'pdfm'),
                ('png', '그림(PNG)으로 변환', None),
                ('open', '앱으로 열기', None),
            ]
        elif ext == '.pdf':
            actions = [
                ('pdfm', 'PDF 하나로 합치기', 'pdfm'),
                ('pdfz', 'PDF 압축', None),
                ('png', '그림(PNG)으로 변환', None),
            ]
        else:
            actions = [
                ('pdf', 'PDF로 변환', None),
                ('pdfm', 'PDF 하나로 합치기', 'pdfm'),
            ]
        for index, (target, label, collect) in enumerate(actions):
            key = base + f'\\shell\\{index:02d}_{target}'
            result[key] = {'MUIVerb': label, 'MultiSelectModel': 'Player'}
            if target == 'open':
                value = f'{command} "%1"'
            elif ctx_prefix:
                value = f'{ctx_prefix}convert "%1" --to {target} --notify'
            elif collect:
                value = f'{command} convert "%1" --to {target} --collect {collect} --notify'
            else:
                value = f'{command} convert "%1" --to {target} --notify'
            result[key + r'\command'] = {'': value}
    return result


def _delete_tree(reg, path):
    try:
        with reg.OpenKey(reg.HKEY_CURRENT_USER, path, 0, reg.KEY_READ | reg.KEY_WRITE) as key:
            children = []
            index = 0
            while True:
                try:
                    children.append(reg.EnumKey(key, index))
                    index += 1
                except OSError:
                    break
        for child in children:
            _delete_tree(reg, path + '\\' + child)
        reg.DeleteKey(reg.HKEY_CURRENT_USER, path)
    except FileNotFoundError:
        pass


def unregister(*, registry=None, menu=MENU, legacy_menu=LEGACY_MENU):
    """새 키와 옛 키를 모두 지운다(다른 앱의 키는 건드리지 않는다)."""
    reg = _registry(registry)
    for ext in EXTENSIONS:
        for name in (menu, legacy_menu):
            _delete_tree(reg, _menu_key(ext, name))


def register(exe_path, *, registry=None, ctx_exe=None, menu=MENU, legacy_menu=LEGACY_MENU):
    """옛 키를 먼저 지운 뒤(메뉴가 둘 보이지 않게) 새 키를 쓴다. 같은 인자로 다시 불러도 결과가 같다."""
    reg = _registry(registry)
    for ext in EXTENSIONS:
        _delete_tree(reg, _menu_key(ext, legacy_menu))
    for path, values in menu_entries(exe_path, ctx_exe, menu=menu).items():
        with reg.CreateKey(reg.HKEY_CURRENT_USER, path) as key:
            for name, value in values.items():
                reg.SetValueEx(key, name, 0, reg.REG_SZ, value)


def _read(reg, path, name):
    try:
        with reg.OpenKey(reg.HKEY_CURRENT_USER, path) as key:
            try:
                return True, reg.QueryValueEx(key, name)[0]
            except FileNotFoundError:
                return True, None
    except FileNotFoundError:
        return False, None


def status(*, registry=None, menu=MENU, legacy_menu=LEGACY_MENU):
    """registered: 새·옛 키 중 하나라도 있으면 True(설정 화면이 켜짐으로 보이게). legacy: 옛 키가 남아 있음."""
    reg = _registry(registry)
    installed, legacy, command, ctx = False, False, None, None
    for ext in EXTENSIONS:
        found, value = _read(reg, _menu_key(ext, menu), 'DocFitCommand')
        if found:
            installed, command = True, value
            ctx = _read(reg, _menu_key(ext, menu), 'DocFitCtx')[1]
        if _read(reg, _menu_key(ext, legacy_menu), 'DocFitCommand')[0]:
            legacy = True
    return {'registered': installed or legacy, 'command': command, 'ctx': ctx, 'legacy': legacy}


def refresh_registered(*, registry=None, menu=MENU, legacy_menu=LEGACY_MENU):
    """앱 시작 때: 이미 켜져 있는 경우에만 옛 키를 새 키로 교체하고 낡은 경로를 고친다(꺼져 있으면 아무것도 쓰지 않는다)."""
    state = status(registry=registry, menu=menu, legacy_menu=legacy_menu)
    if not state['registered']:
        return
    command = launch_command()
    ctx = ctx_exe_for(command)
    if state['legacy'] or state['command'] != _command(command) or (state['ctx'] or '') != (os.fspath(ctx) if ctx else ''):
        register(command, registry=registry, ctx_exe=ctx, menu=menu, legacy_menu=legacy_menu)
