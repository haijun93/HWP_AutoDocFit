"""Safe user-data adapter for the Rust Markdown outliner."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading

from .api_contract import ServiceError
from .package_integrity import missing_window_message


UI_MODES = ('flutter', 'native')
UI_SETTING = 'outliner_ui'
# 상주 엔진 끄기: 설정 값 false 또는 환경 변수 0. 끄면 명령마다 실행하는 예전 방식이다.
RESIDENT_SETTING = 'outliner_resident_engine'
RESIDENT_ENV = 'DOCFIT_OUTLINER_RESIDENT'
_DEFAULT_RUNNER = subprocess.run


class OutlinerService:
    def __init__(self, data_root, executable=None, runner=subprocess.run, settings=None,
                 native_executable=None, launcher=subprocess.Popen, *, persistent=None,
                 popen=subprocess.Popen, engine_timeout=30, runtime_parent=None):
        self.root = (Path(data_root) / 'Outliner').resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        self.executable = Path(executable) if executable else self._find_executable()
        self.runner = runner
        self.engine_timeout = engine_timeout
        self._closed = threading.Event()
        # Custom runners retain the existing single-shot contract unless the
        # caller explicitly opts into the streaming transport.
        self._transport = None
        use_persistent = (persistent if persistent is not None
                          else runner is _DEFAULT_RUNNER and self._resident_enabled(settings))
        if use_persistent:
            from .outliner_engine import OutlinerEngine
            self._transport = OutlinerEngine(self.executable, runner=runner, popen=popen,
                                             timeout=engine_timeout, runtime_parent=runtime_parent)
        # 화면 방식 설정('flutter' 기본 | 'native')과 Win32 창(outliner_win.exe) 실행. 기능은 전부 Rust 창이 맡고 여기는 켜기만 한다.
        self.settings = settings
        self._native_executable = Path(native_executable) if native_executable else None
        self.launcher = launcher
        self._lock = threading.RLock()
        self._versions: dict[str, str] = {}
        self._recent: list[str] = []
        self._trash_history: list[dict[str, str]] = []
        self._rename_history: list[dict[str, str]] = []

    @staticmethod
    def _find_executable(name=None, dev_dir='outliner_core', which='outliner'):
        name = name or ('outliner.exe' if os.name == 'nt' else 'outliner')
        app_root = Path(__file__).resolve().parents[1]
        candidates = []
        for base in (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', app_root))):
            candidates.extend((base / '_internal' / 'resources' / name, base / 'resources' / name))
        candidates.extend((app_root / 'resources' / name, app_root / dev_dir / 'target' / 'release' / name))
        for candidate in candidates:
            if candidate.is_file():
                return candidate
        found = shutil.which(which)
        return Path(found) if found else candidates[0]

    @property
    def native_executable(self):
        if self._native_executable is None:
            self._native_executable = self._find_executable('outliner_win.exe', 'outliner_win', 'outliner_win')
        return self._native_executable

    @staticmethod
    def _resident_enabled(settings):
        if os.environ.get(RESIDENT_ENV) == '0':
            return False
        read = getattr(settings, 'read', None)
        if read is None:
            return True
        try:
            return read().get(RESIDENT_SETTING) is not False
        except ServiceError:  # 설정을 못 읽어도 아웃라이너는 계속 연다.
            return True

    def _ui_mode(self, payload):
        if self.settings is None:
            raise ServiceError('OUTLINER_UNAVAILABLE', '설정을 읽을 수 없습니다.')
        if 'value' in payload:
            value = payload.get('value')
            if value not in UI_MODES:
                raise ServiceError('INVALID_INPUT', '아웃라이너 화면 방식을 확인해 주세요.')
            self.settings.update(lambda current: current.update({UI_SETTING: value}))
        mode = self.settings.read().get(UI_SETTING, 'flutter')
        return {'mode': mode if mode in UI_MODES else 'flutter'}

    def _open_native(self):
        exe = self.native_executable
        if not exe.is_file():
            raise ServiceError('OUTLINER_WINDOW_MISSING', missing_window_message('아웃라이너 창 프로그램','outliner_win.exe'))
        try:
            self.launcher([str(exe), '--data-dir', str(self.root)], close_fds=True)
        except OSError:
            raise ServiceError('OUTLINER_WINDOW_FAILED', '아웃라이너 창을 열지 못했습니다.') from None
        return {'opened': True}

    def _path(self, value):
        if not isinstance(value, str) or not value or Path(value).name != value or value in {'.', '..'}:
            raise ServiceError('INVALID_PATH', '페이지 이름을 확인해 주세요.')
        name = value if value.endswith('.md') else f'{value}.md'
        path = (self.root / name).resolve()
        if path.parent != self.root or path.suffix.lower() != '.md':
            raise ServiceError('INVALID_PATH', '사용자 데이터 폴더 밖의 경로는 사용할 수 없습니다.')
        return path

    def _page_references(self, page_path):
        files = [str(p) for p in sorted(self.root.glob('*.md'))
                 if p.is_file() and p.resolve() != page_path.resolve()]
        result = self._engine({
            'id': 'check-page-references',
            'op': 'check_page_references',
            'args': {'page': page_path.stem, 'files': files},
        })
        referring = [Path(name).stem for name in result.get('referring_files', [])]
        return referring, result.get('referring_blocks', [])

    @staticmethod
    def _digest(path):
        try:
            return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None
        except OSError:
            raise ServiceError('OUTLINER_UNAVAILABLE', '페이지를 읽을 수 없습니다.') from None

    def _engine(self, request):
        if not self.executable.is_file():
            raise ServiceError('OUTLINER_ENGINE_MISSING', '아웃라이너 엔진을 찾을 수 없습니다.')
        if self._transport is not None:
            value = self._transport.request(request)
            if not value['ok']:
                error = value['error']
                raise ServiceError(str(error.get('code', 'OUTLINER_FAILED')),
                                   str(error.get('message', '요청에 실패했습니다.')))
            return value['result']
        try:
            result = self.runner([str(self.executable)], input=json.dumps(request, ensure_ascii=False) + '\n',
                                 creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0), capture_output=True, text=True, encoding='utf-8', timeout=self.engine_timeout, check=False)
            value = json.loads(result.stdout)
        except (OSError, subprocess.TimeoutExpired, ValueError):
            raise ServiceError('OUTLINER_ENGINE_FAILED', '아웃라이너 엔진을 실행하지 못했습니다.') from None
        if result.returncode or not isinstance(value, dict):
            raise ServiceError('OUTLINER_ENGINE_FAILED', '아웃라이너 엔진 응답을 읽지 못했습니다.')
        if not value.get('ok'):
            error = value.get('error') or {}
            raise ServiceError(str(error.get('code', 'OUTLINER_FAILED')), str(error.get('message', '요청에 실패했습니다.')))
        return value.get('result')

    def close(self):
        self._closed.set()
        if self._transport is not None:
            self._transport.close()

    def execute(self, payload):
        action = payload.get('action')
        with self._lock:
            if self._closed.is_set():
                raise ServiceError('OUTLINER_ENGINE_CLOSED', '아웃라이너 엔진이 종료되었습니다.')
            if action == 'ui_mode':
                return self._ui_mode(payload)
            if action == 'open_native':
                return self._open_native()
            if action == 'list':
                pages = [{'name': p.stem, 'path': p.name} for p in sorted(self.root.glob('*.md')) if p.is_file()]
                return {'pages': pages, 'recent': [name for name in self._recent if any(p['name'] == name for p in pages)]}
            if action == 'create':
                path = self._path(payload.get('page'))
                if path.exists():
                    raise ServiceError('OUTLINER_PAGE_EXISTS', '같은 이름의 페이지가 이미 있습니다.')
                path.write_text('- \n', encoding='utf-8')
                return self._load(path.stem)
            if action == 'rename':
                source = self._path(payload.get('page'))
                target = self._path(payload.get('new_name'))
                if not source.exists():
                    raise ServiceError('OUTLINER_PAGE_MISSING', '페이지를 찾을 수 없습니다.')
                if target.exists() and source.resolve() != target.resolve():
                    raise ServiceError('OUTLINER_PAGE_EXISTS', '같은 이름의 페이지가 이미 있습니다.')
                source.rename(target)
                self._versions.pop(source.name, None)
                # 링크 일괄 갱신: 다른 모든 페이지 파일의 [[옛이름]]을 [[새이름]]으로 치환
                all_files = [str(p) for p in sorted(self.root.glob('*.md')) if p.is_file()]
                try:
                    update_result = self._engine({
                        'id': 'rename_page',
                        'op': 'rename_page',
                        'args': {
                            'old_name': source.stem,
                            'new_name': target.stem,
                            'files': all_files,
                            'preview': False,
                        }
                    })
                except ServiceError as error:
                    if error.code != 'OUTLINER_ENGINE_UNCERTAIN':
                        target.rename(source)
                    raise
                self._rename_history.append({'old': source.stem, 'new': target.stem})
                loaded = self._load(target.stem)
                if update_result:
                    loaded['updated_links'] = update_result.get('count', 0)
                    loaded['affected_files'] = update_result.get('affected_files', [])
                return loaded
            if action == 'undo_rename':
                if not self._rename_history:
                    raise ServiceError('OUTLINER_NO_HISTORY', '되돌릴 이름 변경 내역이 없습니다.')
                last = self._rename_history[-1]
                source = self._path(last['new'])
                target = self._path(last['old'])
                if not source.exists():
                    raise ServiceError('OUTLINER_PAGE_MISSING', '이름을 되돌릴 페이지를 찾을 수 없습니다.')
                source.rename(target)
                self._versions.pop(source.name, None)
                all_files = [str(p) for p in sorted(self.root.glob('*.md')) if p.is_file()]
                try:
                    self._engine({
                        'id': 'rename_page',
                        'op': 'rename_page',
                        'args': {
                            'old_name': last['new'],
                            'new_name': last['old'],
                            'files': all_files,
                            'preview': False,
                        }
                    })
                except ServiceError as error:
                    if error.code != 'OUTLINER_ENGINE_UNCERTAIN':
                        target.rename(source)
                    raise
                self._rename_history.pop()
                return self._load(target.stem)
            if action in {'check_delete', 'page_references'}:
                page_name = payload.get('page')
                path = self._path(page_name)
                referring, blocks = self._page_references(path)
                return {'page': path.stem, 'has_references': len(referring) > 0,
                        'count': len(referring), 'referring_pages': referring,
                        'referring_blocks': blocks}
            if action == 'delete':
                path = self._path(payload.get('page'))
                if not path.exists():
                    raise ServiceError('OUTLINER_PAGE_MISSING', '페이지를 찾을 수 없습니다.')
                # 참조 검사 옵션이 있는 경우
                if payload.get('check_references'):
                    referring, _ = self._page_references(path)
                    if referring and not payload.get('force'):
                        return {
                            'deleted': False,
                            'page': path.stem,
                            'has_references': True,
                            'referring_pages': referring,
                            'warning': f"'{path.stem}' 페이지를 참조하는 다른 페이지가 {len(referring)}개 있습니다."
                        }
                # 휴지통 보관으로 실행 취소 가능하게 처리
                trash_dir = self.root / '.trash'
                try:
                    trash_dir.mkdir(parents=True, exist_ok=True)
                    from datetime import datetime
                    stamp = datetime.now().strftime('%Y%m%d_%H%M%S')
                    trash_path = trash_dir / f"{path.stem}.{stamp}.md"
                    shutil.move(str(path), str(trash_path))
                    self._trash_history.append({'page': path.stem, 'trash_path': str(trash_path), 'original': str(path)})
                except OSError as exc:
                    raise ServiceError('OUTLINER_TRASH_FAILED', f'페이지를 휴지통으로 이동하지 못했습니다: {exc}') from exc
                self._versions.pop(path.name, None)
                self._recent = [name for name in self._recent if name != path.stem]
                return {'deleted': True, 'page': path.stem, 'can_restore': True}
            if action == 'restore':
                if not self._trash_history:
                    raise ServiceError('OUTLINER_NO_HISTORY', '복원할 삭제 페이지가 없습니다.')
                last = self._trash_history[-1]
                trash_path = Path(last['trash_path'])
                original_path = Path(last['original'])
                if not trash_path.exists():
                    raise ServiceError('OUTLINER_PAGE_MISSING', '휴지통에서 파일을 찾을 수 없습니다.')
                if original_path.exists():
                    raise ServiceError('OUTLINER_PAGE_EXISTS', '같은 이름의 페이지가 이미 있어 복원하지 못했습니다.')
                shutil.move(str(trash_path), str(original_path))
                self._trash_history.pop()
                return self._load(original_path.stem)
            if action == 'daily':
                from datetime import date
                filename = self._engine({'id': 'daily', 'op': 'daily_page', 'args': {'date': date.today().isoformat()}})['filename']
                return self._load(filename)
            if action == 'load':
                return self._load(payload.get('page'))
            if action == 'save':
                path = self._path(payload.get('page'))
                expected = payload.get('version')
                if not isinstance(expected, str) or self._digest(path) != expected:
                    raise ServiceError('OUTLINER_CONFLICT', '페이지가 다른 곳에서 변경되었습니다. 다시 불러온 뒤 편집해 주세요.')
                markdown = payload.get('markdown')
                if not isinstance(markdown, str):
                    raise ServiceError('INVALID_INPUT', '저장할 내용을 확인해 주세요.')
                self._engine({'id': 'save', 'op': 'save', 'args': {'path': str(path), 'markdown': markdown}})
                version = self._digest(path)
                self._versions[path.name] = version
                return {'page': path.stem, 'version': version, 'saved': True}
            if action in {'edit', 'key', 'history', 'search', 'backlinks', 'parse', 'resolve_block_ref', 'block_ref', 'insert_block_ref', 'block_backlinks'}:
                page = self._path(payload.get('page'))
                markdown = payload.get('markdown')
                if not isinstance(markdown, str):
                    raise ServiceError('INVALID_INPUT', '페이지 내용을 확인해 주세요.')
                args = dict(payload)
                args.pop('action', None)
                if action != 'history':
                    args.pop('page', None)  # history 는 page 로 이력 소속 페이지를 구분한다
                operation = args.pop('operation', None)
                if action in {'edit', 'key', 'history'}:
                    # 엔진은 op='edit' 아래 args.action 으로 편집 명령(set_text·insert·delete 등)을, op='history' 아래 args.action 으로
                    # 이력 명령(push·undo·redo·clear·status·rebase, 아웃라이너 UNDO1)을 받는다.
                    # 예전에는 편집 명령을 op 로 바로 보내 '지원하지 않는 명령: set_text' 오류가 났다(v2.2.83).
                    args['action'] = operation
                return self._engine({'id': action, 'op': action, 'document': markdown, 'args': args})
            raise ServiceError('INVALID_ACTION', '지원하지 않는 아웃라이너 작업입니다.')

    def _load(self, page_name):
        path = self._path(page_name)
        if not path.exists():
            path.write_text('- \n', encoding='utf-8')
        elif path.stat().st_size == 0:
            # New pages need one editable anchor block for the engine's insert operation.
            path.write_text('- \n', encoding='utf-8')
        result = self._engine({'id': 'load', 'op': 'load', 'args': {'path': str(path)}})
        version = self._digest(path)
        self._versions[path.name] = version
        self._recent = [name for name in self._recent if name != path.stem]
        self._recent.insert(0, path.stem)
        del self._recent[10:]
        return {'page': path.stem, 'path': path.name, 'markdown': result['markdown'], 'document': result['document'], 'version': version}
