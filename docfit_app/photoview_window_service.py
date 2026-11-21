"""사진뷰 화면 방식(`photoview_ui`: 'flutter' 기본 | 'native')과 Win32 사진뷰 창(photoview_win.exe) 실행.

기능은 모두 Rust 창이 맡는다. 여기에는 방식 읽기·쓰기와 창 켜기만 있다(settings_ui·outliner_ui 와 같은 구조).
창은 같은 설정 파일로 이미 떠 있으면 새로 뜨지 않고 그 창을 앞으로 가져온다(창 쪽 단일 인스턴스).
"""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

from .api_contract import ServiceError


UI_MODES = ('flutter', 'native')
UI_SETTING = 'photoview_ui'
EXECUTABLE = 'photoview_win.exe' if os.name == 'nt' else 'photoview_win'


def find_executable(name=EXECUTABLE):
    """설치본(resources)·개발 폴더(photoview_win/target)·PATH 순으로 photoview_win.exe 를 찾는다."""
    app_root = Path(__file__).resolve().parents[1]
    candidates = []
    for base in (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', app_root))):
        candidates.extend((base / '_internal' / 'resources' / name, base / 'resources' / name))
    candidates.extend((app_root / 'resources' / name, app_root / 'photoview_win' / 'target' / 'release' / name))
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    found = shutil.which('photoview_win')
    return Path(found) if found else candidates[0]


def _find_klib():
    """창이 쓸 klib(앱이 klib 를 찾는 규칙과 같게). 못 찾으면 None → 창이 자기 규칙(옆 폴더·resources)으로 찾는다."""
    try:
        from .rust_scan import find_klib_executable
        return find_klib_executable()
    except Exception:
        return None


class PhotoViewWindowService:
    def __init__(self, settings, executable=None, launcher=subprocess.Popen, settings_path=None, klib_finder=_find_klib):
        self.settings = settings
        self._executable = Path(executable) if executable else None
        self.launcher = launcher
        self._klib_finder = klib_finder
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
            raise ServiceError('PHOTOVIEW_WINDOW_UNAVAILABLE', '설정을 읽을 수 없습니다.')
        if value is not None:
            if value not in UI_MODES:
                raise ServiceError('INVALID_INPUT', '사진뷰 화면 방식을 확인해 주세요.')
            self.settings.update(lambda current: current.update({UI_SETTING: value}))
        mode = self.settings.read().get(UI_SETTING, 'flutter')
        return mode if mode in UI_MODES else 'flutter'

    def open_native(self):
        exe = self.executable
        if not exe.is_file():
            raise ServiceError('PHOTOVIEW_WINDOW_MISSING', '사진뷰 창 프로그램(photoview_win.exe)을 찾을 수 없습니다.')
        args = [str(exe)]
        if self._settings_path:
            args += ['--settings', str(self._settings_path)]
        klib = self._klib_finder() if self._klib_finder else None
        if klib:
            args += ['--klib', str(klib)]
        try:
            self.launcher(args, close_fds=True)
        except OSError:
            raise ServiceError('PHOTOVIEW_WINDOW_FAILED', '사진뷰 창을 열지 못했습니다.') from None
        return {'opened': True}

    def execute(self, payload):
        action = payload.get('action')
        if action == 'ui_mode':
            return {'mode': self.mode(payload.get('value') if 'value' in payload else None)}
        if action == 'open_native':
            return self.open_native()
        if action == 'open_if_native':
            # 사진뷰 탭 진입이 부르는 한 번의 호출: native 일 때만 창을 연다(기본 flutter 는 아무 일도 하지 않는다).
            mode = self.mode()
            if mode != 'native':
                return {'mode': mode, 'opened': False}
            return {'mode': mode, **self.open_native()}
        raise ServiceError('INVALID_INPUT', '사진뷰 창 명령을 확인해 주세요.')
