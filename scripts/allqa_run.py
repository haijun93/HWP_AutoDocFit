"""Run the ALLQA document corpus once through the Windows HWP all-mode pipeline."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
if str(REPO) not in sys.path:
    sys.path.insert(0, str(REPO))
DEFAULT_DOCSET = REPO / "docs" / "ALLQA_DOCSET.md"
TABLE_ROW = re.compile(r"^\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*$")
STAGE_HINTS = {
    "abbreviation": ("준말 변환",), "standard_format": ("보고서 표준서식",),
    "text_table_convert": ("텍스트 표",), "reset_spacing": ("자간 초기화",),
    "asterisk_superscript": ("별표 위첨자",), "attachment_font": ("붙임 선행 서식",),
    "pre_format": ("붙임 선행 서식",), "table_style": ("기본 표 서식",),
    "table_width": ("표 칸 너비",), "precise_table": ("한 칸 표 영역 조사",),
    "normalize_space": ("공백·문장부호·라벨",), "punctuation_space": ("공백·문장부호·라벨",),
    "parenthesis": ("괄호", "공백·문장부호"), "table_format": ("표 서식",),
    "single_cell_spacing": ("개요·한 칸 표 자간",), "hanging_indent": ("내어쓰기",),
    "supplement_indent": ("부연설명 들여쓰기",), "star_align": ("별표",),
    "body_spacing": ("본문 자간·단어 분리 조정",), "short_line": ("짧은 마지막 줄 병합",),
    "control_spacing": ("표/컨트롤 자간·단어 분리 조정",),
    "control_short_line": ("표/컨트롤 줄 병합",), "word_check": ("본문 자간·단어 분리 조정",),
    "control_word_check": ("표/컨트롤 자간·단어 분리 조정",),
    "page_fit": ("문단 아래 간격 페이지 맞춤",), "page_group": ("개별 문단 페이지 배치",),
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def enabled_stage_check(selected: dict, stages: list[dict]) -> dict:
    names = [item.get("name", "") for item in stages]
    enabled = [key for key, value in selected.items() if value is True]
    executed, unmapped, missing = [], [], []
    for key in enabled:
        hints = STAGE_HINTS.get(key)
        if not hints:
            unmapped.append(key)
        elif any(any(hint in name for name in names) for hint in hints):
            executed.append(key)
        else:
            missing.append(key)
    return {"enabled": enabled, "executed": executed, "missing": missing, "unmapped": unmapped,
            "pass": bool(enabled) and not missing and not unmapped}


def read_docset(path: Path) -> list[dict]:
    rows = []
    in_data = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip() == "<!-- ALLQA-DATA -->":
            in_data = True
            continue
        if line.strip() == "<!-- /ALLQA-DATA -->":
            break
        match = TABLE_ROW.match(line) if in_data else None
        if match and match.group(1).strip().lower() != "id" and not set(match.group(1).strip()) <= {"-", ":"}:
            ident, source, expected_hash, kind = (part.strip() for part in match.groups())
            rows.append({"id": ident, "source": source, "sha256": expected_hash, "kind": kind})
    if not rows:
        raise ValueError(f"문서 세트 데이터가 없습니다: {path}")
    return rows


def run(docset: Path, out: Path, docs_root: Path, label: str) -> dict:
    if sys.platform != "win32":
        raise RuntimeError("Windows + 한/글 COM 환경이 필요합니다.")
    from docfit_core.save_guard import com_test_lock

    destination = out.resolve() / label
    destination.mkdir(parents=True, exist_ok=False)
    perf_root = destination / "comperf"
    rows = read_docset(docset)
    env = dict(os.environ, DOCFIT_COM_PERF="1", PYTHONUTF8="1")
    results = []
    with com_test_lock("allqa_run", f"{label}: {len(rows)}개 문서"):
        env["DOCFIT_COM_LOCK_PATH"] = os.environ.get("DOCFIT_COM_LOCK_PATH", "")
        for index, item in enumerate(rows, 1):
            source = (REPO / item["source"]).resolve(strict=True)
            expected = item["sha256"]
            actual = sha256(source)
            if actual != expected:
                raise ValueError(f"픽스처 해시 불일치: {item['id']} {expected} != {actual}")
            docs_root.mkdir(parents=True, exist_ok=True)
            copy_path = docs_root / source.name
            if not copy_path.exists():
                shutil.copy2(source, copy_path)
            copied_hash = sha256(copy_path)
            if copied_hash != actual:
                raise ValueError(f"문서 사본 해시 불일치: {copy_path}")
            run_dir = destination / item["id"]
            report_dir = perf_root / f"run{index:02d}"
            run_dir.mkdir(parents=True)
            env["DOCFIT_COM_PERF_DIR"] = str(report_dir)
            report_dir.mkdir(parents=True)
            started = datetime.now().astimezone().isoformat()
            command = [sys.executable, str(REPO / "scripts" / "smoke_engine_pc.py"),
                       str(copy_path), "--mode", "all", "--out", str(run_dir / "engine")]
            completed = subprocess.run(command, cwd=REPO, env=env, capture_output=True,
                                       text=True, encoding="utf-8", errors="replace")
            (run_dir / "stdout.log").write_text(completed.stdout + completed.stderr, encoding="utf-8")
            report_path = run_dir / "engine" / "report.json"
            engine = json.loads(report_path.read_text(encoding="utf-8")) if report_path.exists() else {}
            perf_files = sorted(report_dir.glob("*.comperf.json"))
            perf = json.loads(perf_files[0].read_text(encoding="utf-8")) if perf_files else {}
            outputs = []
            for candidate in (run_dir / "engine").glob("*.hwpx"):
                if candidate.name == "source.hwpx":
                    continue
                target = run_dir / f"result{index:02d}.hwpx"
                shutil.copy2(candidate, target)
                outputs.append(str(target))
            engine.update({"id": item["id"], "kind": item["kind"], "source": item["source"],
                           "source_sha256_before": actual, "source_sha256_after": sha256(source),
                           "copy_sha256": copied_hash, "copy_sha256_after": sha256(copy_path), "started_at": started,
                           "returncode": completed.returncode, "page_count": None,
                           "result_files": outputs, "com": perf,
                           "log_file": str(run_dir / "engine" / "engine.log")})
            engine["page_count"] = (engine.get("page_counts") or [None])[0]
            engine["enabled_stages_check"] = enabled_stage_check(engine.get("stages_selected") or {}, perf.get("stages", []))
            (run_dir / "result.json").write_text(json.dumps(engine, ensure_ascii=False, indent=2), encoding="utf-8")
            results.append(engine)
            if completed.returncode:
                print(f"실패 {item['id']} (종료 {completed.returncode})", flush=True)
            else:
                print(f"완료 {item['id']}: {engine.get('seconds')}초", flush=True)
    index_data = {"label": label, "mode": "all", "docset": str(docset), "results": results}
    (destination / "allqa.json").write_text(json.dumps(index_data, ensure_ascii=False, indent=2), encoding="utf-8")
    return index_data


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docset", type=Path, default=DEFAULT_DOCSET)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--docs-dir", type=Path, default=Path(os.environ.get("ALLQA_DOCS_DIR", Path(tempfile.gettempdir()) / "allqa" / "docs")))
    parser.add_argument("--label", default="before")
    args = parser.parse_args()
    data = run(args.docset.resolve(), args.out, args.docs_dir, args.label)
    failures = [row["id"] for row in data["results"] if row.get("returncode") or row.get("errors")]
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
