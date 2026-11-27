"""INDENTW4: 합성 HWP 왕복·Shift+Tab 대조와 HWP/HWPX 파이프라인 전후 측정.

validate를 먼저 실행한다. benchmark는 판정 후 별도로 실행한다.
출력은 작업 폴더 밖에 지정한다. 강제 종료·설치·사용자 설정 변경은 하지 않는다.
"""
from __future__ import annotations

import argparse
import contextlib
import copy
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from defusedxml import ElementTree as ET
from docfit_core import font_widths as fw, indent_table as it
from docfit_core.fidelity.package import PackageSnapshot
from docfit_core.save_guard import DEFAULT_COM_LOCK_PATH, com_test_lock
from scripts import measure_indent_hangul as tool


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


@contextlib.contextmanager
def exclusive(owner="INDENTW4"):
    full = DEFAULT_COM_LOCK_PATH.with_name(".full-test.lock")
    # 다른 실행에서 물려받은 재진입 표식으로 배타 생성을 생략하지 않는다.
    os.environ.pop("DOCFIT_COM_LOCK_PATH", None)
    while True:
        while full.exists():
            print("waiting: full-test lock", flush=True)
            time.sleep(15)
        with com_test_lock(owner, "synthetic validation", timeout_sec=7200):
            if full.exists():
                continue
            yield
            return


class SyntheticHangul(tool.Hangul):
    def open(self, path):
        kind = "HWP" if path.suffix.lower() == ".hwp" else "HWPX"
        if not self.app.Open(str(path), kind, "forceopen:true"):
            raise RuntimeError("합성 문서 열기 실패: " + path.name)
        self.opened.add(path.name.casefold())

    def save(self, path):
        kind = "HWP" if path.suffix.lower() == ".hwp" else "HWPX"
        if not self.app.SaveAs(str(path), kind, ""):
            raise RuntimeError("합성 문서 저장 실패: " + path.name)
        self.opened.add(path.name.casefold())

    def quit(self):
        # 부모 도구의 15초 뒤 taskkill 정책을 사용하지 않는다.
        if self.app is None:
            return
        docs = self.app.XHwpDocuments
        for index in range(docs.Count):
            name = Path(docs.Item(index).FullName or "").name.casefold()
            if name and name not in self.opened:
                raise RuntimeError("소유하지 않은 문서 발견: 닫지 않음")
        self.app.Clear(1)
        self.app.Quit()
        self.app = None


def canonical(node, *, omit=()):
    return (it._tag(node), tuple(sorted((k, v) for k, v in node.attrib.items() if k not in omit)),
            node.text or "", tuple(canonical(c, omit=omit) for c in node))


def inventory(path):
    """참조 ID 대신 실제 글꼴·글자 모양을 해석한다. run 분할 차이는 글자별로 대조한다."""
    package = PackageSnapshot.load(path)
    header = ET.fromstring(package.read("Contents/header.xml"))
    fonts = fw.fontfaces_from_header(header)
    chars = {}
    for node in it._find_all(header, "charPr"):
        node = copy.deepcopy(node)
        ref = node.get("id")
        node.attrib.pop("id", None)
        for item in it._find_all(node, "fontRef"):
            for lang, value in list(item.attrib.items()):
                slot = fonts.get(lang, {}).get(value)
                if slot is None:
                    slot = fonts.get(lang.upper(), {}).get(value)
                if slot is None:
                    raise ValueError("해석할 수 없는 글꼴 참조: " + lang + "/" + value)
                item.set(lang, str(slot))
        chars[ref] = canonical(node)
    paras = {p.get("id"): p for p in it._find_all(header, "paraPr")}
    result = []
    for name in sorted(n for n in package.entries if it._SECTION_NAME.fullmatch(n)):
        section = ET.fromstring(package.read(name))
        for paragraph in section:
            if it._tag(paragraph) != "p":
                continue
            text, styles = "", []
            for run in paragraph:
                if it._tag(run) != "run":
                    continue
                for node in run:
                    if it._tag(node) == "t":
                        part = "".join(node.itertext())
                        text += part
                        styles.extend([chars[run.get("charPrIDRef")]] * len(part))
            shape = paras[paragraph.get("paraPrIDRef")]
            margins = []
            for branch in ("case", "default"):
                for item in it._find_all(shape, branch):
                    margin = it._find_all(item, "margin")
                    if margin:
                        margins.append({it._tag(x): (int(x.get("value", 0)), x.get("unit"))
                                        for x in margin[0]})
            other = copy.deepcopy(shape)
            other.attrib.pop("id", None)
            for margin in it._find_all(other, "margin"):
                for item in list(margin):
                    if it._tag(item) in ("left", "intent"):
                        margin.remove(item)
            result.append({"text": text, "styles": styles, "margins": margins,
                           "paragraph_other": canonical(other)})
    return result


