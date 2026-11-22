"""몽땅파일변환 화면 방식과 Win32 창 실행."""
from __future__ import annotations
import os
from pathlib import Path
import shutil
import subprocess
import sys
from .api_contract import ServiceError
from .package_integrity import missing_window_message

UI_MODES = ('flutter', 'native')
UI_SETTING = 'conversion_ui'
EXECUTABLE = 'convert_win.exe' if os.name == 'nt' else 'convert_win'

def find_executable():
    root = Path(__file__).resolve().parents[1]
    candidates=[]
    for base in (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', root))):
        candidates.extend((base/'_internal'/'resources'/EXECUTABLE, base/'resources'/EXECUTABLE))
    candidates.extend((root/'resources'/EXECUTABLE, root/'convert_win'/'target'/'release'/EXECUTABLE))
    for path in candidates:
        if path.is_file(): return path
    found=shutil.which('convert_win')
    return Path(found) if found else candidates[0]

class ConversionWindowService:
    def __init__(self, settings, executable=None, launcher=subprocess.Popen):
        self.settings=settings; self._executable=Path(executable) if executable else None; self.launcher=launcher
    @property
    def executable(self):
        if self._executable is None: self._executable=find_executable()
        return self._executable
    def mode(self, value=None):
        if self.settings is None: raise ServiceError('CONVERSION_WINDOW_UNAVAILABLE','설정을 읽을 수 없습니다.')
        if value is not None:
            if value not in UI_MODES: raise ServiceError('INVALID_INPUT','변환 화면 방식을 확인해 주세요.')
            self.settings.update(lambda current: current.update({UI_SETTING:value}))
        mode=self.settings.read().get(UI_SETTING,'flutter')
        return mode if mode in UI_MODES else 'flutter'
    def open_native(self):
        if not self.executable.is_file(): raise ServiceError('CONVERSION_WINDOW_MISSING',missing_window_message('몽땅파일변환 창','convert_win.exe'))
        try: self.launcher([str(self.executable)], close_fds=True)
        except OSError: raise ServiceError('CONVERSION_WINDOW_FAILED','몽땅파일변환 창을 열지 못했습니다.') from None
        return {'opened':True}
    def execute(self,payload):
        action=payload.get('action')
        if action=='ui_mode': return {'mode':self.mode(payload.get('value') if 'value' in payload else None)}
        if action=='open_native': return self.open_native()
        if action=='open_if_native':
            mode=self.mode()
            if mode!='native': return {'mode':mode,'opened':False}
            return {'mode':mode,**self.open_native()}
        raise ServiceError('INVALID_INPUT','변환 창 명령을 확인해 주세요.')
