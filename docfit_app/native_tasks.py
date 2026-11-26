"""Rust 별도 창의 화면 없는 작업 프로세스. stdio JSON 외의 명령은 공개하지 않는다.

창 전달은 RUSTSIDE2, 문서 처리는 기존 서비스가 맡는다. 시작만으로 설치·상주·설정
저장·COM 생성은 하지 않는다. EOF 때 진행 작업에 협력 취소를 요청하고 정리를 기다린다.
"""
from __future__ import annotations

import argparse
import contextlib
import json
import os
from pathlib import Path
import sys

MAX_LINE = 4 * 1024 * 1024


class Session:
    def __init__(self, root, *, idioms=None, kvf=None, conversion=None, ktm=None, settings_path=None):
        from .file_rename_service import FileRenameService
        from .jobs import JobManager
        self.root = Path(root)
        self.rename = FileRenameService(self.root)
        self.jobs = JobManager()
        self.idioms = idioms
        self.kvf = kvf
        self.conversion = conversion
        self.conversion_job = None
        self.conversion_result = None
        from .settings_service import SettingsService
        self.settings = SettingsService(settings_path or self.root / 'settings.json')
        self.ktm = ktm
        self.runtime_snapshot = None

    def _idle(self):
        job = self.jobs.latest()
        worker = self.jobs.thread(job['id']) if job else None
        if job and (job['state'] not in {'done', 'failed', 'cancelled'} or (worker and worker.is_alive())):
            raise ValueError('작업이 진행 중입니다. 끝난 뒤 다시 시도해 주세요.')

    def dispatch(self, request):
        if not isinstance(request, dict) or not isinstance(request.get('payload', {}), dict):
            raise ValueError('명령 형식을 확인해 주세요.')
        op, p = request.get('op'), request.get('payload', {})
        if op == 'job_status':
            status = self.jobs.job_status(p.get('job_id'))
            if p.get('job_id') == self.conversion_job and self.conversion_result is not None:
                status['result'] = self.conversion_result
            return dict(ok=True, **status)
        if op == 'cancel':
            return dict(ok=True, **self.jobs.cancel(p.get('job_id')))
        self._idle()
        if op == 'rename':
            action = p.get('action')
            args = p.get('args', {})
            if not isinstance(args, dict):
                raise ValueError('이름 변경 입력을 확인해 주세요.')
            if action == 'add_paths':
                return self.rename.add_paths(args.get('paths', []), recursive=args.get('recursive') is True,
                    include_hidden=args.get('include_hidden') is True, as_folder=args.get('as_folder') is True)
            if action in {'apply', 'undo'}:
                runner = self.rename.job_runner(action, args)
                identifier = self.jobs.start('file_rename_' + action, {}, runner)
                return dict(ok=True, job_id=identifier)
            allowed = {'list', 'remove', 'clear', 'move', 'sort', 'preview', 'history', 'ruleset_list',
                       'ruleset_save', 'ruleset_delete', 'ruleset_import', 'ruleset_export',
                       'replace_mru_list', 'replace_mru_add', 'date_options_get', 'date_options_save'}
            if action not in allowed:
                raise ValueError('지원하지 않는 이름 변경 명령입니다.')
            return self.rename.command(action, args)
        if op == 'convert':
            from .conversion_runner import run_conversion
            from docfit_core.file_conversion import CONVERSION_OUTPUTS
            paths = p.get('paths')
            if not isinstance(paths, list) or not 1 <= len(paths) <= 2000 or p.get('target') not in CONVERSION_OUTPUTS:
                raise ValueError('변환할 파일·결과 형식을 확인해 주세요.')
            paths = [self.rename._default_validate(path, 'open') for path in paths]
            options = {}
            mb = p.get('pdf_target_mb', 10)
            if type(mb) not in (int, float) or not 1 <= mb <= 1000:
                raise ValueError('PDF 목표 크기는 1~1000 MB입니다.')
            options['pdf_target_bytes'] = int(mb * 1024 * 1024)
            method = p.get('pdf_compress_method', 'auto')
            if method not in {'auto', 'preserve_text', 'image_pdf'}:
                raise ValueError('PDF 압축 방식을 확인해 주세요.')
            options['pdf_compress_method'] = method
            out = p.get('output_dir') or None
            if out is not None:
                out = self.rename._default_validate(out, 'directory')
            if self.conversion:
                options['runtime_factory'] = self.conversion
            self.conversion_result = None
            def runner(report, cancelled):
                class Stop:
                    def is_set(self):
                        return cancelled()
                def progress(event):
                    if event[0] == 'doc_begin':
                        report('progress', dict(file=event[1], total=event[2], stage=event[3],
                            percent=(event[1] - 1) * 100 / event[2]))
                result = run_conversion(paths, p['target'], out, progress=progress, stop_event=Stop(), **options)
                if p['target'] == 'pdfm':
                    merged = next((row for row in result if row['status'] == 'success'), None)
                    if merged:
                        result = [dict(merged, source=path) for path in paths]
                self.conversion_result = result
                return result
            identifier = self.jobs.start('file_format', {}, runner)
            self.conversion_job = identifier
            return dict(ok=True, job_id=identifier)
        if op == 'idioms':
            from .idiom_service import IdiomService
            if self.idioms is None:
                # 앱과 같은 설정 HWPX 원본이 있으면 읽기 전용·직접 열기 흐름을 유지한다.
                from docfit_core.format_samples import APP_HOME
                self.idioms = IdiomService(config_root=Path(os.environ.get('DOCFIT_DOCUMENT_ROOT', APP_HOME)),
                    app_copy=Path(__file__).resolve().parents[1] / 'resources' / 'HWP.IDO')
            action = p.get('action', 'get')
            if action not in {'get', 'save_text', 'save_body', 'add_body', 'import_app'}:
                raise ValueError('지원하지 않는 상용구 명령입니다.')
            return dict(ok=True, **self.idioms.command(action, p.get('args', {})))
        if op == 'kvf':
            from .kvf_client import KvfClient
            if self.kvf is None:
                self.kvf = KvfClient()
            if p.get('action') not in {'status', 'enable', 'disable'}:
                raise ValueError('지원하지 않는 가상 폴더 명령입니다.')
            return self.kvf.command(p['action'], p.get('args', {}))
        if op == 'runtime_settings':
            from .settings_schema import DEFAULTS
            keys = ('always_on_top', 'mongttang_search')
            if p.get('action', 'get') == 'set':
                values = p.get('values')
                if not isinstance(values, dict) or set(values) != set(keys) or not all(type(v) is bool for v in values.values()):
                    raise ValueError('창·검색 설정 값을 확인해 주세요.')
                if self.runtime_snapshot is None:
                    raise ValueError('설정을 다시 읽은 뒤 저장해 주세요.')
                def change(current):
                    before = {k: current.get(k, DEFAULTS[k]) for k in keys}
                    if before != self.runtime_snapshot:
                        raise ValueError('다른 창에서 설정을 바꿨습니다. 다시 읽어 주세요.')
                    if values['mongttang_search'] != before['mongttang_search']:
                        from .ktm_service import KtmService
                        result = (self.ktm or KtmService()).apply_setting(values['mongttang_search'])
                        if not result.get('ok'):
                            raise ValueError(result.get('error') or result.get('message') or '검색 프로그램 설정 실패')
                    current.update(values)
                self.settings.update(change)
            elif p.get('action', 'get') != 'get':
                raise ValueError('지원하지 않는 설정 명령입니다.')
            current = self.settings.read()
            self.runtime_snapshot = {k: current.get(k, DEFAULTS[k]) for k in keys}
            return dict(ok=True, values=self.runtime_snapshot)
        if op == 'about':
            from . import settings_schema
            return dict(ok=True, developer='도토리만두', ui='Rust + Win32',
                        version=getattr(sys.modules.get('__main__'), 'APP_VERSION', '개발 소스'),
                        python=sys.version.split()[0], settings_keys=len(settings_schema.DEFAULTS),
                        licenses='Rust/windows-sys: MIT/Apache-2.0; Python: PSF; defusedxml: PSF; Pillow: HPND')
        if op == 'fonts':
            from docfit_core.hangul_fonts import catalog
            return dict(ok=True, fonts=[row['name'] for row in catalog()])
        raise ValueError('지원하지 않는 작업입니다.')

    def close(self):
        self.jobs.shutdown()


