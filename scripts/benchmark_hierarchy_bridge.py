"""Compare 300 real DLL analyses per round with an earlier local git revision."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time
import types
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from docfit_app import hierarchy_bridge as current
from docfit_app.settings_service import SettingsService


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline-ref', default='23c1a158')
    args = parser.parse_args()
    source = subprocess.check_output(['git', 'show', f'{args.baseline_ref}:docfit_app/hierarchy_bridge.py'], cwd=ROOT)
    baseline = types.ModuleType('baseline_bridge')
    baseline.__file__ = current.__file__
    exec(compile(source, current.__file__, 'exec'), baseline.__dict__)
    paragraphs = [
        {'text': '\u25a1 title', 'indent': 0, 'font_size': 15},
        {'text': '\u3147 body', 'indent': 10, 'font_size': 13},
        {'text': '- content', 'indent': 20, 'font_size': 13},
        {'text': '* explanation', 'indent': 30, 'font_size': 13},
    ]
    assert current.find_library() is not None, 'Build the real hierarchy DLL first'
    results = {'before': [], 'after': []}
    calls = {'before': [], 'after': []}
    expected = None
    with tempfile.TemporaryDirectory(dir=ROOT) as directory, patch.dict(os.environ, APPDATA=directory):
        path = Path(directory) / 'HwpAutoDocFit' / 'settings.json'
        path.parent.mkdir()
        path.write_text(json.dumps({'hierarchy_engine': True}), encoding='utf-8')
        # Warm imports and native engine equally; then include the first cache miss.
        for module in (baseline, current):
            assert module.analyze_levels(paragraphs) is not None
        current._LIBRARIES.clear()
        current._SETTINGS_CACHE = None
        for round_index in range(5):
            order = [('before', baseline), ('after', current)]
            if round_index % 2:
                order.reverse()
            for name, module in order:
                start = time.perf_counter()
                batch = [module.analyze_levels(paragraphs) for _ in range(300)]
                elapsed = time.perf_counter() - start
                assert all(result is not None for result in batch)
                if expected is None:
                    expected = batch[0]
                
                assert all(result == expected for result in batch), 'result changed'
                results[name].append(elapsed)

        # Count separately so mock bookkeeping does not distort timings.
        current._LIBRARIES.clear()
        current._SETTINGS_CACHE = None
        for name, module in [('before', baseline), ('after', current)]:
            native_load = module.ctypes.CDLL
            read = SettingsService.read
            with patch.object(module.ctypes, 'CDLL', wraps=native_load) as loads, patch.object(
                    SettingsService, 'read', autospec=True, side_effect=read) as reads:
                for _ in range(300):
                    assert module.analyze_levels(paragraphs) == expected
            calls[name] = {'settings_reads': reads.call_count, 'dll_loads': loads.call_count}
    output = {'baseline_ref': args.baseline_ref, 'calls_per_round': 300, 'paragraphs': paragraphs,
              'seconds': results, 'median_seconds': {name: statistics.median(values) for name, values in results.items()},
              'calls': calls, 'all_3000_results_equal': True}
    print(json.dumps(output, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
