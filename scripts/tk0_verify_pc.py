"""Real hidden Tcl/Tk bridge benchmark; no HWP COM or document operations.

The fallback host supplies real Tk variables and production services, not Mock
objects. It deliberately does not construct the legacy document-processing UI.
Run this script in a fresh process for each --repo (imports are target-local).
"""
from __future__ import annotations

import argparse
import faulthandler
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import threading
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--repeat', type=int, default=50)
    args = parser.parse_args()
    repo = args.repo.resolve()
    output = args.output.resolve() / args.label
    output.mkdir(parents=True, exist_ok=False)
    sys.dont_write_bytecode = True  # baseline remains read-only
    sys.path.insert(0, str(repo))
    os.environ.update(DOCFIT_OFFLINE='1', APPDATA=str(output / 'Roaming'),
                      LOCALAPPDATA=str(output / 'Local'), TEMP=str(output), TMP=str(output))
    for name in ('APPDATA', 'LOCALAPPDATA'):
        Path(os.environ[name]).mkdir()
    os.chdir(repo)
    faulthandler.dump_traceback_later(60, repeat=True)
    import tkinter as tk
    import desktop_web_ui as ui
    from docfit_core.app_state import AppState
    from docfit_app.abbreviation_service import AbbreviationService
    from docfit_app.settings_service import SettingsService
    from docfit_app.freeze_tracer import FreezeTracer
    from docfit_ui.scheduler import install_scheduler

    class HiddenHost:
        def __init__(self, root):
            self.root = root
            self.state = AppState()
            self.files = self.state.files
            self.running = False
            self.stage_choices = self.state.stage_choices
            self._세부작업_기본저장 = self.state.default_saved
            self._job_id = None
            self.stage_board = tk.Frame(root)
            for name, value in dict(selected_mode='spacing', status_var='대기',
                                    range_mode_var='all', range_start_var='1', range_end_var='1').items():
                setattr(self, name, tk.StringVar(root, value=value))
            self.store = SettingsService(output / 'settings.json')
            # A private profile enables read-only external_app status lookup.
            self.store.update(lambda values: values.update(developer_mode=True))
            self.abbreviations = AbbreviationService(self.store)
            install_scheduler(self)

        def _준말_서비스(self):
            return self.abbreviations

    root = tk.Tk()
    root.withdraw()
    host = HiddenHost(root)
    bridge = ui.DesktopWebBridge(host)
    bridge.tk_stopped = threading.Event()
    observed = []
    class ObservedTracer(FreezeTracer):
        def begin_tk(self, name):
            observed.append(time.perf_counter())
            return super().begin_tk(name)

    tracer = ObservedTracer(output / 'freeze-logs', threshold=30)
    bridge._freeze_tracer = tracer
    tracer.start()
    bridge.start_tk_heartbeat()
    result = dict(repo=str(repo), pid=os.getpid(), repeat=args.repeat,
                  commit=subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
                  mode='real-hidden-tk', full_app_started=False, screen_verified=False, screenshot=None,
                  resource_path=str(ui._resource_path()),
                  flutter_assets=(repo / 'desktop_web/flutter/main.dart.js').is_file(),
                  tk_version=root.tk.call('info', 'patchlevel'), rows=[], errors=[])
    # Count the production tracing entry point, available on both revisions.
    # _tk and callbacks remain untouched; no all-Python profiler skews XML work.

    calls = [
        ('get_settings', bridge.get_settings),
        ('get_pending_dialog', bridge.get_pending_dialog),
        ('document_tool(clean)', lambda: bridge.document_tool('clean', {'text': '확인'})),
        ('file_rename(list)', lambda: bridge.file_rename('list')),
        ('timemachine(settings_get)', lambda: bridge.timemachine('settings_get')),
        ('kclip_window(get)', lambda: bridge.kclip_window('get')),
        ('external_app(status)', lambda: bridge.external_app('status', {'app': 'photo_album'})),
        ('table_sketch(take_drop)', lambda: bridge.table_sketch('take_drop')),
        ('table_units(arrows)', lambda: bridge.table_units('arrows')),
        ('cancel_zero_click', bridge.cancel_zero_click),
        ('start_zero_click', lambda: bridge.start_zero_click('tk0-invalid-token')),
        ('add_folder(no_selection)', bridge.add_folder),
        ('_initial_window_settings', lambda: ui._initial_window_settings(bridge)),
        ('get_state', bridge.get_state),
    ]

    def measure():
        try:
            for name, call in calls:
                durations, samples, errors = [], [], []
                before = len(observed)
                for _ in range(args.repeat):
                    started = time.perf_counter()
                    try:
                        value = call()
                        samples.append(value)
                    except Exception as exc:
                        errors.append(f'{type(exc).__name__}: {exc}')
                    durations.append((time.perf_counter() - started) * 1000)
                result['rows'].append(dict(name=name, tk_roundtrips=len(observed)-before,
                    median_ms=statistics.median(durations), max_ms=max(durations),
                    durations_ms=durations, exceptions=errors,
                    last_response=samples[-1] if samples else None))
                print(name, 'completed', len(observed)-before, 'Tk calls', flush=True)
            # Safe synthetic no-op: block only this private Tk owner, then queue
            # the same timeout=None path used by dialog commands. No dialog opens.
            busy_started = threading.Event()
            ran = threading.Event()
            def block_owner():
                busy_started.set()
                time.sleep(6)
            host._bridge_callbacks.put(block_owner)
            if not busy_started.wait(5):
                raise RuntimeError('private Tk blocker did not start')
            started = time.perf_counter()
            try:
                bridge._tk(ran.set, timeout=None)
                busy = dict(outcome='returned')
            except Exception as exc:
                busy = dict(outcome=type(exc).__name__, message=str(exc))
            busy['elapsed_ms'] = (time.perf_counter()-started)*1000
            bridge._tk(lambda: None)  # drain cancelled request after blocker ends
            busy['callback_executed'] = ran.is_set()
            result['busy_probe'] = busy
        except BaseException as exc:
            result['errors'].append(f'{type(exc).__name__}: {exc}')
        finally:
            bridge._shutdown()
            # _shutdown cancels the heartbeat and flushes native tracing.
            host._bridge_callbacks.put(root.quit)

    worker = threading.Thread(target=measure, name='tk0-measure')
    worker.start()
    try:
        root.mainloop()
    finally:
        worker.join(timeout=10)
        result['worker_alive'] = worker.is_alive()
        root.destroy()
    result['observed_roundtrips'] = len(observed)
    result['native_trace_available'] = hasattr(tracer, 'record_tk')
    result['native_trace_files'] = [str(path) for path in (output / 'freeze-logs').glob('tk-roundtrips-*.json')]
    # Baseline explicitly labelled observer data, never presented as native trace.
    (output / 'observer-roundtrips.json').write_text(json.dumps(result['rows'], ensure_ascii=False, indent=2), encoding='utf-8')
    (output / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({key: result[key] for key in ('repo', 'pid', 'errors', 'busy_probe', 'observed_roundtrips')}, ensure_ascii=False))
    faulthandler.cancel_dump_traceback_later()
    return int(bool(result['errors']) or result['worker_alive'] or any(row['exceptions'] for row in result['rows']))


if __name__ == '__main__':
    raise SystemExit(main())
