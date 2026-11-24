"""Windows 한/글에서 명시적 RunContext의 실제 문서 파이프라인을 사본으로 시험한다."""
import argparse
from hashlib import sha256
import json
from pathlib import Path
import runpy
import shutil
import sys
from datetime import datetime, timezone
from time import perf_counter


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--mode', choices=('spacing', 'unify', 'format', 'all'), required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--trace-jsonl', type=Path,
                        help='단계 진입·종료를 실행 중 JSONL 파일에 즉시 기록한다.')
    args = parser.parse_args()
    if sys.platform != 'win32':
        parser.error('Windows + 한/글 COM 환경이 필요합니다.')
    repo = Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(repo))
    from docfit_core.app_state import RunOptions
    from docfit_core.hwpx import inspect_hwpx, validate_hwpx
    from docfit_engine.pipeline import PipelineCallbacks
    from docfit_engine.runtime import session_for

    source = args.source.resolve(strict=True)
    is_hwpx = source.suffix.lower() == '.hwpx'
    if is_hwpx:
        validate_hwpx(source)
    before = sha256(source.read_bytes()).hexdigest()
    args.out.mkdir(parents=True, exist_ok=False)
    copied = (args.out / ('source' + source.suffix.lower())).resolve()
    shutil.copy2(source, copied)
    ns = runpy.run_path(str(repo / 'hwp-auto-docfit.py'), run_name='engine_smoke')
    namespace = ns['작업_실행'].__globals__
    events = []
    messages = []
    def trace(kind, **fields):
        if args.trace_jsonl is None:
            return
        record = {'time': datetime.now(timezone.utc).isoformat(), 'kind': kind, **fields}
        args.trace_jsonl.parent.mkdir(parents=True, exist_ok=True)
        with args.trace_jsonl.open('a', encoding='utf-8') as stream:
            stream.write(json.dumps(record, ensure_ascii=False) + '\n')
            stream.flush()

    callbacks = PipelineCallbacks(
        event=events.append, log=messages.append,
        progress=lambda name: trace('stage_mark', stage=name),
        on_stage_enter=lambda name: trace('stage_start', stage=name),
        on_stage_exit=lambda name, ok: trace('stage_end', stage=name, ok=ok),
        on_stage_error=lambda name, error: trace(
            'stage_error', stage=name, error=f'{type(error).__name__}: {error}'),
    )
    options = RunOptions(mode=args.mode, verify_result=False, autoclose=True)
    context = session_for(namespace).new_context(options=options, callbacks=callbacks)
    started = perf_counter()
    trace('pipeline_start', source=str(source), copy=str(copied), mode=args.mode)
    ns['작업_실행']([str(copied)], context=context, callbacks=callbacks,
                  표준서식=args.mode in ('all', 'format'), 준말_등록={})
    trace('pipeline_return')
    errors = [event for event in events if event[0] in ('fatal_error', 'document_error')]
    saved = [event for event in events if event[0] == 'saved']
    outputs = [Path(event[2]) for event in saved]
    inspections = []
    for output in outputs:
        validate_hwpx(output)
        inspections.append(inspect_hwpx(output))
    # TXT 등은 한/글이 변환해 연다. 글자 보존 비교는 원문 텍스트를 기준으로 한다(서식 변환으로 다를 수 있다).
    if is_hwpx:
        original_text = ''.join(inspect_hwpx(copied).text.split())
    else:
        original_text = ''.join(copied.read_text(encoding='utf-8-sig', errors='replace').split())
    report = {
        'version': ns['APP_VERSION'], 'mode': args.mode,
        'seconds': round(perf_counter() - started, 3),
        'errors': errors, 'saved': len(saved),
        'output_files': [output.name for output in outputs],
        'page_counts': [event[3] if len(event) > 3 else None for event in saved],
        'paragraph_counts': [inspection.paragraph_count for inspection in inspections],
        'text_preserved_ignoring_whitespace': [
            ''.join(inspection.text.split()) == original_text for inspection in inspections],
        'original_unchanged': sha256(source.read_bytes()).hexdigest() == before,
        'copy_unchanged': sha256(copied.read_bytes()).hexdigest() == before,
        'stages_selected': context.state.get('선택_세부작업', {}),
        'table_spacing': context.state.get('표_자간조정_사용'),
    }
    (args.out / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    (args.out / 'engine.log').write_text('\n'.join(messages), encoding='utf-8')
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if errors or len(saved) != 1 or not report['original_unchanged'] or not report['copy_unchanged']:
        raise RuntimeError('실제 COM 파이프라인 또는 원본 보존 검사에 실패했습니다.')


if __name__ == '__main__':
    main()
