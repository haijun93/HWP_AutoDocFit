"""설치 꾸러미 완전성 점검: 필수 Win32 창 프로그램이 빠졌을 때 구체적인 안내를 만든다(PKGWIN1)."""
from __future__ import annotations

import logging
import sys
from pathlib import Path

REQUIRED_WINDOW_EXES = (
    'outliner_win.exe', 'settings_win.exe', 'docview_win.exe',
    'photoview_win.exe', 'convert_win.exe', 'rename_win.exe',
)

REINSTALL_HINT = ('설치가 완료되지 않았을 수 있습니다. 앱을 모두 닫고(작업 표시줄 오른쪽 아래 트레이 아이콘 포함) '
                  '설치 프로그램을 다시 실행해 주세요.')


def missing_window_message(label: str, exe_name: str) -> str:
    """서비스 오류 메시지: 어떤 파일이 없는지 + 다시 설치하라는 안내."""
    return f'{label}({exe_name})을 찾을 수 없습니다. {REINSTALL_HINT}'


def find_missing_window_exes(resources_dir) -> list:
    base = Path(resources_dir)
    return [name for name in REQUIRED_WINDOW_EXES if not (base / name).is_file()]


def check_frozen_package(resources_dir=None) -> list:
    """배포 앱에서만 점검해 누락 목록을 돌려주고 로그에 남긴다. 개발 환경(비동결)은 빈 목록."""
    if resources_dir is None:
        if not getattr(sys, 'frozen', False):
            return []
        resources_dir = Path(getattr(sys, '_MEIPASS', Path(sys.executable).parent)) / 'resources'
    try:
        missing = find_missing_window_exes(resources_dir)
    except OSError:
        return []
    if missing:
        logging.getLogger(__name__).warning('설치 꾸러미에 필수 창 프로그램 누락: %s — %s', ', '.join(missing), REINSTALL_HINT)
    return missing