def compare(a, b):
    if len(a) != len(b):
        return {"paragraph_counts": [len(a), len(b)], "same_count": False}
    fields = ("text", "styles", "paragraph_other")
    differences = {key: [i for i, (x, y) in enumerate(zip(a, b)) if x[key] != y[key]] for key in fields}
    errors = []
    for x, y in zip(a, b):
        if x["margins"] and y["margins"]:
            errors.append(max(abs(x["margins"][0][k][0] - y["margins"][0][k][0])
                              for k in ("left", "intent")) / 100)
    return {"paragraph_counts": [len(a), len(b)], "same_count": True, **differences,
            "max_margin_error_pt": max(errors, default=0),
            "over_half_pt": sum(error > .5 for error in errors)}


def cases():
    faces = [(x["face"], x["type"]) for x in json.loads(fw.DATA_PATH.read_text(encoding="utf-8"))["fonts"]]
    body = "관련 부서는 사업 추진 현황과 향후 계획을 검토하여 기한 내 의견을 제출하여 주시기 바랍니다. "
    result = []
    for n, (face, kind) in enumerate(faces):
        for align in tool.ALIGN_NAMES:
            for j, prefix in enumerate(("□ ", " ㅇ ", "   - ", "※ 추진 기간 : ")):
                style = tool.Style(face, (12, 13, 15, 17)[j], kind,
                                   ratio=(100, 95, 90, 105)[j], spacing=(0, -5, 3, -3)[j], bold=j == 3)
                other = tool.Style("함초롬바탕", 14, ratio=97, spacing=-3)
                text = f"{n + 1}차 {j + 1}항 " + body * (1 if j == 0 else 3)
                runs = ((prefix, style), (text, other)) if j % 2 else ((prefix + text, style),)
                result.append(tool.Case(runs, len(prefix), align, left=(0, 500, 1500, 3333)[j],
                                        right=(0, 2001, 6000, 0)[j], tag=f"{face}/{kind}/{align}/{j}"))
    # 한 문단 안에서 TTF/HFT를 교대로 쓴다.
    for align in tool.ALIGN_NAMES:
        for prefix in ("ㅇ ", "   - ", "※ 검토 결과 : ", "* "):
            result.append(tool.Case(((prefix, tool.Style("신명 중명조", 15, "HFT", bold=True)),
                                     (body * 3, tool.Style("휴먼명조", 15))), len(prefix), align,
                                    tag="TTF/HFT mixed/" + align))
    return result


def engine():
    ns = runpy.run_path(str(ROOT / "hwp-auto-docfit.py"), run_name="indentw4_probe")
    return ns["작업_실행"].__globals__


def coverage(path):
    document = it.plan_document(path, lambda text: 2 if len(text) > 2 else None)
    fonts = fw.fontfaces_from_header(document.header)
    chars = {c.get("id"): c for c in it._find_all(document.header, "charPr")}
    paras = {p.get("id"): p for p in it._find_all(document.header, "paraPr")}
    used, aligns = set(), {}
    for section in document.sections.values():
        for paragraph in section:
            if it._tag(paragraph) != "p":
                continue
            align = it._find_all(paras[paragraph.get("paraPrIDRef")], "align")[0].get("horizontal")
            aligns[align] = aligns.get(align, 0) + 1
            for run in paragraph:
                if it._tag(run) != "run" or not it._find_all(run, "t"):
                    continue
                style = fw.style_from_char_pr(chars[run.get("charPrIDRef")], fonts)
                used.update((slot.face, slot.font_type) for slot in style.fonts.values())
    return {"fonts": sorted(used), "aligns": aligns}


