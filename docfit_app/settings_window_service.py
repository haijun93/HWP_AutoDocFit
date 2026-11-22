"""설정 창 방식(`settings_ui`: 'flutter' 기본 | 'native')과 Win32 설정 창(settings_win.exe) 실행.

기능은 모두 Rust 창이 맡는다. 여기에는 방식 읽기·쓰기와 창 켜기만 있다(outliner_ui 와 같은 구조).
"""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

from .api_contract import ServiceError
from .package_integrity import missing_window_message


UI_MODES = ('flutter', 'native')
UI_SETTING = 'settings_ui'
EXECUTABLE = 'settings_win.exe' if os.name == 'nt' else 'settings_win'


def find_executable(name=EXECUTABLE):
    """설치본(resources)·개발 폴더(settings_win/target)·PATH 순으로 settings_win.exe 를 찾는다."""
    app_root = Path(__file__).resolve().parents[1]
    candidates = []
    for base in (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', app_root))):
        candidates.extend((base / '_internal' / 'resources' / name, base / 'resources' / name))
    candidates.extend((app_root / 'resources' / name, app_root / 'settings_win' / 'target' / 'release' / name))
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    found = shutil.which('settings_win')
    return Path(found) if found else candidates[0]


class SettingsWindowService:
    def __init__(self, settings, executable=None, launcher=subprocess.Popen, settings_path=None):
        self.settings = settings
        self._executable = Path(executable) if executable else None
        self.launcher = launcher
        # 창이 읽고 쓸 설정 파일(앱과 같은 파일). 없으면 창이 기본 위치(%APPDATA%)를 쓴다.
        path = settings_path if settings_path is not None else getattr(settings, '_path', None)
        self._settings_path = path() if callable(path) else path

    @property
    def executable(self):
        if self._executable is None:
            self._executable = find_executable()
        return self._executable

    def mode(self, value=None):
        """현재 방식. `value` 가 있으면 저장한다('flutter'|'native' 만)."""
        if self.settings is None:
            raise ServiceError('SETTINGS_WINDOW_UNAVAILABLE', '설정을 읽을 수 없습니다.')
        if value is not None:
            if value not in UI_MODES:
                raise ServiceError('INVALID_INPUT', '설정 창 방식을 확인해 주세요.')
            self.settings.update(lambda current: current.update({UI_SETTING: value}))
        mode = self.settings.read().get(UI_SETTING, 'flutter')
        return mode if mode in UI_MODES else 'flutter'

    def open_native(self):
        exe = self.executable
        if not exe.is_file():
            raise ServiceError('SETTINGS_WINDOW_MISSING', missing_window_message('설정 창 프로그램','settings_win.exe'))
        args = [str(exe)]
        if self._settings_path:
            args += ['--settings', str(self._settings_path)]
        try:
            self.launcher(args, close_fds=True)
        except OSError:
            raise ServiceError('SETTINGS_WINDOW_FAILED', '설정 창을 열지 못했습니다.') from None
        return {'opened': True}

    def execute(self, payload):
        action = payload.get('action')
        if action == 'ui_mode':
            return {'mode': self.mode(payload.get('value') if 'value' in payload else None)}
        if action == 'open_native':
            return self.open_native()
        if action == 'open_if_native':
            # 앱의 [설정] 진입이 부르는 한 번의 호출: native 일 때만 창을 연다(기본 flutter 는 아무 일도 하지 않는다).
            mode = self.mode()
            if mode != 'native':
                return {'mode': mode, 'opened': False}
            return {'mode': mode, **self.open_native()}
        raise ServiceError('INVALID_INPUT', '설정 창 명령을 확인해 주세요.')
