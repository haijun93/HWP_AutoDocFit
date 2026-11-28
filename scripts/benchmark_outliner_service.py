"""OUTENGINE1: synthetic-only execute timings, launches, and result fingerprints.

Run from the repository root with --executable pointing at a local release build.
No installed app, settings, or user documents are opened.
"""
from __future__ import annotations

import argparse
from datetime import date
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from docfit_app.outliner_service import OutlinerService


def markdown_for(count):
    return ''.join(
        f'- 합성 항목 {i:04d} [[Home]] #sample ((synthetic-id))\n'
        + ('  id:: synthetic-id\n' if i == 0 else '')
        for i in range(count)
    )


def fingerprint(value, root):
    text = json.dumps(value, ensure_ascii=False, sort_keys=True)
    text = text.replace(json.dumps(str(root))[1:-1], '<DATA>')
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def measure(executable, parent, repetitions, mode):
    rows = []
    for count in (10, 200, 2000):
        markdown = markdown_for(count)
        payloads = {
            'list': {'action': 'list'},
            'create': {'action': 'create', 'page': 'New'},
            'load': {'action': 'load', 'page': 'Home'},
            'save': {'action': 'save', 'page': 'Home', 'markdown': markdown},
            'rename': {'action': 'rename', 'page': 'Home', 'new_name': 'Renamed'},
            'undo_rename': {'action': 'undo_rename'},
            'check_delete': {'action': 'check_delete', 'page': 'Home'},
            'page_references': {'action': 'page_references', 'page': 'Home'},
            'delete': {'action': 'delete', 'page': 'Home', 'check_references': True, 'force': True},
            'restore': {'action': 'restore'},
            'daily': {'action': 'daily'},
            'edit': {'action': 'edit', 'operation': 'set_text', 'block_id': 1, 'text': '수정'},
            'key': {'action': 'key', 'operation': 'enter', 'block': 0, 'start': 0, 'end': 0},
            'history': {'action': 'history', 'operation': 'status'},
            'search': {'action': 'search', 'query': '합성'},
            'backlinks': {'action': 'backlinks', 'target': 'Home'},
            'parse': {'action': 'parse'},
            'resolve_block_ref': {'action': 'resolve_block_ref', 'uuid': 'synthetic-id'},
            'block_ref': {'action': 'block_ref', 'uuid': 'synthetic-id'},
            'insert_block_ref': {'action': 'insert_block_ref', 'block_id': 1, 'target': 'synthetic-id'},
            'block_backlinks': {'action': 'block_backlinks', 'target': 'synthetic-id'},
            'edit_save_backlinks': {'action': 'edit_save_backlinks'},
        }
        for name, template in payloads.items():
            samples, calls, digests, file_digests, cold_samples, cold_calls = [], [], [], [], [], []
            for _ in range(repetitions):
                with tempfile.TemporaryDirectory(prefix='outengine1-', dir=parent) as folder:
                    launches = 0

                    def run(*args, **kwargs):
                        nonlocal launches
                        launches += 1
                        return subprocess.run(*args, **kwargs)

                    def popen(*args, **kwargs):
                        nonlocal launches
                        launches += 1
                        return subprocess.Popen(*args, **kwargs)

                    service = OutlinerService(folder, executable=executable, runner=run, popen=popen,
                                              persistent=mode == 'resident', runtime_parent=parent)
                    for page in ('Home', 'Other', date.today().isoformat().replace('-', '_')):
                        (service.root / f'{page}.md').write_text(markdown, encoding='utf-8', newline='')
                    payload = {'page': 'Home', 'markdown': markdown, **template}
                    payload['version'] = service._digest(service.root / 'Home.md')
                    started = time.perf_counter()
                    service.execute({'action': 'parse', 'page': 'Home', 'markdown': markdown})
                    cold_samples.append((time.perf_counter() - started) * 1000)
                    cold_calls.append(launches)
                    if name == 'undo_rename':
                        service.execute(payloads['rename'])
                    elif name == 'restore':
                        service.execute(payloads['delete'])
                    launches = 0
                    started = time.perf_counter()
                    try:
                        if name == 'edit_save_backlinks':
                            edited = service.execute({**payload, **payloads['edit']})
                            saved = service.execute({**payload, **payloads['save'], 'markdown': edited['markdown']})
                            linked = service.execute({**payload, **payloads['backlinks'], 'markdown': edited['markdown']})
                            result = {'edit': edited, 'save': saved, 'backlinks': linked}
                        else:
                            result = service.execute(payload)
                        samples.append((time.perf_counter() - started) * 1000)
                        calls.append(launches)
                        digests.append(fingerprint(result, service.root))
                        files = {str(p.relative_to(service.root)).replace(p.name, 'Home.md')
                                 if p.parent.name == '.trash' else str(p.relative_to(service.root)):
                                 hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in sorted(service.root.rglob('*.md'))}
                        file_digests.append(fingerprint(files, service.root))
                    finally:
                        service.close()
            row = {'blocks': count, 'action': name, 'milliseconds': samples,
                   'median_ms': statistics.median(samples), 'max_ms': max(samples),
                   'launches': calls, 'result_sha256': digests, 'files_sha256': file_digests,
                   'cold_ms': cold_samples, 'cold_launches': cold_calls}
            rows.append(row)
            print(f'{count:4} {name:18} median={row["median_ms"]:8.3f} '
                  f'max={row["max_ms"]:8.3f} launches={calls}', flush=True)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--executable', required=True, type=Path)
    parser.add_argument('--temp-parent', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--repetitions', type=int, default=5)
    parser.add_argument('--mode', choices=('oneshot', 'resident'), default='oneshot')
    args = parser.parse_args()
    if args.repetitions < 5:
        parser.error('at least five repetitions are required')
    args.temp_parent.mkdir(parents=True, exist_ok=True)
    result = {'mode': args.mode, 'repetitions': args.repetitions, 'engine_sha256': hashlib.sha256(
        args.executable.read_bytes()).hexdigest(), 'rows': measure(
            args.executable.resolve(), args.temp_parent, args.repetitions, args.mode)}
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
