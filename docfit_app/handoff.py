"""창끼리 파일 넘기기의 받는 쪽과, 앱 화면 창에서 보내는 쪽.

보내기는 `sidebar.exe --send` 한 곳이 한다(`dock/src/handoff.rs`). 받는 앱 화면 창은 자기 수신함만 비운다:
  %LOCALAPPDATA%\\HWP_AutoDocFit\\open_requests\\screen-N\\*.json  (현재 사용자·SYSTEM 만 접근)
요청 상태: 대기(.json) → 가져감(.claimed, 이름 바꾸기에 성공한 한 곳만) → 삭제. 10분 지난 대기는 버린다.
형식·개수 표는 `flutter_ui/lib/features/common/file_handoff.dart`·`grid_core/src/handoff.rs` 와 같다.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from .api_contract import ServiceError

TARGETS = {
    # 메뉴 번호: (받는 확장자 — None 은 제한 없음, 한 번에 받는 개수)
    0: (frozenset('hwp hwpx pdf doc docx txt md xls pptx hml'.split()), 1000),
    11: (frozenset(['pdf']), 100),
    10: (frozenset('hwp hwpx pdf docx txt md xls pptx hml png jpg jpeg webp bmp tif tiff gif'.split()), 1000),
    12: (None, 200),
    13: (frozenset('hwpx hwp docx pdf xlsx pptx txt md'.split()), 200),
}
PHOTO_PASTE_MENU = 5
EXPIRE_SECONDS = 600
MAX_REQUEST_BYTES = 1024 * 1024


def mailbox(menu):
    from .instance_files import _folder
    return _folder() / f'screen-{menu}'


def _usable(menu, value):
    allowed = TARGETS[menu][0]
    if not isinstance(value, str) or '\0' in value:
        return False
    path = Path(value)
    if not path.is_absolute() or ':' in path.name:
        return False
    if allowed is not None and path.suffix.lower().lstrip('.') not in allowed:
        return False
    try:
        return path.is_file() and not path.is_symlink()
    except OSError:
        return False


def _read(path):
    try:
        if path.stat().st_size > MAX_REQUEST_BYTES:
            return None
        value = json.loads(path.read_text(encoding='utf-8'))
    except (OSError, ValueError):
        return None
    if not isinstance(value, dict) or not isinstance(value.get('paths'), list):
        return None
    return value


def take(menu, *, clock=time.time):
    """이 메뉴 창이 받을 것. {'paths': [...], 'photo_paste': bool, 'rejected': 건수}.

    개수 한도를 넘는 요청 묶음은 다음 호출로 남긴다. 검사에 걸린 경로는 버리고 건수를 알린다.
    """
    result = {'paths': [], 'photo_paste': False, 'rejected': 0}
    if menu not in TARGETS and menu != PHOTO_PASTE_MENU:
        return result
    try:
        requests = sorted(mailbox(menu).glob('*.json'))
    except OSError:
        return result
    for request in requests:
        try:
            expired = clock() - request.stat().st_mtime > EXPIRE_SECONDS
        except OSError:
            continue
        value = None if expired else _read(request)
        if value is not None and menu in TARGETS:
            limit = TARGETS[menu][1]
            if len(value['paths']) > limit:
                result['rejected'] += len(value['paths'])
                value = None
            elif result['paths'] and len(result['paths']) + len(value['paths']) > limit:
                break
        claimed = request.with_suffix('.claimed')
        try:
            request.replace(claimed)
        except OSError:
            continue  # 다른 곳이 가져갔다.
        try:
            claimed.unlink()
        except OSError:
            pass
        if value is None:
            continue
        if menu == PHOTO_PASTE_MENU:
            result['photo_paste'] = result['photo_paste'] or value.get('photo_paste') is True
            continue
        for path in value['paths']:
            if not _usable(menu, path):
                result['rejected'] += 1
            elif path not in result['paths']:
                result['paths'].append(path)
    return result


def send(menu, paths=(), *, photo_paste=False, settings_folder=None, runner=subprocess.run, finder=None):
    """`sidebar.exe --send` 로 보낸다. 실패하면 사용자에게 보일 문장을 담은 ServiceError."""
    from . import sidebar_launch
    if photo_paste:
        if menu != PHOTO_PASTE_MENU or paths:
            raise ServiceError('INVALID_INPUT', '붙임사진 편집 요청을 확인해 주세요.')
    elif (menu not in TARGETS or not isinstance(paths, (list, tuple)) or not 1 <= len(paths) <= TARGETS[menu][1]
          or not all(_usable(menu, p) for p in paths)):
        raise ServiceError('INVALID_INPUT', '보낼 수 있는 형식·개수의 파일인지 확인해 주세요.')
    executable = (finder or sidebar_launch.find_executable)()
    if executable is None:
        raise ServiceError('HANDOFF_UNAVAILABLE', '사이드바 프로그램(sidebar.exe)이 없어 다른 창으로 보낼 수 없습니다.')
    args = sidebar_launch.command(executable, Path(settings_folder or sidebar_launch._data_folder()))
    args += ['--send', str(menu)]
    listing = None
    try:
        if photo_paste:
            args.append('--photo-paste')
        else:
            # 명령줄 길이 한도를 피해 목록 파일로 넘긴다.
            with tempfile.NamedTemporaryFile('w', suffix='.json', prefix='docfit-send-', delete=False,
                                             encoding='utf-8') as stream:
                json.dump({'paths': list(paths)}, stream, ensure_ascii=False)
                listing = stream.name
            args += ['--list', listing]
        try:
            done = runner(args, capture_output=True, timeout=30,
                          creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        except (OSError, subprocess.TimeoutExpired):
            raise ServiceError('HANDOFF_FAILED', '다른 창으로 보내지 못했습니다.') from None
        if done.returncode != 0:
            message = (done.stdout or b'').decode('utf-8', 'replace').strip()
            raise ServiceError('HANDOFF_FAILED', message or '다른 창으로 보내지 못했습니다.')
    finally:
        if listing:
            try:
                os.unlink(listing)
            except OSError:
                pass
    return {'sent': len(paths)}
