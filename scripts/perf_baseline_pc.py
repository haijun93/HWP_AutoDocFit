"""같은 문서로 단계별 시간과 한/글 COM 호출 수·시간의 기준선을 잰다(Windows + 한/글 필요).

문서마다 ``scripts/smoke_engine_pc.py``를 따로 실행해(사본으로 처리, 원본 불변) COM 계측
(``DOCFIT_COM_PERF=1``)을 켜고, 결과를 한 폴더에 모아 ``baseline.json``과 ``baseline.md``로 정리한다.

    python scripts/perf_baseline_pc.py 문서1.hwpx 문서2.hwpx --mode all --out E:/docfit-tmp/perf/2026-11-12

측정은 1회만 한다(반복 회차 없음). 측정 중에는 다른 시험·빌드를 돌리지 않는다.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]


def summarize(reports: list[dict]) -> dict:
    """문서별 보고서를 모아 단계별·호출별 합계를 만든다(순수 함수)."""
    stages: dict[str, dict] = {}
    calls: dict[str, dict] = {}
    for report in reports:
        for stage in report.get("stages", []):
            item = stages.setdefault(stage["name"], {
                "name": stage["name"], "seconds": 0.0, "com_calls": 0, "com_seconds": 0.0, "python_seconds": 0.0})
            for key in ("seconds", "com_calls", "com_seconds", "python_seconds"):
                item[key] += stage.get(key, 0)
        for call in report.get("top_calls", []):
            item = calls.setdefault(call["name"], {"name": call["name"], "calls": 0, "seconds": 0.0})
            item["calls"] += call.get("calls", 0)
            item["seconds"] += call.get("seconds", 0.0)
    total = sum(report.get("total_seconds", 0.0) for report in reports)
    com_seconds = sum(report.get("com_seconds", 0.0) for report in reports)
    return {
        "documents": len(reports),
        "total_seconds": round(total, 3),
        "com_calls": sum(report.get("com_calls", 0) for report in reports),
        "com_seconds": round(com_seconds, 3),
        "python_seconds": round(max(total - com_seconds, 0.0), 3),
        "com_share": round(com_seconds / total, 4) if total > 0 else None,
        "stages": sorted(stages.values(), key=lambda item: -item["seconds"]),
        "top_calls": sorted(calls.values(), key=lambda item: -item["seconds"]),
    }


def to_markdown(summary: dict, reports: list[dict], mode: str, version: str) -> str:
    """요약을 사람이 읽는 표로 바꾼다(순수 함수)."""
    share = summary.get("com_share")
    lines = [
        f"# 처리 시간·COM 호출 기준선 (v{version}, 작업 방식 {mode})",
        "",
        f"- 문서 {summary['documents']}개, 전체 {summary['total_seconds']:.1f}초",
        f"- 한/글 COM: {summary['com_calls']:,}회, {summary['com_seconds']:.1f}초"
        + (f"({share:.0%})" if share is not None else ""),
        f"- 우리 코드(파이썬): {summary['python_seconds']:.1f}초",
        "",
        "## 문서별",
        "",
        "| 문서 | 전체(초) | COM 호출 | COM(초) | 파이썬(초) |",
        "|---|---:|---:|---:|---:|",
    ]
    for report in reports:
        lines.append("| {file} | {total_seconds:.1f} | {com_calls:,} | {com_seconds:.1f} | {python_seconds:.1f} |".format(**report))
    lines += ["", "## 단계별(오래 걸린 순)", "", "| 단계 | 시간(초) | COM 호출 | COM(초) | 파이썬(초) |", "|---|---:|---:|---:|---:|"]
    for stage in summary["stages"][:25]:
        lines.append("| {name} | {seconds:.1f} | {com_calls:,} | {com_seconds:.1f} | {python_seconds:.1f} |".format(**stage))
    lines += ["", "## COM 호출별(오래 걸린 순)", "", "| 호출 | 횟수 | 시간(초) |", "|---|---:|---:|"]
    for call in summary["top_calls"][:25]:
        lines.append("| `{name}` | {calls:,} | {seconds:.1f} |".format(**call))
    lines += ["", "계측을 켜면 대리 객체를 거치느라 파이썬 쪽 시간이 조금 늘어난다. 전후 비교는 같은 조건(계측 켬)끼리 한다.", ""]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("documents", nargs="+", type=Path)
    parser.add_argument("--mode", choices=("spacing", "unify", "format", "all"), default="all")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Windows + 한/글 COM 환경이 필요합니다.")
    sys.path.insert(0, str(REPO))
    from docfit_core.save_guard import com_test_lock

    out = args.out.resolve()
    perf_dir = out / "comperf"
    perf_dir.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, DOCFIT_COM_PERF="1", PYTHONUTF8="1")
    failures = []
    version = ""
    with com_test_lock("perf_baseline_pc", ",".join(path.name for path in args.documents)):
        env["DOCFIT_COM_LOCK_PATH"] = os.environ.get("DOCFIT_COM_LOCK_PATH", "")
        for index, document in enumerate(args.documents, 1):
            run_dir = out / f"run{index:02d}"
            print(f"[{index}/{len(args.documents)}] {document.name}", flush=True)
            env["DOCFIT_COM_PERF_DIR"] = str(perf_dir / f"run{index:02d}")
            done = subprocess.run(
                [sys.executable, str(REPO / "scripts" / "smoke_engine_pc.py"), str(document),
                 "--mode", args.mode, "--out", str(run_dir)],
                env=env, cwd=str(REPO), capture_output=True, text=True, encoding="utf-8", errors="replace")
            (out / f"run{index:02d}.stdout.txt").write_text(done.stdout + "\n" + done.stderr, encoding="utf-8")
            if done.returncode != 0:
                failures.append(document.name)
            report_file = run_dir / "report.json"
            if report_file.is_file():
                version = json.loads(report_file.read_text(encoding="utf-8")).get("version", version)
    # 처리 사본 이름이 모두 source.hwpx라 계측 파일이 덮어써지지 않도록 실행 폴더별로 읽는다.
    reports = []
    for index, document in enumerate(args.documents, 1):
        found = sorted((perf_dir / f"run{index:02d}").glob("*.comperf.json"))
        if found:
            report = json.loads(found[0].read_text(encoding="utf-8"))
            report["file"] = document.name
            reports.append(report)
    summary = summarize(reports)
    summary.update({"mode": args.mode, "version": version, "failures": failures})
    (out / "baseline.json").write_text(
        json.dumps({"summary": summary, "documents": reports}, ensure_ascii=False, indent=2), encoding="utf-8")
    (out / "baseline.md").write_text(to_markdown(summary, reports, args.mode, version), encoding="utf-8")
    print(to_markdown(summary, reports, args.mode, version))
    return 1 if failures or len(reports) != len(args.documents) else 0


if __name__ == "__main__":
    raise SystemExit(main())