def bind_probe(ns, hwp):
    ns.update(hwp=hwp, hwp_run=hwp.HAction.Run, 현재_한칸표인가=lambda: False,
              내어쓰기_변경문단=set(), 로그=lambda *_: None, 진단로그=lambda *_: None)


def validate(out):
    out.mkdir(parents=True, exist_ok=False)
    os.environ["APPDATA"] = str(out / "appdata")
    items = cases()
    tool.build_hwpx(items, out / "synthetic.hwpx")
    ns = engine()
    offset = ns["폭표_내어쓰기_오프셋"]
    rows = []
    with exclusive():
        hangul = SyntheticHangul()
        try:
            hwp = hangul.app
            bind_probe(ns, hwp)
            version = hwp.Version
            assert it.version_matches(version), "실측 환경 지문 불일치"
            hangul.open(out / "synthetic.hwpx")
            hangul.save(out / "source.hwp")
            hangul.close_document()
            source_hash = hashlib.sha256((out / "source.hwp").read_bytes()).hexdigest()
            hangul.open(out / "source.hwp")
            hangul.save(out / "working.hwpx")
            before_pages = hwp.PageCount
            document = it.plan_document(out / "working.hwpx", offset)
            before = inventory(out / "working.hwpx")
            print(f"validation: {len(items)} cases, {len(document.plans)} plans", flush=True)
            for index, plan in document.plans.items():
                hwp.SetPos(0, index, 0)
                ok = ns["문단_내어쓰기_적용"]((0, index, 0), plan.text)
                actual = [int(hwp.ParaShape.Item(k)) for k in ("LeftMargin", "Indentation")]
                error = max(abs(actual[0] / 2 - plan.left), abs(actual[1] / 2 - plan.intent)) / 100
                rows.append({"index": index, "tag": items[index - 1].tag, "left": plan.left,
                             "intent": plan.intent, "com": actual, "error_pt": error, "ok": ok})
                if len(rows) % 40 == 0:
                    print(f"validation: {len(rows)}/{len(document.plans)}", flush=True)
            hangul.save(out / "shift.hwpx")
            shift_pages = hwp.PageCount
            hangul.close_document()
            shutil.copy2(out / "working.hwpx", out / "table.hwpx")
            changed = it.apply_in_place(out / "table.hwpx", offset)
            hangul.open(out / "table.hwpx")
            hangul.save(out / "table.hwp")
            hangul.close_document()
            hangul.open(out / "table.hwp")
            hangul.save(out / "roundtrip.hwpx")
            table_pages = hwp.PageCount
            units = []
            after = inventory(out / "roundtrip.hwpx")
            for index, plan in document.plans.items():
                hwp.SetPos(0, index, 0)
                actual = [int(hwp.ParaShape.Item(k)) for k in ("LeftMargin", "Indentation")]
                margin = after[index]["margins"]
                units.append({"index": index, "expected": [plan.left, plan.intent], "com": actual,
                              "case": margin[0], "default": margin[1]})
            unit_errors = []
            for row in units:
                for k, expected in zip(("left", "intent"), row["expected"]):
                    if (row["case"][k] != (expected, "HWPUNIT")
                            or row["default"][k] != (expected * 2, "HWPUNIT")
                            or row["com"][("left", "intent").index(k)] != expected * 2):
                        unit_errors.append({"index": row["index"], "field": k, "values": row})
            report = {"version": version, "cases": len(items), "plans": len(rows), "changed": changed,
                      "hwp_gate_enabled_at_measurement": it.enabled(
                          hwp, lambda: {}, out / "source.hwp", converted_hwpx=out / "working.hwpx"),
                      "max_error_pt": max((r["error_pt"] for r in rows), default=0),
                      "over_half_pt": [r for r in rows if r["error_pt"] > .5 or not r["ok"]],
                      "fallback": [items[i - 1].tag for i in range(1, len(items) + 1) if i not in document.plans],
                      "pages": [before_pages, shift_pages, table_pages],
                      "before_vs_table": compare(before, after),
                      "shift_vs_table": compare(inventory(out / "shift.hwpx"), after),
                      "source_unchanged": source_hash == hashlib.sha256((out / "source.hwp").read_bytes()).hexdigest(),
                      "rows": rows, "roundtrip_units": units, "unit_errors": unit_errors}
            write_json(out / "validation.json", report)
            print(json.dumps({k: v for k, v in report.items() if k not in ("rows", "roundtrip_units", "fallback")},
                             ensure_ascii=False, indent=2), flush=True)
        finally:
            hangul.quit()


