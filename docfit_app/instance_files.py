"""같은 사용자 앱에 '열기' 파일을 전달하는 원자적 요청 큐."""
import json
import os
import sys
import uuid
from pathlib import Path


def _folder():
    return Path(os.environ.get('LOCALAPPDATA', Path.home())) / 'HWP_AutoDocFit' / 'open_requests'


def forward(files, photo_paste=False):
    rename, paths = split_rename_arguments(files)
    files = [str(Path(f).resolve()) for f in paths if Path(f).is_file()]
    if not files and not photo_paste:
        return
    folder = _folder()
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / (uuid.uuid4().hex + '.tmp')
    path.write_text(json.dumps({'rename': rename, 'paths': files, 'photo_paste': photo_paste}, ensure_ascii=False), encoding='utf-8')
    path.replace(path.with_suffix('.json'))


def split_photo_paste_arguments(arguments):
    """Separate `--photo-paste` or `--open-photo-edit` from command line arguments."""
    values = list(arguments or [])
    found = any(arg in values for arg in ('--photo-paste', '--open-photo-edit', '--open-photo-edit-paste'))
    rest = [v for v in values if v not in ('--photo-paste', '--open-photo-edit', '--open-photo-edit-paste')]
    return found, rest


def split_rename_arguments(arguments):
    """Separate `--rename <paths>` from ordinary document-open arguments."""
    values = list(arguments or [])
    if '--rename' not in values:
        return False, values
    index = values.index('--rename')
    paths = values[index + 1:]
    return True, [value for value in paths if isinstance(value, str) and not value.startswith('--')]


def _deliver(gui, paths, rename=False, photo_paste=False):
    if photo_paste:
        gui._photo_edit_startup = True
    if rename:
        # 화면 쪽 다음 상태 조회가 가져간다(desktop_web_ui._drain_rename_paths).
        queue = getattr(gui, '_rename_startup_paths', None)
        if isinstance(queue, list):
            queue.extend(paths)
        else:
            gui._rename_startup_paths = list(paths)
    else:
        for value in paths:
            if isinstance(value, str):
                gui.파일추가(value)


def start(gui):
    photo_paste, rest_args = split_photo_paste_arguments(sys.argv[1:])
    rename, paths = split_rename_arguments(rest_args)
    _deliver(gui, paths if rename else [value for value in paths if Path(value).is_file()], rename, photo_paste=photo_paste)

    def poll():
        folder = _folder()
        if folder.exists() and not gui.running:
            for path in sorted(folder.glob('*.json')):
                try:
                    values = json.loads(path.read_text(encoding='utf-8'))
                    if isinstance(values, dict):
                        _deliver(gui, values.get('paths') or [], values.get('rename') is True, photo_paste=values.get('photo_paste') is True)
                    elif isinstance(values, list):
                        _deliver(gui, values)
                    path.unlink()
                except (ValueError, UnicodeError):
                    # 깨진 요청을 격리하고 다음 정상 요청을 계속 처리한다.
                    try:
                        path.replace(path.with_suffix('.bad'))
                    except OSError:
                        pass
                except OSError:
                    pass
        gui.root.after(500, poll)
    gui.root.after(500, poll)
