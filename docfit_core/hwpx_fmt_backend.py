"""선행 구조 서식의 선택적 Rust backend. 기본값은 기존 Python 함수다.

DOCFIT_HWPXFMT5=1, HWPX_FMT_BIN=<실행 파일>로 켠다. 실패/미지원은
동일 입력에서 Python 조사를 다시 수행한다. Rust 산출물은 성공 후에만 채택한다.
"""
from __future__ import annotations

import functools
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

PROCESSORS = {'보고서머리_hwpx_처리': 'report_header', '제목_hwpx_처리': 'title',
              '중제목_hwpx_처리': 'midtitle', '붙임_hwpx_처리': 'attachment',
              '상자서식_hwpx_처리': 'box'}


def configured_processor(processor, log=None, *, namespace=None):
    """기존 (source, target=None, selections=None) API를 유지한다."""
    name = PROCESSORS.get(getattr(processor, '__name__', ''))
    if os.environ.get('DOCFIT_HWPXFMT5') != '1' or name is None:
        return processor

    def emit(state, detail=''):
        if log:
            log(f'[HWPXFMT5] {name} {state}' + (f': {detail}' if detail else ''))

    # 각 선행 단계의 조사 → 적용 전이를 한곳에서 기억한다.
    # Python 조사를 사용한 경우에는 그 선택을 그대로 Python에 돌려준다.
    survey_state = {'key': None, 'backend': None}

    @functools.wraps(processor)
    def run(source, target=None, selections=None):
        binary = os.environ.get('HWPX_FMT_BIN') or shutil.which('hwpx_fmt')
        # RunContext 안의 facade 함수는 최초 앱 이름공간을 가리킬 수 있다.
        # 선행 실행기는 현재 실행의 이름공간을 명시적으로 넘긴다.
        g = namespace if namespace is not None else processor.__globals__
        cfg = {'format_samples': g.get('활성_서식표_프로필'),
               'title_owner_text': g.get('제목_담당자_글') or '',
               'midtitle_number_bold': bool(g.get('중제목_번호굵게', True)),
               'marker_scheme_active': bool(g.get('활성_항목기호_체계')),
               'title_parenthesis_reduce': bool(g.get('괄호_축소_사용', True)),
               'title_parenthesis_pt': g.get('괄호_축소_pt', 2) or 2,
               'title_subtitle_pt': g.get('제목_부제_크기_pt', 15) or 15}
        key = None
        try:
            key = (str(source), json.dumps(cfg, ensure_ascii=False, sort_keys=True))
            if target is not None and survey_state == {'key': key, 'backend': 'python'}:
                return processor(source, target, selections)
            if not binary:
                raise RuntimeError('HWPX_FMT_BIN 실행 파일 없음')
            with tempfile.TemporaryDirectory(prefix='docfit_hwpxfmt5_') as folder:
                folder = Path(folder)
                config = folder / 'config.json'
                config.write_text(json.dumps(cfg, ensure_ascii=False), encoding='utf-8')
                command = [str(binary), 'survey', name, str(source)]
                if target is not None:
                    sel = folder / 'selections.json'
                    sel.write_text(json.dumps(selections, ensure_ascii=False), encoding='utf-8')
                    result = folder / 'result.hwpx'
                    command = [str(binary), name, str(source), str(result)]
                    if selections is not None:
                        command += ['--selections', str(sel)]
                command += ['--config', str(config)]
                completed = subprocess.run(command, capture_output=True, text=True,
                                           encoding='utf-8', timeout=120)
                if completed.returncode:
                    raise RuntimeError(completed.stderr.strip() or f'종료 {completed.returncode}')
                response = json.loads(completed.stdout)
                if target is None:
                    if not isinstance(response, dict):
                        raise ValueError('조사 응답 형식 오류')
                    survey_state.update(key=key, backend='rust')
                    emit('rust_survey')
                    return response
                count = response['count']
                if type(count) is not int or count < 0 or not result.is_file():
                    raise ValueError('처리 응답 형식 오류')
                shutil.copyfile(result, target)
                emit('rust_applied', str(count))
                return count
        except (OSError, ValueError, TypeError, KeyError, RuntimeError, subprocess.SubprocessError) as exc:
            emit('python_fallback', str(exc))
            if target is None:
                survey_state.update(key=key, backend='python')
                return processor(source)
            return processor(source, target, selections)

    return run