def benchmark_run(source, out, enabled):
    """별도 프로세스에서 실제 파이프라인 실행. 설정은 시험 폴더에만 둔다."""
    from docfit_core.app_state import RunOptions
    from docfit_engine.pipeline import PipelineCallbacks
    from docfit_engine.runtime import session_for
    if (os.environ.get("DOCFIT_COM_LOCK_PATH") != str(DEFAULT_COM_LOCK_PATH.resolve())
            or not DEFAULT_COM_LOCK_PATH.is_file()):
        raise RuntimeError("부모 실행의 COM 배타 잠금이 필요합니다.")
    out.mkdir(parents=True, exist_ok=False)
    settings = out / "appdata" / "HwpAutoDocFit"
    settings.mkdir(parents=True)
    write_json(settings / "settings.json", {"indent_table": enabled})
    os.environ["APPDATA"] = str(settings.parent)
    os.environ["DOCFIT_OFFLINE"] = "1"
    os.environ["DOCFIT_COM_PERF"] = "1"
    os.environ["DOCFIT_COM_PERF_DIR"] = str(out / "perf")
    ns = engine()
    # 합성 프로필: 양쪽 정렬 HFT 문단과 왼쪽 정렬 TTF 문단을 실제 서식 적용에도 섞는다.
    profile = copy.deepcopy(ns["표준서식_설정"])
    profile["기호_규칙"] = [tuple([*rule[:2], "신명 중명조", *rule[3:]]) if rule[0] == "ㅇ" else rule
                            for rule in profile["기호_규칙"]]
    profile["글꼴형식"] = {"신명 중명조": "HFT"}
    profile["계층_추가서식"] = {symbol: {"align": "JUSTIFY"} for symbol in ("ㅇ", "※", "*")}
    ns["표준서식_설정"] = profile
    # Dispatch 폴백·보안 DLL 설치/등록을 하지 않는다. 이미 등록된 모듈만 사용한다.
    import win32com.client
    from docfit_core import hwp_process
    owned_pids = set()

    def dispatch(**_):
        with hwp_process.spawn_guard():
            before = hwp_process.hwp_pids()
            app = win32com.client.DispatchEx("HwpFrame.HwpObject")
            owned = hwp_process.hwp_pids() - before
            if len(owned) != 1:
                raise RuntimeError("새 한/글 프로세스 하나를 특정하지 못했습니다.")
            owned_pids.update(owned)
            hwp_process.register_owned(owned)
            return app

    ns["한글_COM_인스턴스_생성"] = dispatch
    ns["보안모듈_초기화"] = lambda: None
    start_hangul = ns["한글_시작"]

    def minimized_start():
        from types import SimpleNamespace
        from docfit_engine.context import current_context
        start_hangul()
        tool.Hangul.minimize(SimpleNamespace(app=current_context().state["hwp"],
                                            pids=hwp_process.hwp_pids() & owned_pids))

    ns["한글_시작"] = minimized_start
    close_hangul = ns["작업창_닫기"]

    def discard_and_close():
        from docfit_engine.context import current_context
        app = current_context().state.get("hwp")
        if app is not None:
            app.Clear(1)
        close_hangul()

    ns["작업창_닫기"] = discard_and_close
    copied = out / ("source" + source.suffix)
    shutil.copy2(source, copied)
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    events, logs = [], []
    def log(message):
        logs.append(message)
        with (out / "engine.log").open("a", encoding="utf-8") as stream:
            stream.write(message + "\n")

    callbacks = PipelineCallbacks(event=events.append, log=log,
                                 on_stage_enter=lambda name: print("stage:", name, flush=True))
    context = session_for(ns).new_context(options=RunOptions(mode="format", verify_result=False, autoclose=True),
                                          callbacks=callbacks)
    started = time.perf_counter()
    ns["작업_실행"]([str(copied)], context=context, callbacks=callbacks, 표준서식=True, 준말_등록={})
    saved = [e for e in events if e[0] == "saved"]
    report = {"seconds": time.perf_counter() - started, "saved": saved,
              "errors": [e for e in events if e[0] in ("fatal_error", "document_error")],
              "original_unchanged": digest == hashlib.sha256(source.read_bytes()).hexdigest(),
              "copy_unchanged": digest == hashlib.sha256(copied.read_bytes()).hexdigest()}
    write_json(out / "report.json", report)
    (out / "engine.log").write_text("\n".join(logs), encoding="utf-8")
    assert len(saved) == 1 and not report["errors"] and report["original_unchanged"] and report["copy_unchanged"], report


