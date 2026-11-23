"""Run a single sequential batch through the production processing entry point."""
from __future__ import annotations

import argparse
import inspect
import json
import runpy
import sys
from pathlib import Path
from time import perf_counter


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--close-after-each", action="store_true",
                        help="experimental: close the saved HWP document before processing the next input")
    parser.add_argument("--restart-after-each", action="store_true",
                        help="experimental: recreate the HWP COM process between input documents")
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(repo))
    from docfit_core.app_state import RunOptions
    from docfit_engine.pipeline import PipelineCallbacks
    from docfit_engine.runtime import session_for

    namespace = runpy.run_path(str(repo / "hwp-auto-docfit.py"), run_name="engine_seqdiff1")
    func = namespace["작업_실행"]
    globals_ = func.__globals__
    restart_events: list[dict] = []
    active_state = {}
    if args.close_after_each or args.restart_after_each:
        original_document_processor = globals_["문서_처리"]

        def process_then_close(*call_args, **call_kwargs):
            try:
                return original_document_processor(*call_args, **call_kwargs)
            finally:
                app = active_state.get("hwp")
                if app is not None:
                    if args.restart_after_each:
                        from docfit_core.hwp_process import hwp_pids
                        before = sorted(hwp_pids())
                        app.Quit()
                        active_state["hwp"] = active_state["한글_COM_인스턴스_생성"](독립=True)
                        restart_events.append({"before": before, "after": sorted(hwp_pids())})
                    else:
                        app.Run("FileClose")

        globals_["문서_처리"] = process_then_close
    events: list = []
    messages: list = []
    callbacks = PipelineCallbacks(event=events.append, log=messages.append)
    options = RunOptions(mode="all", verify_result=False, autoclose=True)
    context = session_for(globals_).new_context(options=options, callbacks=callbacks)
    active_state = context.state
    values = {name: parameter.default for name, parameter in inspect.signature(func).parameters.items()
              if name not in ("context", "callbacks") and parameter.default is not inspect._empty}
    values.update(파일목록=[str(path.resolve()) for path in args.files], 표준서식=True, 준말_등록={},
                  실행모드=options.effective_mode, 표자간조정=options.effective_table_spacing,
                  자동닫기=True, 검수=False, 쪽범위=None,
                  세부작업_선택=options.resolve_stages({}))
    names = [name for name in inspect.signature(func).parameters
             if name not in ("context", "callbacks")]
    started = perf_counter()
    func(*[values[name] for name in names], context=context, callbacks=callbacks)
    report = {"seconds": round(perf_counter() - started, 3),
              "saved": [event[2] for event in events if event[0] == "saved"],
              "errors": [list(map(str, event)) for event in events
                         if event[0] in ("fatal_error", "document_error")],
              "stopped": any(event[0] == "stopped" for event in events),
              "com_restart_events": restart_events}
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    args.report.with_suffix(".log").write_text("\n".join(map(str, messages)), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
