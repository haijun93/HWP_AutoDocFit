"""Desktop adapter for the isolated Rust PDF page editor."""
from __future__ import annotations

import os
import sys
from pathlib import Path
import shutil
import subprocess

from .api_contract import ServiceError


class PdfEditService:
    def __init__(self, executable=None, runner=subprocess.run):
        self.executable = Path(executable) if executable else self._find_executable()
        self.runner = runner

    @staticmethod
    def _find_executable():
        name = 'pdf-edit.exe' if os.name == 'nt' else 'pdf-edit'
        app_root = Path(__file__).resolve().parents[1]
        candidates = []
        # PyInstaller onedir installs resources beside the app under _internal;
        # onefile builds expose bundled resources through _MEIPASS.
        for base in (Path(sys.executable).resolve().parent,
                     Path(getattr(sys, '_MEIPASS', app_root))):
            candidates.append(base / '_internal' / 'resources' / name)
            candidates.append(base / 'resources' / name)
        candidates.extend((app_root / 'resources' / name,
                           app_root / 'pdf_edit_core' / 'target' / 'release' / name,
                           app_root / name))
        for candidate in candidates:
            if candidate.is_file():
                return candidate
        located = shutil.which('pdf-edit')
        return Path(located) if located else candidates[0]

    @staticmethod
    def _classify(path):
        try:
            raw = path.read_bytes()
        except OSError:
            raise ServiceError('PDF_UNAVAILABLE', '선택한 PDF를 읽을 수 없습니다.') from None
        if len(raw) > 1024 * 1024 * 1024:
            raise ServiceError('PDF_TOO_LARGE', '1GB보다 큰 PDF는 편집할 수 없습니다.')
        if not raw.startswith(b'%PDF-') or b'%%EOF' not in raw[-4096:]:
            raise ServiceError('PDF_DAMAGED', 'PDF 파일이 손상되었거나 PDF 형식이 아닙니다.')
        if b'/Encrypt' in raw and b'/P' not in raw:
            raise ServiceError('PDF_ENCRYPTED', '암호가 설정된 PDF는 현재 편집할 수 없습니다.')
        try:
            from pypdf import PdfReader
            reader = PdfReader(str(path), strict=True)
            encrypt = reader.trailer.get('/Encrypt')
            if reader.is_encrypted or encrypt is not None:
                if reader.is_encrypted and not reader.decrypt(''):
                    raise ServiceError('PDF_ENCRYPTED', '열기 암호가 설정된 PDF는 현재 편집할 수 없습니다.')
                if encrypt is not None:
                    permissions = encrypt.get_object().get('/P')
                    if permissions is not None and int(permissions) != -1:
                        raise ServiceError('PDF_RESTRICTED', '권한 제한이 설정된 PDF는 현재 편집할 수 없습니다.')
                raise ServiceError('PDF_ENCRYPTED', '암호가 설정된 PDF는 현재 편집할 수 없습니다.')
            if not reader.pages:
                raise ServiceError('PDF_DAMAGED', '페이지가 없는 손상 PDF입니다.')
            if any(page.get('/Annots') and any(str(a.get_object().get('/FT', '')) == '/Sig'
                                                for a in page['/Annots']) for page in reader.pages):
                raise ServiceError('PDF_SIGNED', '전자서명된 PDF는 편집할 수 없습니다. 서명이 무효화됩니다.')
        except ServiceError:
            raise
        except Exception:
            raise ServiceError('PDF_DAMAGED', 'PDF 구조를 읽을 수 없습니다. 손상된 PDF일 수 있습니다.') from None

    def execute(self, action, source, output, *, pages=None, degrees=None, sources=None):
        if action not in {'delete', 'reorder', 'rotate', 'split', 'merge'}:
            raise ServiceError('INVALID_ACTION', '지원하지 않는 PDF 편집 작업입니다.')
        source_path = Path(source) if source else None
        output_path = Path(output) if output is not None else None
        if action != 'merge' and (source_path is None or not source_path.is_file() or source_path.suffix.lower() != '.pdf'):
            raise ServiceError('INVALID_INPUT', '원본 PDF 파일을 선택해 주세요.')
        if action == 'split':
            if output_path is None or output_path.exists():
                raise ServiceError('OUTPUT_EXISTS', '결과 폴더는 기존 경로와 다른 새 폴더여야 합니다.')
        else:
            if output_path is None or output_path.suffix.lower() != '.pdf' or output_path.exists():
                raise ServiceError('OUTPUT_EXISTS', '결과 경로는 기존 파일과 다른 새 PDF 경로여야 합니다.')
            try:
                if source_path is not None and source_path.resolve() == output_path.resolve():
                    raise ServiceError('OUTPUT_EXISTS', '원본 파일을 덮어쓸 수 없습니다. 새 결과 경로를 선택해 주세요.')
            except OSError:
                raise ServiceError('INVALID_INPUT', '파일 경로를 확인해 주세요.') from None
        if source_path is not None:
            self._classify(source_path)
        if not self.executable.is_file():
            raise ServiceError('PDF_ENGINE_MISSING', 'PDF 편집 엔진(pdf-edit)을 찾을 수 없습니다. 앱 설치 상태를 확인해 주세요.')
        if action == 'merge':
            inputs = [Path(p) for p in (sources or [])] if isinstance(sources, (list, tuple)) else []
            if len(inputs) < 2 or any(not p.is_file() or p.suffix.lower() != '.pdf' for p in inputs):
                raise ServiceError('INVALID_INPUT', '합칠 PDF 파일을 두 개 이상 선택해 주세요.')
            if any(p.resolve() == output_path.resolve() for p in inputs):
                raise ServiceError('OUTPUT_EXISTS', '결과 파일은 합칠 원본 파일과 다른 경로여야 합니다.')
            for item in inputs:
                self._classify(item)
            args = ['merge', str(output_path), *map(str, inputs)]
        elif not isinstance(pages, (list, tuple)) or not pages or any(type(p) is not int or p < 1 for p in pages):
            raise ServiceError('INVALID_INPUT', '편집할 페이지 번호를 확인해 주세요.')
        elif action == 'split':
            if any(type(p) is not int or p < 1 for p in pages):
                raise ServiceError('INVALID_INPUT', '분할 페이지 범위를 확인해 주세요.')
            if pages != sorted(set(pages)):
                raise ServiceError('INVALID_INPUT', '분할 페이지는 오름차순으로 중복 없이 입력해 주세요.')
            ranges = []
            start = previous = pages[0]
            for page in pages[1:]:
                if page == previous + 1:
                    previous = page
                else:
                    ranges.append(f'{start}-{previous}')
                    start = previous = page
            ranges.append(f'{start}-{previous}')
            args = ['split', str(source_path), str(output_path), ','.join(ranges)]
        elif action == 'rotate':
            if type(degrees) is not int or degrees % 90:
                raise ServiceError('INVALID_INPUT', '회전 각도는 90도의 배수여야 합니다.')
            args = ['rotate', str(source_path), str(output_path), str(degrees), ','.join(map(str, pages))]
        else:
            args = [action, str(source_path), str(output_path), ','.join(map(str, pages))]
        try:
            result = self.runner([str(self.executable), *args], capture_output=True, text=True, timeout=300, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise ServiceError('PDF_ENGINE_FAILED', 'PDF 편집 엔진을 실행하지 못했습니다.') from None
        if result.returncode:
            message = (result.stderr or '').strip()
            if output_path.exists() and output_path.is_file():
                output_path.unlink()
            raise ServiceError('PDF_EDIT_FAILED', message or 'PDF 편집에 실패했습니다.')
        if action == 'split':
            if not output_path.is_dir() or not list(output_path.glob('part-*.pdf')):
                raise ServiceError('PDF_EDIT_FAILED', 'PDF 편집 엔진이 분할 결과를 만들지 않았습니다.')
        elif not output_path.is_file():
            raise ServiceError('PDF_EDIT_FAILED', 'PDF 편집 엔진이 결과 파일을 만들지 않았습니다.')
        return {'output': str(output_path), 'action': action}