def benchmark(out):
    out.mkdir(parents=True, exist_ok=False)
    # 크기·정렬·HFT 혼합은 그대로 두고, 긴 문장이 서로 다른 14문단으로 측정한다.
    selected = cases()[0:12:2] + cases()[-8:]
    selected = [replace(case, runs=(*case.runs[:-1], (case.runs[-1][0] + f" 제{index + 1}항 고유 기록.",
                                                    case.runs[-1][1]))) for index, case in enumerate(selected)]
    tool.build_hwpx(selected, out / "synthetic.hwpx")
    with exclusive():
        hangul = SyntheticHangul()
        try:
            hangul.open(out / "synthetic.hwpx")
            hangul.save(out / "input.hwpx")
            hangul.save(out / "input.hwp")
        finally:
            hangul.quit()
        for kind in ("hwpx", "hwp"):
            for repeat in (1, 2):
                for enabled in (False, True):
                    name = f"{kind}-{repeat}-{'on' if enabled else 'off'}"
                    print("benchmark:", name, flush=True)
                    args = [sys.executable, str(Path(__file__).resolve()), "run", "--out", str(out / name),
                            "--source", str(out / ("input." + kind)), "--enabled", str(int(enabled))]
                    with (out / (name + ".stdout.txt")).open("w", encoding="utf-8") as stream:
                        subprocess.run(args, cwd=ROOT, check=True, stdout=stream, stderr=subprocess.STDOUT)
        results = []
        for kind in ("hwpx", "hwp"):
            for repeat in (1, 2):
                a, b = [out / f"{kind}-{repeat}-{flag}" for flag in ("off", "on")]
                reports = [json.loads((p / "report.json").read_text(encoding="utf-8")) for p in (a, b)]
                outputs = [Path(r["saved"][0][2]) for r in reports]
                checks = compare(*[inventory(p) for p in outputs])
                perf = [json.loads(next((p / "perf").glob("*.comperf.json")).read_text(encoding="utf-8")) for p in (a, b)]
                results.append({"kind": kind, "repeat": repeat, "comparison": checks, "reports": reports,
                                "perf": perf, "coverage": [coverage(p) for p in outputs]})
        write_json(out / "benchmark.json", results)
        print("benchmark complete", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate", "benchmark", "run"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--enabled", type=int, choices=(0, 1))
    args = parser.parse_args()
    out = args.out.resolve()
    if out.is_relative_to(ROOT):
        parser.error("출력 폴더는 저장소 밖이어야 합니다.")
    if args.command == "validate":
        validate(out)
    elif args.command == "benchmark":
        benchmark(out)
    else:
        benchmark_run(args.source, out, bool(args.enabled))


if __name__ == "__main__":
    main()
