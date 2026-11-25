"""KORDOCD1 단독 실행 측정: 새 Python 프로세스 + 자식의 최대 commit 메모리.

--out은 저장소 밖 새 디렉터리. 각 엔진/작업에 단건 1회와 연속 20건 1회만 실행.
Windows Job Object의 PeakJobMemoryUsed를 사용하므로 RSS와 혼동하지 않는다.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')


class Job:
    """자식 Node/klib까지 포함한 커널 최대 commit 계측."""
    class IO(C.Structure):
        _fields_ = [(n, C.c_ulonglong) for n in ('read_ops', 'write_ops', 'other_ops', 'read_bytes', 'write_bytes', 'other_bytes')]

    class Basic(C.Structure):
        _fields_ = [('process_time', C.c_longlong), ('job_time', C.c_longlong), ('flags', W.DWORD),
                    ('min_ws', C.c_size_t), ('max_ws', C.c_size_t), ('active', W.DWORD),
                    ('affinity', C.c_size_t), ('priority', W.DWORD), ('scheduling', W.DWORD)]

    def __init__(self, process):
        class Extended(C.Structure):
            _fields_ = [('basic', Job.Basic), ('io', Job.IO), ('process_limit', C.c_size_t),
                        ('job_limit', C.c_size_t), ('peak_process', C.c_size_t), ('peak_job', C.c_size_t)]
        self.extended = Extended
        self.kernel = C.WinDLL('kernel32', use_last_error=True)
        self.kernel.CreateJobObjectW.restype = W.HANDLE
        self.kernel.AssignProcessToJobObject.argtypes = [W.HANDLE, W.HANDLE]
        self.kernel.QueryInformationJobObject.argtypes = [W.HANDLE, C.c_int, C.c_void_p, W.DWORD, C.c_void_p]
        self.kernel.CloseHandle.argtypes = [W.HANDLE]
        self.handle = self.kernel.CreateJobObjectW(None, None)
        if not self.handle or not self.kernel.AssignProcessToJobObject(self.handle, W.HANDLE(int(process._handle))):
            raise C.WinError(C.get_last_error())

    def peak(self):
        info = self.extended()
        if not self.kernel.QueryInformationJobObject(self.handle, 9, C.byref(info), C.sizeof(info), None):
            raise C.WinError(C.get_last_error())
        return info.peak_job

    def close(self):
        self.kernel.CloseHandle(self.handle)


def operation(source, target, engine, task):
    from docfit_core import kordoc_bridge as kb
    if task == 'extract':
        if engine == 'native':
            done = subprocess.run([str(kb._klib_executable()), 'extract', str(source)], capture_output=True,
                                  check=True, timeout=90, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            paragraphs = json.loads(done.stdout)['paragraphs']
            text = '\n'.join(p['text'] for p in paragraphs)
            target.write_text(text, encoding='utf-8')
        else:
            # parse_document의 Rust 우선 분기를 의도적으로 우회한다.
            from docfit_core.conversion_routing import _expected
            from docfit_core.markdown_hwpx import parse_markdown
            markdown = target.with_suffix('.md')
            kb.run_kordoc([str(source), '--format', 'markdown', '-o', str(markdown), '--silent'])
            # B2의 HTML 태그 제거 정규식은 정상 리터럴 '< >'도 지운다.
            # 실제 생성기가 읽는 의미 모델을 써서 본문/표 셀 순서만 정규화한다.
            target.write_text(_expected(parse_markdown(markdown.read_text(encoding='utf-8')))['text'], encoding='utf-8')
        return []
    if engine == 'native':
        from docfit_core.markdown_hwpx import generate_hwpx
        if source.suffix == '.md':
            markdown, image_dir, warnings = source.read_text(encoding='utf-8'), source.parent, ()
        else:
            from docfit_core.input_markdown import adapt_to_markdown
            adapted = adapt_to_markdown(source, target.parent / (target.stem + '-assets'))
            markdown, image_dir, warnings = adapted.markdown, adapted.image_dir, adapted.warnings
        result = generate_hwpx(markdown, target, image_dir=image_dir)
        return [*warnings, *result.warnings]
    if source.suffix == '.md':
        kb.generate_hwpx(source, '보고서', target, image_dir=source.parent)
    elif source.suffix == '.html':
        # 제품의 HTML 경로: 번들 Pandoc -> MD -> kordoc generate.
        from docfit_core.file_conversion import convert_to_pandoc_format
        markdown = target.with_suffix('.md')
        convert_to_pandoc_format(source, markdown)
        kb.generate_hwpx(markdown, '보고서', target, image_dir=source.parent)
    else:
        kb.convert_to_hwpx(source, target)
    return []


def worker(args):
    sys.stdin.readline()  # 부모가 Job에 연결하기 전에 변환을 시작하지 않는다.
    manifest = json.loads((args.out / 'manifest.json').read_text(encoding='utf-8'))
    cases = [c for c in manifest if c['task'] == args.task and c['format'] == args.format]
    folder = args.out / f'{args.task}-{args.format}-{args.engine}-{args.count}'
    folder.mkdir()
    rows = []
    for i in range(args.count):
        case = cases[i % len(cases)]
        target = folder / f'{i:02}-{case["id"]}{".txt" if args.task == "extract" else ".hwpx"}'
        row = {'id': case['id'], 'output': target.relative_to(args.out).as_posix()}
        start = time.perf_counter()
        try:
            row['warnings'] = operation(args.out / case['file'], target, args.engine, args.task)
        except Exception as exc:
            row['error'] = f'{type(exc).__name__}: {exc}'
        row['operation_seconds'] = time.perf_counter() - start
        rows.append(row)
    write_json(folder / 'outputs.json', rows)
    return 0


def measure(args):
    manifest = json.loads((args.out / 'manifest.json').read_text(encoding='utf-8'))
    groups = sorted({(c['task'], c['format']) for c in manifest if args.task is None or c['task'] == args.task})
    summary = json.loads((args.out / 'timing.json').read_text(encoding='utf-8')) if (args.out / 'timing.json').exists() else []
    for task, fmt in groups:
        for engine in ('native', 'kordoc'):
            if args.engine is not None and engine != args.engine:
                continue
            for count in (1, 20):
                start = time.perf_counter()
                process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), 'worker', '--out', str(args.out),
                                            '--task', task, '--format', fmt, '--engine', engine, '--count', str(count)],
                                           stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                           creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
                job = Job(process)
                try:
                    stdout, stderr = process.communicate(b'go\n', timeout=1100)
                    seconds = time.perf_counter() - start
                    peak = job.peak()
                except BaseException:
                    process.kill()
                    process.communicate()
                    raise
                finally:
                    job.close()
                if process.returncode:
                    raise RuntimeError(stderr.decode('utf-8', 'replace'))
                row = {'task': task, 'format': fmt, 'engine': engine, 'count': count,
                       'seconds_with_startup': seconds, 'peak_job_commit_bytes': peak}
                summary.append(row)
                write_json(args.out / 'timing.json', summary)
                print(json.dumps(row), flush=True)
    return 0


def prepare(out):
    from docfit_core import conversion_fixtures as fx
    from docfit_core.markdown_hwpx_fixtures import fixtures_v2, write_images
    out.mkdir(parents=True, exist_ok=False)
    sources = out / 'sources'
    sources.mkdir()
    write_images(sources)
    manifest = []

    def add(name, fmt, expected, task='convert'):
        p = sources / f'{name}.{fmt}'
        manifest.append({'id': name, 'format': fmt, 'file': p.relative_to(out).as_posix(), 'task': task,
                         'sha256': hashlib.sha256(p.read_bytes()).hexdigest(), 'expected': expected})

    for name, (markdown, expected) in fixtures_v2().items():
        (sources / f'{name}.md').write_text(markdown, encoding='utf-8')
        add(name, 'md', expected)
    (sources / 'image1.png').write_bytes(fx.png_bytes())
    docs = {**fx.DOC_SPECS,
            'doc_image': [('h', 1, '그림 보고서'), ('img', '현황'), ('p', '그림 다음 문단입니다.')],
            'doc_nested': [('h', 1, '계층'), ('h', 2, '주제'), ('ul', [(0, '상위'), (1, '하위'), (2, '세부')]),
                           ('ol', [(0, '첫 순서'), (0, '둘째 순서')])]}
    for name, blocks in docs.items():
        fx.build_docx(blocks, sources / f'{name}.docx')
        (sources / f'{name}.html').write_text(fx.render_html(blocks), encoding='utf-8')
        for fmt in ('docx', 'html'):
            add(name, fmt, fx.expected_from_blocks(blocks))
    sheets = {**fx.SHEET_SPECS,
              'sheet_empty_cells': [('공백', [['키', '값', '메모'], ['가', '', '끝'], ['', '20', '']])],
              'sheet_unicode': [('다국어', [['한국어', 'English'], ['특수 & < >', '1234.50']])],
              'sheet_wide': [('넓은 표', [[f'열{i}' for i in range(6)], [str(i) for i in range(6)]])],
              'sheet_tall': [('긴 표', [['번호', '이름']] + [[str(i), f'항목 {i}'] for i in range(15)])]}
    for name, spec in sheets.items():
        fx.build_xlsx(spec, sources / f'{name}.xlsx')
        add(name, 'xlsx', fx.expected_from_sheets(spec))
    slides = {**fx.SLIDE_SPECS,
              'slides_titles': [{'title': '첫 제목'}, {'title': '둘째 제목'}],
              'slides_nested': [{'title': '목록', 'bullets': [(0, '위'), (1, '중간'), (2, '아래')]}],
              'slides_table': [{'title': '집계', 'table': [['부서', '건수'], ['총무', '15']]}],
              'slides_image': [{'title': '사진', 'image': True}]}
    for name, spec in slides.items():
        fx.build_pptx(spec, sources / f'{name}.pptx')
        add(name, 'pptx', fx.expected_from_slides(spec))
    # COM 전용 후속 단계에서 합성 HWP/HWPX 추출 입력을 더한다.
    write_json(out / 'manifest.json', manifest)
    return 0


def prepare_extract(out):
    from docfit_core.markdown_hwpx import generate_hwpx
    from docfit_core.save_guard import com_test_lock
    import pythoncom
    import win32com.client
    from verify_table_catalog_paste import _register
    manifest = json.loads((out / 'manifest.json').read_text(encoding='utf-8'))
    specs = [
        ('body', '한국어 본문과 English 12345입니다.\n\n두 번째 문단입니다.', '한국어 본문과 English 12345입니다.\n두 번째 문단입니다.'),
        ('heading', '# 제목\n\n## 중제목\n\n본문 내용입니다.', '제목\n중제목\n본문 내용입니다.'),
        ('table', '| 항목 | 값 |\n|---|---|\n| 접수 | 120 |\n| 처리 | 118 |', '항목\n값\n접수\n120\n처리\n118'),
        ('break', '앞 문장<br>뒤 문장\n\n특수 & < > 문장', '앞 문장\n뒤 문장\n특수 & < > 문장'),
        ('long', '\n\n'.join(f'합성 문단 {i}: 주민 안내 및 예산 집행 점검입니다.' for i in range(200)),
         '\n'.join(f'합성 문단 {i}: 주민 안내 및 예산 집행 점검입니다.' for i in range(200))),
    ]
    rows = []
    with com_test_lock('KORDOCD1', '합성 추출 입력 HWP/HWPX 저장', timeout_sec=60):
        pythoncom.CoInitialize()
        app = None
        try:
            app = win32com.client.DispatchEx('HwpFrame.HwpObject')
            _register(app)
            app.SetMessageBoxMode(0x00020000)
            for name, markdown, text in specs:
                original = out / 'sources' / f'extract-{name}.hwpx'
                generate_hwpx(markdown, original)
                if not app.Open(str(original.resolve()), 'HWPX', 'forceopen:true;versionwarning:false'):
                    raise RuntimeError(f'합성 추출 입력 열기 실패: {name}')
                row = {'id': name, 'open': True, 'pages': int(app.PageCount), 'version': str(app.Version)}
                for fmt in ('hwp', 'hwpx'):
                    target = out / 'sources' / f'extract-{name}-saved.{fmt}'
                    if not app.SaveAs(str(target.resolve()), fmt.upper(), ''):
                        raise RuntimeError(f'합성 입력 저장 실패: {name}/{fmt}')
                    manifest.append({'id': name, 'format': fmt, 'task': 'extract',
                                     'file': target.relative_to(out).as_posix(),
                                     'sha256': hashlib.sha256(target.read_bytes()).hexdigest(), 'expected': {'text': text}})
                rows.append(row)
                app.Clear(1)
        finally:
            if app is not None:
                app.Clear(1)
                app.Quit()
            pythoncom.CoUninitialize()
    write_json(out / 'extract-input-com.json', rows)
    write_json(out / 'manifest.json', manifest)
    return 0


def quality(out):
    from docfit_core import conversion_fidelity as cf
    from docfit_core.hwpx import validate_hwpx
    manifest = json.loads((out / 'manifest.json').read_text(encoding='utf-8'))
    index = {(c['task'], c['format'], c['id']): c for c in manifest}
    rows = []
    for folder in sorted(out.glob('*-20')):
        task, fmt, engine, _ = folder.name.split('-')
        seen = set()
        for output in json.loads((folder / 'outputs.json').read_text(encoding='utf-8')):
            if output['id'] in seen:
                continue
            seen.add(output['id'])
            case = index[(task, fmt, output['id'])]
            row = {'task': task, 'format': fmt, 'engine': engine, **output}
            if 'error' not in output:
                target = out / output['output']
                try:
                    if task == 'convert':
                        validate_hwpx(target, structure_only=True, parse_xml=True)
                        row['quality'] = cf.compare(dict(case['expected'], form='hwpx'), cf.extract_hwpx(target))
                        row['valid_hwpx'] = True
                    else:
                        row['quality'] = cf.compare(cf.extract_text(case['expected']['text']), cf.extract_text(target.read_text(encoding='utf-8')))
                except Exception as exc:
                    row['error'] = f'{type(exc).__name__}: {exc}'
            rows.append(row)
    write_json(out / 'quality.json', rows)
    print(json.dumps([{'task': t, 'format': f, 'engine': e,
                       'pass': sum(r.get('quality', {}).get('verdict', {}).get('pass', False) for r in rows if (r['task'], r['format'], r['engine']) == (t, f, e)),
                       'total': sum((r['task'], r['format'], r['engine']) == (t, f, e) for r in rows)}
                      for t, f, e in sorted({(r['task'], r['format'], r['engine']) for r in rows})], ensure_ascii=False, indent=2))
    return 0


def verify_com(out):
    from docfit_core import conversion_fidelity as cf
    from docfit_core.save_guard import com_test_lock
    from docfit_core.hwpx import validate_hwpx
    import pythoncom
    import win32com.client
    from verify_table_catalog_paste import _register
    rows = json.loads((out / 'quality.json').read_text(encoding='utf-8'))
    manifest = json.loads((out / 'manifest.json').read_text(encoding='utf-8'))
    for case in manifest:
        if hashlib.sha256((out / case['file']).read_bytes()).hexdigest() != case['sha256']:
            raise ValueError('합성 입력 해시 불일치')
    target_dir = out / 'roundtrip'
    target_dir.mkdir(exist_ok=False)
    results = []
    with com_test_lock('KORDOCD1', 'native 합성 결과 열기/사본 저장/쪽 수', timeout_sec=60):
        pythoncom.CoInitialize()
        app = None
        try:
            app = win32com.client.DispatchEx('HwpFrame.HwpObject')
            _register(app)
            app.SetMessageBoxMode(0x00020000)
            for row in rows:
                if row['engine'] != 'native' or row['task'] != 'convert' or 'error' in row:
                    continue
                result = {'id': row['id'], 'format': row['format'], 'version': str(app.Version)}
                try:
                    source = out / row['output']
                    target = target_dir / f'{row["format"]}-{row["id"]}.hwpx'
                    if not app.Open(str(source.resolve()), 'HWPX', 'forceopen:true;versionwarning:false'):
                        raise RuntimeError('한/글 열기 실패')
                    result.update(open=True, pages=int(app.PageCount))
                    if not app.SaveAs(str(target.resolve()), 'HWPX', ''):
                        raise RuntimeError('한/글 사본 저장 실패')
                    result['save'] = True
                    validate_hwpx(target, structure_only=True, parse_xml=True)
                    result['quality'] = cf.compare(cf.extract_hwpx(source), cf.extract_hwpx(target))
                except Exception as exc:
                    result['error'] = f'{type(exc).__name__}: {exc}'
                finally:
                    app.Clear(1)
                results.append(result)
                write_json(out / 'com.json', results)
                print(json.dumps(result, ensure_ascii=False), flush=True)
        finally:
            if app is not None:
                app.Clear(1)
                app.Quit()
            pythoncom.CoUninitialize()
    return 0 if results and all(r.get('save') and r.get('pages', 0) > 0 and r.get('quality', {}).get('verdict', {}).get('pass') for r in results) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('prepare', 'prepare-extract', 'measure', 'worker', 'quality', 'com'))
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--task')
    parser.add_argument('--format')
    parser.add_argument('--engine')
    parser.add_argument('--count', type=int)
    args = parser.parse_args()
    if args.out.resolve().is_relative_to(ROOT):
        parser.error('산출폴더는 저장소 밖이어야 합니다.')
    if args.mode == 'worker':
        return worker(args)
    return {'prepare': prepare, 'prepare-extract': prepare_extract, 'quality': quality, 'com': verify_com}.get(args.mode, lambda out: measure(args))(args.out)


if __name__ == '__main__':
    raise SystemExit(main())
