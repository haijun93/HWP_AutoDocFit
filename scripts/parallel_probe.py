"""1회 ALLQA 소규모 순차/2프로세스 병렬 측정 (Windows 한/글 COM 전용)."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from scripts.allqa_run import read_docset, sha256


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--docset', type=Path, default=REPO / 'docs/ALLQA_DOCSET.md')
    parser.add_argument('--ids', nargs='+', default=['txt_short', 'hwp_reference', 'hwpx_table', 'hwpx_continuation'])
    parser.add_argument('--out', type=Path, required=True, help='결과를 저장할 저장소 외부 경로')
    parser.add_argument('--docs-dir', type=Path, required=True, help='처리용 사본을 둘 외부 경로')
    parser.add_argument('--timeout', type=int, default=900)
    args = parser.parse_args()
    if sys.platform != 'win32':
        parser.error('Windows + 한/글 COM 환경이 필요합니다.')
    import psutil
    from docfit_core.save_guard import com_test_lock

    rows = read_docset(args.docset.resolve())
    chosen = [r for r in rows if r['id'] in args.ids]
    if len(chosen) != len(args.ids) or not 2 <= len(chosen) <= 4:
        parser.error('--ids는 ALLQA 세트의 서로 다른 문서 2~4개여야 합니다.')
    root = args.out.resolve(); docs = args.docs_dir.resolve()
    if root.exists():
        parser.error(f'결과 폴더가 이미 있습니다: {root}')
    root.mkdir(parents=True); docs.mkdir(parents=True, exist_ok=True)
    prepared = []
    for row in chosen:
        source = (REPO / row['source']).resolve(strict=True)
        if sha256(source) != row['sha256']:
            raise ValueError(f"픽스처 해시 불일치: {row['id']}")
        copied = docs / f"{row['id']}{source.suffix.lower()}"
        shutil.copy2(source, copied)
        if sha256(copied) != row['sha256']:
            raise ValueError(f"사본 해시 불일치: {row['id']}")
        prepared.append((row, copied, source))

    env = dict(os.environ, PYTHONUTF8='1', DOCFIT_COM_PERF='1')
    runs = {}
    with com_test_lock('parallel_probe', f"순차/2프로세스 비교: {len(chosen)}개 문서"):
        for label, parallel in [('sequential', False), ('parallel2', True)]:
            dest = root / label; dest.mkdir(); perf = dest / 'comperf'; perf.mkdir()
            jobs = []
            started_all = time.perf_counter()
            lock = threading.Lock(); samples = {'mem': 0, 'cpu': 0.0, 'peak_hwp_count': 0, 'hwp_pids': set()}
            stop_monitor = threading.Event()
            def monitor():
                while not stop_monitor.wait(.25):
                    procs = []
                    for p in psutil.process_iter(['pid', 'name']):
                        if (p.info.get('name') or '').lower() in ('hwp.exe', 'hwpframe.exe'):
                            procs.append(p)
                    mem = cpu = 0
                    for p in procs:
                        try:
                            mem += p.memory_info().rss
                            cpu += p.cpu_percent(None)
                            samples['hwp_pids'].add(p.pid)
                        except (psutil.NoSuchProcess, psutil.AccessDenied):
                            pass
                    with lock:
                        samples['mem'] = max(samples['mem'], mem)
                        samples['cpu'] = max(samples['cpu'], cpu)
                        samples['peak_hwp_count'] = max(samples['peak_hwp_count'], len(procs))
            sampler = threading.Thread(target=monitor, daemon=True); sampler.start()

            def execute(index, row, copied):
                run = dest / row['id']; run.mkdir()
                perf_dir = perf / f"run{index:02d}"; perf_dir.mkdir()
                child_env = dict(env, DOCFIT_COM_PERF_DIR=str(perf_dir))
                cmd = [sys.executable, str(REPO / 'scripts/smoke_engine_pc.py'), str(copied),
                       '--mode', 'all', '--out', str(run / 'engine')]
                start = time.perf_counter()
                try:
                    cp = subprocess.run(cmd, cwd=REPO, env=child_env, capture_output=True,
                                        text=True, encoding='utf-8', errors='replace', timeout=args.timeout)
                    (run / 'stdout.log').write_text(cp.stdout + cp.stderr, encoding='utf-8')
                    report_path = run / 'engine/report.json'
                    report = json.loads(report_path.read_text(encoding='utf-8')) if report_path.exists() else {}
                    outputs = []
                    for candidate in (run / 'engine').glob('*.hwpx'):
                        if candidate.name != 'source.hwpx':
                            output = run / f"result_{row['id']}.hwpx"; shutil.copy2(candidate, output); outputs.append(str(output))
                    source_path = next(source for item, _, source in prepared if item['id'] == row['id'])
                    input_copy = run / 'engine' / ('source' + copied.suffix.lower())
                    report.update(id=row['id'], kind=row['kind'], source=row['source'], returncode=cp.returncode,
                                  seconds=round(time.perf_counter()-start, 3), errors=report.get('errors', []), result_files=outputs,
                                  source_sha256_before=sha256(source_path), source_sha256_after=sha256(source_path),
                                  copy_sha256=sha256(copied), copy_sha256_after=sha256(copied),
                                  page_count=(report.get('page_counts') or [None])[0],
                                  enabled_stages_check={'pass': None})
                    (run / 'result.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
                    return report
                except subprocess.TimeoutExpired as exc:
                    return {'id': row['id'], 'returncode': -1, 'timeout': True,
                            'seconds': round(time.perf_counter()-start, 3), 'errors': [str(exc)]}

            if parallel:
                from concurrent.futures import ThreadPoolExecutor, as_completed
                with ThreadPoolExecutor(max_workers=2) as pool:
                    futures = [pool.submit(execute, i, row, copied) for i, (row, copied, _) in enumerate(prepared, 1)]
                    jobs = [f.result() for f in as_completed(futures)]
            else:
                jobs = [execute(i, row, copied) for i, (row, copied, _) in enumerate(prepared, 1)]
            stop_monitor.set(); sampler.join()
            for report, (_, _, source) in zip(jobs, prepared):
                report['source_sha256_after'] = sha256(source)
            # Preserve corpus order regardless of process completion order.
            by_id = {j['id']: j for j in jobs}
            data = {'label': label, 'mode': 'all', 'docset': str(args.docset),
                    'results': [by_id[row['id']] for row, _, _ in prepared]}
            (dest / 'allqa.json').write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding='utf-8')
            runs[label] = {'wall_seconds': round(time.perf_counter()-started_all, 3),
                           'documents': data['results'], 'peak_hwp_rss_mb': round(samples['mem']/1048576, 1),
                           'peak_hwp_cpu_percent': round(samples['cpu'], 1),
                           'peak_hwp_process_count': samples['peak_hwp_count'],
                           'observed_hwp_pids': sorted(samples['hwp_pids'])}
    summary = {'selected_ids': args.ids, 'runs': runs, 'one_iteration': True,
               'note': '문서별 시간은 smoke_engine_pc 파이프라인 실행 구간; 전체 시간은 준비 완료 후 각 레인 시작부터 종료까지.'}
    (root / 'probe.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    compare = subprocess.run([sys.executable, str(REPO / 'scripts/allqa_compare.py'),
                              str(root / 'sequential'), str(root / 'parallel2'), '--output', str(root / 'compare.md')],
                             cwd=REPO, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace')
    (root / 'compare.log').write_text(compare.stdout + compare.stderr, encoding='utf-8')
    summary['compare_returncode'] = compare.returncode
    (root / 'probe.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    return 0 if compare.returncode == 0 else compare.returncode


if __name__ == '__main__':
    raise SystemExit(main())
