"""파일명 변경 Win32 미리보기 창 방식 설정과 실행."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

from .api_contract import ServiceError

UI_MODES = ('flutter', 'native')
UI_SETTING = 'rename_ui'
EXECUTABLE = 'rename_win.exe' if os.name == 'nt' else 'rename_win'


def find_executable(name=EXECUTABLE):
    root = Path(__file__).resolve().parents[1]
    bases = (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', root)))
    candidates = [p for b in bases for p in (b / '_internal' / 'resources' / name, b / 'resources' / name)]
    candidates += [root / 'resources' / name, root / 'rename_win' / 'target' / 'release' / name]
    return next((p for p in candidates if p.is_file()), Path(shutil.which('rename_win') or candidates[0]))


class RenameWindowService:
    def __init__(self, settings, executable=None, launcher=subprocess.Popen):
        self.settings, self._executable, self.launcher = settings, Path(executable) if executable else None, launcher

    @property
    def executable(self):
        return self._executable or find_executable()

    def execute(self, payload):
        action = payload.get('action')
        if action == 'ui_mode':
            value = payload.get('value')
            if value is not None:
                if value not in UI_MODES:
                    raise ServiceError('INVALID_INPUT', '파일명 변경 화면 방식을 확인해 주세요.')
                self.settings.update(lambda current: current.update({UI_SETTING: value}))
            mode = self.settings.read().get(UI_SETTING, 'flutter')
            return {'mode': mode if mode in UI_MODES else 'flutter'}
        if action in ('open_native', 'open_if_native'):
            mode = self.settings.read().get(UI_SETTING, 'flutter')
            if action == 'open_if_native' and mode != 'native':
                return {'mode': 'flutter', 'opened': False}
            if not self.executable.is_file():
                raise ServiceError('RENAME_WINDOW_MISSING', '파일명 변경 Win32 창(rename_win.exe)을 찾을 수 없습니다.')
            try:
                self.launcher([str(self.executable)], close_fds=True)
            except OSError:
                raise ServiceError('RENAME_WINDOW_FAILED', '파일명 변경 Win32 창을 열지 못했습니다.') from None
            return {'mode': mode if mode in UI_MODES else 'flutter', 'opened': True}
        raise ServiceError('INVALID_INPUT', '파일명 변경 창 명령을 확인해 주세요.')
