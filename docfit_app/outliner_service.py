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


class OutlinerService:
    def __init__(self, data_root, executable=None, runner=subprocess.run):
        self.root = (Path(data_root) / 'Outliner').resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        self.executable = Path(executable) if executable else self._find_executable()
        self.runner = runner
        self._lock = threading.RLock()
        self._versions: dict[str, str] = {}
        self._recent: list[str] = []

    @staticmethod
    def _find_executable():
        name = 'outliner.exe' if os.name == 'nt' else 'outliner'
        app_root = Path(__file__).resolve().parents[1]
        candidates = []
        for base in (Path(sys.executable).resolve().parent, Path(getattr(sys, '_MEIPASS', app_root))):
            candidates.extend((base / '_internal' / 'resources' / name, base / 'resources' / name))
        candidates.extend((app_root / 'resources' / name, app_root / 'outliner_core' / 'target' / 'release' / name))
        for candidate in candidates:
            if candidate.is_file():
                return candidate
        found = shutil.which('outliner')
        return Path(found) if found else candidates[0]

    def _path(self, value):
        if not isinstance(value, str) or not value or Path(value).name != value or value in {'.', '..'}:
            raise ServiceError('INVALID_PATH', '페이지 이름을 확인해 주세요.')
        name = value if value.endswith('.md') else f'{value}.md'
        path = (self.root / name).resolve()
        if path.parent != self.root or path.suffix.lower() != '.md':
            raise ServiceError('INVALID_PATH', '사용자 데이터 폴더 밖의 경로는 사용할 수 없습니다.')
        return path

    @staticmethod
    def _digest(path):
        try:
            return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None
        except OSError:
            raise ServiceError('OUTLINER_UNAVAILABLE', '페이지를 읽을 수 없습니다.') from None

    def _engine(self, request):
        if not self.executable.is_file():
            raise ServiceError('OUTLINER_ENGINE_MISSING', '아웃라이너 엔진을 찾을 수 없습니다.')
        try:
            result = self.runner([str(self.executable)], input=json.dumps(request, ensure_ascii=False) + '\n',
                                 capture_output=True, text=True, encoding='utf-8', timeout=30, check=False)
            value = json.loads(result.stdout)
        except (OSError, subprocess.TimeoutExpired, ValueError):
            raise ServiceError('OUTLINER_ENGINE_FAILED', '아웃라이너 엔진을 실행하지 못했습니다.') from None
        if result.returncode or not isinstance(value, dict):
            raise ServiceError('OUTLINER_ENGINE_FAILED', '아웃라이너 엔진 응답을 읽지 못했습니다.')
        if not value.get('ok'):
            error = value.get('error') or {}
            raise ServiceError(str(error.get('code', 'OUTLINER_FAILED')), str(error.get('message', '요청에 실패했습니다.')))
        return value.get('result')

    def execute(self, payload):
        action = payload.get('action')
        with self._lock:
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
                if target.exists():
                    raise ServiceError('OUTLINER_PAGE_EXISTS', '같은 이름의 페이지가 이미 있습니다.')
                source.rename(target)
                self._versions.pop(source.name, None)
                return self._load(target.stem)
            if action == 'delete':
                path = self._path(payload.get('page'))
                if not path.exists():
                    raise ServiceError('OUTLINER_PAGE_MISSING', '페이지를 찾을 수 없습니다.')
                path.unlink()
                self._versions.pop(path.name, None)
                self._recent = [name for name in self._recent if name != path.stem]
                return {'deleted': True, 'page': path.stem}
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
            if action in {'edit', 'search', 'backlinks', 'parse'}:
                page = self._path(payload.get('page'))
                markdown = payload.get('markdown')
                if not isinstance(markdown, str):
                    raise ServiceError('INVALID_INPUT', '페이지 내용을 확인해 주세요.')
                args = dict(payload)
                args.pop('action', None)
                args.pop('page', None)
                operation = args.pop('operation', None)
                return self._engine({'id': action, 'op': operation if action == 'edit' else action, 'document': markdown, 'args': args})
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
