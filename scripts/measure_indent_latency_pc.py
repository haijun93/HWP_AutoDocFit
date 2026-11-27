"""INDENTW5: 배타 잠금 아래 합성 HWP/HWPX 크기별 시간·보존 대조.

profile은 기존 구현의 비용 분해, matrix는 최종 구현의 끔/켬 각 2회이다.
출력은 저장소 밖에 두며 프로세스 강제 종료나 사용자 설정 변경을 하지 않는다.
"""
from __future__ import annotations

import argparse
import contextlib
from dataclasses import replace
import functools
import json
from pathlib import Path
import subprocess
import sys
import time
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import validate_indent_hwp_pc as probe
from docfit_core import indent_table as it


class Timings:
    """겹친 호출의 포함 시간과 순수 시간을 모두 기록한다."""
    def __init__(self):
        self.rows = {}
        self.stack = []

    def wrap(self, label, function):
        @functools.wraps(function)
        def measured(*args, **kwargs):
            grouped = "indent/" + label if any(f[2] == "indent_stages" for f in self.stack) else label
            frame = [time.perf_counter(), 0.0, label]
            self.stack.append(frame)
            try:
                return function(*args, **kwargs)
            finally:
                elapsed = time.perf_counter() - frame[0]
                self.stack.pop()
                row = self.rows.setdefault(grouped, {"calls": 0, "inclusive_seconds": 0., "exclusive_seconds": 0.})
                row["calls"] += 1
                row["inclusive_seconds"] += elapsed
                row["exclusive_seconds"] += elapsed - frame[1]
                if self.stack:
                    self.stack[-1][1] += elapsed
        return measured


def measured_run(source, out, enabled):
    timings = Timings()
    original_engine = probe.engine

    def engine():
        ns = original_engine()
        for name, label in (("_제목_임시hwpx_저장", "snapshot_save"),
                            ("_제목_임시폴더_정리", "snapshot_cleanup"),
                            ("_폭표_내어쓰기_검사", "stage_prepare"),
                            ("문단_내어쓰기_전체_갱신", "indent_stages")):
            ns[name] = timings.wrap(label, ns[name])
        return ns

    with contextlib.ExitStack() as stack:
        stack.enter_context(patch.object(probe, "engine", engine))
        for owner, name, label in (
            (it, "enabled", "environment_gate"),
            (it, "plan_document", "plan_document"),
            (it, "apply_in_place", "xml_write"),
            (it, "indent_width", "width_calculation"),
            (it, "_para_shape", "paragraph_condition"),
            (it, "_char_style", "character_condition"),
            (it, "_section_info", "section_condition"),
            (it, "cleanup_snapshot", "snapshot_cleanup"),
            (it.StageCheck, "matches", "com_recheck"),
            (it.PackageSnapshot, "load", "package_load"),
            (it.ET, "fromstring", "xml_parse"),
        ):
            stack.enter_context(patch.object(owner, name, timings.wrap(label, getattr(owner, name))))
        probe.benchmark_run(source, out, enabled)
    probe.write_json(out / "timings.json", timings.rows)


def child(source, out, enabled):
    args = [sys.executable, str(Path(__file__).resolve()), "run", "--out", str(out),
            "--source", str(source), "--enabled", str(int(enabled))]
    with out.with_suffix(".stdout.txt").open("w", encoding="utf-8") as stream:
        subprocess.run(args, cwd=ROOT, check=True, stdout=stream, stderr=subprocess.STDOUT)


def summarize(folder, count):
    rows = []
    for kind in ("hwpx", "hwp"):
        for repeat in (1, 2):
            dirs = [folder / f"{kind}-{repeat}-{flag}" for flag in ("off", "on")]
            reports = [json.loads((d / "report.json").read_text(encoding="utf-8")) for d in dirs]
            outputs = [Path(r["saved"][0][2]) for r in reports]
            inventories = [probe.inventory(p) for p in outputs]
            comparison = probe.compare(*inventories)
            for field in ("left", "intent"):
                comparison["max_" + field + "_error_pt"] = max(
                    abs(a["margins"][0][field][0] - b["margins"][0][field][0]) / 100
                    for a, b in zip(*inventories))
            rows.append({"paragraphs": count, "kind": kind, "repeat": repeat,
                         "input_bytes": (folder / ("input." + kind)).stat().st_size,
                         "comparison": comparison,
                         "reports": reports,
                         "perf": [json.loads(next((d / "perf").glob("*.comperf.json")).read_text(encoding="utf-8")) for d in dirs],
                         "timings": [json.loads((d / "timings.json").read_text(encoding="utf-8")) for d in dirs]})
    return rows


def matrix(out, sizes=(15, 60, 300)):
    out.mkdir(parents=True, exist_ok=False)
    seed = probe.cases()[0:12:2] + probe.cases()[-8:]
    results = []
    with probe.exclusive(owner="INDENTW5"):
        for count in (size - 1 for size in sizes):
            folder = out / str(count + 1)
            folder.mkdir()
            selected = []
            for index in range(count):
                case = seed[index % len(seed)]
                selected.append(replace(case, runs=(*case.runs[:-1],
                    (case.runs[-1][0] + f" 제{index + 1}항 고유 기록.", case.runs[-1][1]))))
            probe.tool.build_hwpx(selected, folder / "synthetic.hwpx")
            hangul = probe.SyntheticHangul()
            try:
                hangul.open(folder / "synthetic.hwpx")
                hangul.save(folder / "input.hwpx")
                hangul.save(folder / "input.hwp")
            finally:
                hangul.quit()
            for kind in ("hwpx", "hwp"):
                for repeat in (1, 2):
                    for enabled in (False, True):
                        name = f"{kind}-{repeat}-{'on' if enabled else 'off'}"
                        print(f"matrix: {count + 1}/{name}", flush=True)
                        child(folder / ("input." + kind), folder / name, enabled)
            results.extend(summarize(folder, count + 1))
            probe.write_json(out / "matrix.json", results)
    print("matrix complete", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("profile", "matrix", "run"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--enabled", type=int, choices=(0, 1), default=1)
    parser.add_argument("--sizes", type=int, nargs="+", choices=(15, 60, 300), default=(15, 60, 300))
    args = parser.parse_args()
    out = args.out.resolve()
    if out.is_relative_to(ROOT):
        parser.error("출력 폴더는 저장소 밖이어야 합니다.")
    if args.command == "run":
        measured_run(args.source.resolve(), out, bool(args.enabled))
    elif args.command == "profile":
        out.mkdir(parents=True, exist_ok=False)
        with probe.exclusive(owner="INDENTW5"):
            child(args.source.resolve(), out / "profile", bool(args.enabled))
    else:
        matrix(out, args.sizes)


if __name__ == "__main__":
    main()
