"""JOBSTATE1 의 실패·재시작 실패·검수 중 중단을 실제 한/글에서 확인한다. 합성 문서만 쓴다.

C:/HWP_AUTODOCFIT/.com-test.lock 을 배타 생성한 경우에만 실행하고, DispatchEx 새 인스턴스만 쓴다.
보안 모듈 설치·레지스트리 쓰기는 하지 않고 이미 등록된 값을 쓴다. APPDATA·LOCALAPPDATA 는 임시 폴더다.
대화상자가 뜨면 기록하고, 제한 시간을 넘기면 이 스크립트가 만든 한/글 프로세스만 끝낸다.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import runpy
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
LOCK = Path('C:/HWP_AUTODOCFIT/.com-test.lock')


def watch(hwp_process, before, found, stop, limit):
    """새로 만든 한/글의 대화상자를 모으고, 제한 시간이 지나면 그 프로세스만 끝낸다."""
    import win32gui
    import win32process
    started = time.monotonic()
    while not stop.wait(0.3):
        owned = hwp_process.owned_pids() - before

        def visit(hwnd, _):
            if win32gui.IsWindowVisible(hwnd) and win32process.GetWindowThreadProcessId(hwnd)[1] in owned:
                kind = win32gui.GetClassName(hwnd)
                if kind == '#32770' or 'dialog' in kind.lower():
                    found.add((kind, win32gui.GetWindowText(hwnd)))
            return True
        win32gui.EnumWindows(visit, None)
        if time.monotonic() - started > limit:
            print(json.dumps({'timeout': limit, 'dialogs': sorted(found),
                              'killed': hwp_process.kill_owned()}, ensure_ascii=False), flush=True)
            LOCK.unlink()   # 이 실행이 만든 잠금이다.
            os._exit(3)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--timeout', type=int, default=240, help='시나리오별 제한 시간(초)')
    args = parser.parse_args()
    with LOCK.open('x', encoding='utf-8') as lock:
        lock.write(f'JOBVERIFY1 {time.strftime("%Y-%m-%dT%H:%M:%S")} job state COM check')
    try:
        return run(args.timeout)
    finally:
        LOCK.unlink()


def run(limit):
    work = Path(tempfile.mkdtemp(prefix='jobverify-'))
    os.environ['APPDATA'] = str(work / 'appdata')
    os.environ['LOCALAPPDATA'] = str(work / 'localappdata')

    import pythoncom
    import win32com.client
    from docfit_core import hwp_process

    before = hwp_process.hwp_pids()
    ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))
    batch = ns['작업_실행']
    g = batch.__globals__
    logs, report = [], {'hwp_before': len(before), 'work': str(work)}

    def create(독립=True):
        known = hwp_process.hwp_pids()
        app = win32com.client.DispatchEx('HwpFrame.HwpObject')
        new = hwp_process.hwp_pids() - known
        if not new:   # 기존 창에 붙었으면 아무것도 하지 않는다.
            raise RuntimeError('새 한/글 프로세스가 생기지 않았습니다.')
        hwp_process.register_owned(new)
        return app

    g.update(보안모듈_초기화=lambda: None, 한글_COM_인스턴스_생성=create,
             로그=lambda message: logs.append(str(message)), 진단로그=lambda *_: None,
             상태=lambda message: logs.append(f'[상태] {message}'), 단계표시=lambda *_: None,
             진행률=lambda *_: None, 이전_작업_임시폴더_정리=lambda: 0)
    originals = {name: g[name] for name in (
        '문서_처리_1회', '저장결과_규칙검수', '한글_문서_열기', '한글_COM_인스턴스_생성')}

    def make_sources(name):
        folder = work / name
        folder.mkdir()
        pythoncom.CoInitialize()
        app = create()
        try:
            assert app.RegisterModule(g['REGISTER_MODULE_NAME'], g['REGISTER_MODULE_VALUE'])
            files = []
            for letter in 'abc':
                app.Clear(1)
                action = app.CreateAction('InsertText')
                params = action.CreateSet()
                params.SetItem('Text', f'ㅇ 합성 문서 {letter} 상태 확인용 본문입니다.')
                assert action.Execute(params) is not False
                path = folder / f'{letter}.hwpx'
                assert app.SaveAs(str(path), 'HWPX', '') is not False
                files.append(str(path))
            return files
        finally:
            app.Clear(1)
            app.Quit()
            pythoncom.CoUninitialize()

    def digest(path):
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()

    def scenario(name, restart, overrides):
        files = make_sources(name)
        hashes = [digest(path) for path in files]
        events, stop_flag = queue.Queue(), threading.Event()
        found, done = set(), threading.Event()
        del logs[:]
        g.update(originals, 한글_COM_인스턴스_생성=create)
        g.update(gui_queue=events, 중단_요청됨=stop_flag.is_set)
        g.update(overrides(files, stop_flag))
        os.environ['DOCFIT_RESTART_BETWEEN_DOCS'] = restart
        guard = threading.Thread(target=watch, args=(hwp_process, before, found, done, limit), daemon=True)
        guard.start()
        started = time.monotonic()
        try:
            batch(files, 검수=True, 자동닫기=True, 준말_등록={}, 실행모드='format',
                  세부작업_선택=ONLY_NONE)
        finally:
            done.set()
            guard.join(5)
        items = []
        while not events.empty():
            items.append(events.get_nowait())
        records = [dict(item) for item in g['최종검수_문서목록']]
        names = lambda path: Path(path).name if path else None
        report[name] = {
            'seconds': round(time.monotonic() - started, 1),
            'dialogs': sorted(found),
            'events': [[e[0]] + ([names(e[1])] if e[0] in ('saved', 'document_state', 'document_error') else [])
                       + ([e[2]] if e[0] == 'document_state' else [])
                       + (list(e[1:3]) if e[0] == 'finished' else [])
                       for e in items if e[0] in (
                           'saved', 'document_state', 'document_error', 'finished', 'stopped', 'fatal_error')],
            'records': [{'source': names(r.get('source')), 'output': names(r.get('output')),
                         'status': r.get('status'), 'success': r.get('success'),
                         'output_exists': bool(r.get('output')) and Path(r['output']).is_file(),
                         'error': r.get('error')} for r in records],
            'sources_unchanged': [digest(path) for path in files] == hashes,
            'result_files': sorted(p.name for p in (work / name).iterdir() if p.name not in {'a.hwpx', 'b.hwpx', 'c.hwpx'}),
            'log_tail': [line for line in logs if any(word in line for word in
                         ('실패', '중단', '미처리', '재시작', '오류', '[상태] 작업', '[상태] 모든'))][-12:],
        }
        time.sleep(1.5)
        report[name]['hwp_left'] = len(hwp_process.hwp_pids() - before)

    from docfit_core.stage_selection import stages_for_mode
    ONLY_NONE = {key: False for key, _ in stages_for_mode('format')}

    # (a) 첫 문서를 고친(저장 안 된 변경) 상태에서 실패시키고, 한/글 재시작 없이 다음 문서를 처리한다.
    def fail_first(files, stop_flag):
        seen = []
        original = originals['문서_처리_1회']

        def once(*a, **k):
            seen.append(True)
            if len(seen) == 1:
                g['텍스트_삽입']('저장하면 안 되는 변경')
                report['a_modified_before_close'] = bool(g['hwp'].IsModified)
                return False
            report['a_documents_open_at_next'] = int(g['hwp'].XHwpDocuments.Count)
            report['a_next_text_clean'] = '저장하면 안 되는 변경' not in g['hwp'].GetTextFile('TEXT', '')
            return original(*a, **k)
        return {'문서_처리_1회': once}

    # (b) 첫 문서 뒤 재시작에서 새 인스턴스 생성을 실패시킨다.
    def fail_restart(files, stop_flag):
        calls = []

        def broken(독립=True):
            calls.append(True)
            if len(calls) > 1:
                raise RuntimeError('주입한 재시작 실패')
            return create(독립)
        return {'한글_COM_인스턴스_생성': broken}

    # (c) 저장 뒤 검수가 저장 결과를 연 직후 중단을 요청한다.
    def cancel_in_inspection(files, stop_flag):
        original_open = originals['한글_문서_열기']
        original_audit = originals['저장결과_규칙검수']
        inside = []

        def opened(app, path, *a):
            result = original_open(app, path, *a)
            if inside:
                report['c_reopened_result'] = Path(path).name
                stop_flag.set()
            return result

        def audit(*a, **k):
            inside.append(True)
            try:
                return original_audit(*a, **k)
            finally:
                inside.pop()
        return {'한글_문서_열기': opened, '저장결과_규칙검수': audit}

    # (d) 저장 결과 재열기만 실패시켜 검수 항목 error 가 상태에 반영되는지 본다.
    def fail_reopen(files, stop_flag):
        original_open = originals['한글_문서_열기']
        original_audit = originals['저장결과_규칙검수']
        inside = []

        def opened(app, path, *a):
            return False if inside else original_open(app, path, *a)

        def audit(*a, **k):
            inside.append(True)
            try:
                return original_audit(*a, **k)
            finally:
                inside.pop()
        return {'한글_문서_열기': opened, '저장결과_규칙검수': audit}

    # (c)(d)는 저장 결과를 다시 여는 검사가 하나 이상 있어야 한다.
    scenario('a_fail_then_next', '0', fail_first)
    scenario('b_restart_failure', '1', fail_restart)
    checked = dict(ONLY_NONE, page_group=True)
    ONLY_NONE = checked
    scenario('c_cancel_in_inspection', '0', cancel_in_inspection)
    scenario('d_reopen_failure', '0', fail_reopen)

    print(json.dumps(report, ensure_ascii=False, indent=1))
    return 0


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    sys.exit(main())
