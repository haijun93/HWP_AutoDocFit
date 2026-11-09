"""tools/startup_import_total.py

앱 시작 경로 import 시간 합계를 측정하고 브랜치/커밋 간 비교를 수행하는 도구.
hwp-auto-docfit.py 모듈을 import만 수행(exec_module)하여 GUI 창을 띄우지 않고,
Python -X importtime의 self [us] 총합, 누적 시간, 프로세스 경과 시간(wall clock)을 측정합니다.
"""

from __future__ import annotations

import argparse
import io
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path


IMPORT_TIME_RE = re.compile(
    r"^import time:\s+(\d+)\s+\|\s+(\d+)\s+\|\s+(.+)$"
)


def _run_worker(target_dir: str) -> None:
    """GUI 창을 띄우지 않고 hwp-auto-docfit.py 모듈만 import합니다."""
    import importlib.util

    target_path = Path(target_dir).resolve()
    script_path = target_path / "hwp-auto-docfit.py"
    if not script_path.exists():
        raise FileNotFoundError(f"Script not found: {script_path}")

    # Ensure target_dir is at the front of sys.path
    if str(target_path) not in sys.path:
        sys.path.insert(0, str(target_path))

    spec = importlib.util.spec_from_file_location("hwp_auto_docfit", str(script_path))
    if spec is None or spec.loader is None:
        raise ImportError(f"Failed to create spec for {script_path}")

    mod = importlib.util.module_from_spec(spec)
    sys.modules["hwp_auto_docfit"] = mod
    spec.loader.exec_module(mod)


def parse_importtime_output(stderr_text: str) -> dict:
    """-X importtime 표준 에러 출력을 파싱하여 요약 정보를 반환합니다."""
    modules = []
    total_self_us = 0
    package_times: dict[str, dict[str, int]] = {}

    for line in stderr_text.splitlines():
        match = IMPORT_TIME_RE.match(line.strip())
        if not match:
            continue
        self_us = int(match.group(1))
        cumulative_us = int(match.group(2))
        pkg_name = match.group(3).strip()

        total_self_us += self_us
        modules.append({
            "name": pkg_name,
            "self_us": self_us,
            "cumulative_us": cumulative_us,
        })
        package_times[pkg_name] = {
            "self_us": self_us,
            "cumulative_us": cumulative_us,
        }

    return {
        "module_count": len(modules),
        "total_self_us": total_self_us,
        "total_self_ms": total_self_us / 1000.0,
        "package_times": package_times,
        "raw_entries": modules,
    }


def measure_single_run(
    python_exe: str,
    target_dir: str,
    tool_script: str,
) -> dict:
    """단일 실행으로 -X importtime 및 wall clock 시간을 측정합니다."""
    cmd = [
        python_exe,
        "-X",
        "importtime",
        str(tool_script),
        "--import-worker",
        "--target-dir",
        str(target_dir),
    ]

    t0 = time.perf_counter()
    proc = subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    wall_ms = (time.perf_counter() - t0) * 1000.0

    if proc.returncode != 0:
        raise RuntimeError(
            f"Measurement subprocess failed (code {proc.returncode}):\nSTDOUT: {proc.stdout}\nSTDERR: {proc.stderr}"
        )

    parsed = parse_importtime_output(proc.stderr)
    parsed["wall_ms"] = wall_ms
    return parsed


def measure_multiple_runs(
    python_exe: str,
    target_dir: str,
    tool_script: str,
    runs: int = 3,
) -> dict:
    """지정 횟수만큼 측정을 반복하고 평균을 계산합니다."""
    results = []
    for i in range(runs):
        res = measure_single_run(python_exe, target_dir, tool_script)
        results.append(res)

    avg_self_us = sum(r["total_self_us"] for r in results) / runs
    avg_self_ms = avg_self_us / 1000.0
    avg_wall_ms = sum(r["wall_ms"] for r in results) / runs
    avg_mod_count = sum(r["module_count"] for r in results) / runs

    # Key modules check in the last run (or across runs)
    last_packages = results[-1]["package_times"]
    key_targets = [
        "docfit_core.report_header",
        "docfit_core.style_inventory",
        "docfit_engine.formatter_apply",
        "docfit_engine.formatter_titles",
        "docfit_engine.formatter_unify",
        "docfit_engine.formatter_text",
        "docfit_engine.formatter_tables",
        "docfit_engine.formatter_indent",
        "docfit_engine.formatter_analysis",
    ]

    key_module_status = {}
    for mod in key_targets:
        if mod in last_packages:
            key_module_status[mod] = {
                "loaded": True,
                "self_us": last_packages[mod]["self_us"],
                "cumulative_us": last_packages[mod]["cumulative_us"],
            }
        else:
            key_module_status[mod] = {
                "loaded": False,
                "self_us": 0,
                "cumulative_us": 0,
            }

    return {
        "runs": runs,
        "individual_runs": results,
        "avg_self_us": avg_self_us,
        "avg_self_ms": avg_self_ms,
        "avg_wall_ms": avg_wall_ms,
        "avg_module_count": avg_mod_count,
        "key_module_status": key_module_status,
    }