def serve(reader, writer, session):
    """한 줄에 요청/응답 하나. 잘못된 요청 뒤에도 다음 요청을 처리한다."""
    try:
        while True:
            line = reader.readline(MAX_LINE + 1)
            if not line:
                break
            if len(line) > MAX_LINE:
                writer.write(json.dumps(dict(ok=False, message='요청이 너무 큽니다.')) + '\n')
                writer.flush()
                break  # 남은 조각을 새 명령으로 해석하지 않는다.
            try:
                request = json.loads(line)
                with contextlib.redirect_stdout(sys.stderr):
                    response = session.dispatch(request)
            except Exception as exc:
                response = dict(ok=False, code=getattr(exc, 'code', 'NATIVE_TASK_FAILED'), message=str(exc))
            writer.write(json.dumps(response, ensure_ascii=False) + '\n')
            writer.flush()
    finally:
        session.close()


def main(arguments=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('--data-dir', required=True)
    parser.add_argument('--settings')
    args = parser.parse_args(arguments)
    # PyInstaller windowed EXE는 상속된 파이프가 있어도 sys.std*를 None으로 둔다.
    if os.name == 'nt' and (sys.stdin is None or sys.stdout is None):
        import ctypes
        import msvcrt
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.GetStdHandle.argtypes = [ctypes.c_ulong]
        kernel.GetStdHandle.restype = ctypes.c_void_p
        for name, code, mode, flag in [('stdin', -10, 'r', os.O_RDONLY), ('stdout', -11, 'w', os.O_WRONLY)]:
            if getattr(sys, name) is None:
                handle = kernel.GetStdHandle(code & 0xffffffff)
                if not handle or handle == ctypes.c_void_p(-1).value:
                    raise RuntimeError('Rust 창이 연결한 입출력 파이프가 없습니다.')
                setattr(sys, name, os.fdopen(msvcrt.open_osfhandle(handle, flag), mode, encoding='utf-8'))
        if sys.stderr is None:
            sys.stderr = open(os.devnull, 'w', encoding='utf-8')
    if hasattr(sys.stdin, 'reconfigure'):
        sys.stdin.reconfigure(encoding='utf-8')
        sys.stdout.reconfigure(encoding='utf-8')
    serve(sys.stdin, sys.stdout, Session(Path(args.data_dir), settings_path=args.settings))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
