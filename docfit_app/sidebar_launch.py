"""앱 실행을 사이드바(sidebar.exe)로 넘길지 정한다. 기본은 기존 앱이다.

넘기는 경우: `--sidebar`, 또는 인자 없이 실행했고 설정 `launch_mode` 가 'sidebar'.
`--full-app` 은 설정과 상관없이 기존 앱을 연다.
업데이트 교체 중이거나 설치형 런처가 넘겨받기를 기다릴 때는 넘기지 않는다(새 판이 켜졌다는 신호가 필요하다).
sidebar.exe 를 직접 실행하면 Python 없이 바로 뜬다. 이 경로는 기존 바로가기를 그대로 쓸 때의 것이다.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

SETTING = 'launch_mode'
FORCE = '--sidebar'
FULL = '--full-app'
EXECUTABLE = 'sidebar.exe'


def _data_folder():
    appdata = os.environ.get('APPDATA') or os.environ.get('LOCALAPPDATA')
    return Path(appdata) / 'HwpAutoDocFit' if appdata else Path.home() / '.hwp_auto_docfit'


def _root():
    return Path(__file__).resolve().parents[1]


def find_executable():
    candidates = []
    for base in (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', _root()))):
        candidates.extend((base / '_internal' / 'resources' / EXECUTABLE, base / 'resources' / EXECUTABLE))
    candidates.extend((_root() / 'resources' / EXECUTABLE, _root() / 'dock' / 'target' / 'release' / EXECUTABLE))
    return next((path for path in candidates if path.is_file()), None)


def wanted(arguments, folder, environ):
    if environ.get('DOCFIT_LAUNCHER_HANDOFF_EVENT') or (folder / 'updates' / 'update_busy.flag').exists():
        return False
    if arguments == [FORCE]:
        return True
    if arguments:
        return False
    try:
        settings = json.loads((folder / 'settings.json').read_text(encoding='utf-8'))
        return settings.get(SETTING) == 'sidebar'
    except (OSError, ValueError, AttributeError):
        return False


def command(executable, folder):
    args = [str(executable), '--settings', str(folder / 'settings.json')]
    if getattr(sys, 'frozen', False):
        args += ['--app', str(Path(sys.executable).resolve())]
    else:
        args += ['--python', str(Path(sys.executable)), '--source-root', str(_root())]
    return args


def redirect(argv, *, folder=None, environ=None, finder=find_executable, launcher=subprocess.Popen):
    """사이드바를 띄웠으면 True(호출한 앱은 끝낸다). `--full-app` 은 argv 에서 지운다."""
    if FULL in argv[1:]:
        argv[1:] = [value for value in argv[1:] if value != FULL]
        return False
    folder = folder or _data_folder()
    if os.name != 'nt' or not wanted(argv[1:], folder, os.environ if environ is None else environ):
        argv[1:] = [value for value in argv[1:] if value != FORCE]
        return False
    executable = finder()
    if executable is not None:
        try:
            launcher(command(executable, folder), close_fds=True)
            return True
        except OSError:
            pass
    argv[1:] = [value for value in argv[1:] if value != FORCE]
    return False  # 사이드바가 없으면 기존 앱을 연다.