def extract_git_ref(ref: str, dest_dir: str, repo_root: str) -> None:
    """git archive를 통해 지정 git ref의 코드를 dest_dir로 추출합니다."""
    cmd = ["git", "-C", repo_root, "archive", ref]
    p = subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    with tarfile.open(fileobj=io.BytesIO(p.stdout), mode="r:*") as tar:
        tar.extractall(dest_dir)


def format_markdown_report(
    base_name: str,
    base_res: dict,
    curr_name: str,
    curr_res: dict,
) -> str:
    """측정 결과 비교 표를 마크다운 형식으로 포맷팅합니다."""
    b_self_ms = base_res["avg_self_ms"]
    c_self_ms = curr_res["avg_self_ms"]
    diff_self_ms = c_self_ms - b_self_ms
    pct_self = (diff_self_ms / b_self_ms * 100.0) if b_self_ms > 0 else 0.0

    b_wall = base_res["avg_wall_ms"]
    c_wall = curr_res["avg_wall_ms"]
    diff_wall = c_wall - b_wall
    pct_wall = (diff_wall / b_wall * 100.0) if b_wall > 0 else 0.0

    b_cnt = base_res["avg_module_count"]
    c_cnt = curr_res["avg_module_count"]
    diff_cnt = c_cnt - b_cnt

    lines = []
    lines.append("### 1. 시작 경로 Import 시간 및 소요 시간 비교 (3회 평균)")
    lines.append("")
    lines.append(f"| 지표 | 기준 ({base_name}) | 현재 ({curr_name}) | 변화량 | 변화율 |")
    lines.append("|---|---|---|---|---|")
    lines.append(
        f"| **Import 합계 (Self 총합)** | **{b_self_ms:.2f} ms** | **{c_self_ms:.2f} ms** | **{diff_self_ms:+.2f} ms** | **{pct_self:+.1f}%** |"
    )
    lines.append(
        f"| 프로세스 경과 시간 (Wall clock) | {b_wall:.2f} ms | {c_wall:.2f} ms | {diff_wall:+.2f} ms | {pct_wall:+.1f}% |"
    )
    lines.append(
        f"| 로드된 모듈 수 | {b_cnt:.0f}개 | {c_cnt:.0f}개 | {diff_cnt:+.0f}개 | - |"
    )
    lines.append("")

    lines.append("### 2. 회차별 측정 세부 데이터 (각 3회)")
    lines.append("")
    lines.append(f"| 회차 | {base_name} Import 합계 | {base_name} Wall clock | {curr_name} Import 합계 | {curr_name} Wall clock |")
    lines.append("|---|---|---|---|---|")
    runs = min(len(base_res["individual_runs"]), len(curr_res["individual_runs"]))
    for i in range(runs):
        b_r = base_res["individual_runs"][i]
        c_r = curr_res["individual_runs"][i]
        lines.append(
            f"| Run #{i+1} | {b_r['total_self_ms']:.2f} ms | {b_r['wall_ms']:.2f} ms | {c_r['total_self_ms']:.2f} ms | {c_r['wall_ms']:.2f} ms |"
        )
    lines.append(
        f"| **평균** | **{b_self_ms:.2f} ms** | **{b_wall:.2f} ms** | **{c_self_ms:.2f} ms** | **{c_wall:.2f} ms** |"
    )
    lines.append("")

    lines.append("### 3. 주요 지연 로딩 대상 모듈 로드 상태")
    lines.append("")
    lines.append(f"| 모듈명 | 기준 ({base_name}) | 현재 ({curr_name}) | 로드 제거/지연 확인 |")
    lines.append("|---|---|---|---|")

    all_mods = sorted(
        set(base_res["key_module_status"].keys()) | set(curr_res["key_module_status"].keys())
    )
    for mod in all_mods:
        b_info = base_res["key_module_status"].get(mod, {"loaded": False, "self_us": 0})
        c_info = curr_res["key_module_status"].get(mod, {"loaded": False, "self_us": 0})

        b_str = f"로드됨 ({b_info['self_us'] / 1000.0:.2f} ms)" if b_info["loaded"] else "미로드 (0.0 ms)"
        c_str = f"로드됨 ({c_info['self_us'] / 1000.0:.2f} ms)" if c_info["loaded"] else "미로드 (0.0 ms)"

        if b_info["loaded"] and not c_info["loaded"]:
            verdict = "✅ 지연 로딩 완료 (시작 경로 제외)"
        elif not b_info["loaded"] and not c_info["loaded"]:
            verdict = "미로드 유지"
        elif b_info["loaded"] and c_info["loaded"]:
            verdict = "로드 유지"
        else:
            verdict = "신규 로드"

        lines.append(f"| `{mod}` | {b_str} | {c_str} | {verdict} |")

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="앱 시작 경로 import 총합 측정 및 브랜치 비교 스크립트"
    )
    parser.add_argument(
        "--import-worker",
        action="store_true",
        help="GUI 없이 hwp-auto-docfit.py import만 수행하는 내부 워커 모드",
    )
    parser.add_argument(
        "--target-dir",
        default=".",
        help="측정 대상 디렉터리 (기본값: 현재 디렉터리)",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=3,
        help="측정 반복 횟수 (기본값: 3)",
    )
    parser.add_argument(
        "--compare-ref",
        help="비교할 git ref (예: github/release/v2.2.47-int)",
    )
    parser.add_argument(
        "--python",
        help="사용할 Python 실행 파일 경로 (기본값: sys.executable)",
    )
    parser.add_argument(
        "--markdown",
        action="store_true",
        help="결과를 마크다운 형식으로 출력",
    )

    args = parser.parse_args()

    if args.import_worker:
        _run_worker(args.target_dir)
        return

    python_exe = args.python or sys.executable
    tool_script = Path(__file__).resolve()
    repo_root = Path(args.target_dir).resolve()

    print(f"=== 시작 속도 Import 총합 측정 도구 ===")
    print(f"Python: {python_exe}")
    print(f"Target: {repo_root}")
    print(f"Runs: {args.runs}회")

    if args.compare_ref:
        print(f"Comparing with git ref: {args.compare_ref}")
        tmp_base = tempfile.mkdtemp(prefix="docfit_base_")
        try:
            print(f"Extracting ref {args.compare_ref} to temporary directory...")
            extract_git_ref(args.compare_ref, tmp_base, str(repo_root))

            print(f"\n[1/2] 기준 ({args.compare_ref}) 측정 중 (3회)...")
            base_res = measure_multiple_runs(
                python_exe, tmp_base, str(tool_script), runs=args.runs
            )
            print(f"  -> 기준 Import 합계: {base_res['avg_self_ms']:.2f} ms")

            print(f"\n[2/2] 현재 브랜치 ({repo_root.name}) 측정 중 (3회)...")
            curr_res = measure_multiple_runs(
                python_exe, str(repo_root), str(tool_script), runs=args.runs
            )
            print(f"  -> 현재 Import 합계: {curr_res['avg_self_ms']:.2f} ms")

            print("\n" + "=" * 50)
            report_md = format_markdown_report(
                args.compare_ref,
                base_res,
                "feature/rust-launch-rl3c",
                curr_res,
            )
            print(report_md)

        finally:
            shutil.rmtree(tmp_base, ignore_errors=True)
    else:
        print(f"\n측정 진행 중 ({args.runs}회)...")
        res = measure_multiple_runs(
            python_exe, str(repo_root), str(tool_script), runs=args.runs
        )
        print(f"Import 시간 합계 (Self 총합): {res['avg_self_ms']:.2f} ms")
        print(f"프로세스 Wall clock: {res['avg_wall_ms']:.2f} ms")
        print(f"로드된 모듈 수: {res['avg_module_count']:.0f}개")
        for i, r in enumerate(res["individual_runs"]):
            print(f"  Run #{i+1}: Import {r['total_self_ms']:.2f} ms, Wall {r['wall_ms']:.2f} ms")


if __name__ == "__main__":
    main()
