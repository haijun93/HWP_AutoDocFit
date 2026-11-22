"""Compare two ALLQA labels for timing, COM use, document fidelity and QA checks."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from zipfile import ZipFile

from defusedxml import ElementTree as ET

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))
from analyze_hwpx_diff import compare as compare_hwpx  # noqa: E402


def xml_hashes(path: Path) -> dict[str, str]:
    """Hash normalized text-bearing section XML and formatting XML from a HWPX."""
    text = hashlib.sha256()
    formatting = hashlib.sha256()
    with ZipFile(path) as archive:
        for name in sorted(n for n in archive.namelist()
                           if n.startswith("Contents/") and n.endswith(".xml")):
            root = ET.fromstring(archive.read(name))
            if re.fullmatch(r"Contents/section\d+\.xml", name):
                for elem in root.iter():
                    if elem.tag.rsplit("}", 1)[-1] == "t":
                        text.update((elem.text or "").encode("utf-8"))
                text.update(b"\0")
            else:
                formatting.update(name.encode("utf-8"))
                formatting.update(ET.tostring(root, encoding="utf-8"))
    return {"text_xml_sha256": text.hexdigest(), "format_xml_sha256": formatting.hexdigest()}


def load_label(path: Path) -> dict:
    return json.loads((path / "allqa.json").read_text(encoding="utf-8"))


def _seconds(report: dict) -> float:
    return float(report.get("seconds", report.get("com", {}).get("total_seconds", 0)) or 0)


def qa_checks(report: dict, result_path: Path | None) -> list[tuple[str, str]]:
    checks = []
    checks.append(("원본 해시 불변", "PASS" if report.get("source_sha256_before") == report.get("source_sha256_after") else "FAIL"))
    checks.append(("처리 사본 해시 불변", "PASS" if report.get("copy_sha256") == report.get("copy_sha256_after") else "FAIL"))
    checks.append(("오류 0", "PASS" if report.get("returncode") == 0 and not report.get("errors") else "FAIL"))
    checks.append(("결과 저장", "PASS" if result_path and result_path.is_file() and report.get("saved") == 1 else "FAIL"))
    preserved = report.get("text_preserved_ignoring_whitespace")
    checks.append(("원문 텍스트 보존(공백 무시)", "PASS" if preserved == [True] else "FAIL"))
    checks.append(("문단 수 기록", "PASS" if report.get("paragraph_counts") else "UNMEASURED"))
    checks.append(("쪽 수 기록", "PASS" if report.get("page_count") is not None else "UNMEASURED"))
    stage_check = report.get("enabled_stages_check") or {}
    checks.append(("선택된 단계 실행 로그", "PASS" if stage_check.get("pass") else "FAIL" if stage_check.get("missing") else "UNMEASURED"))
    return checks


def render(before: dict, after: dict, before_dir: Path, after_dir: Path) -> str:
    lines = ["# ALLQA 전후 비교", "", f"- 전: `{before['label']}`", f"- 후: `{after['label']}`", "",
             "## 문서별 시간·COM", "", "| 문서 | 단계/전체 | 전(초) | 후(초) | 증감(%) | 전 COM | 후 COM |", "|---|---|---:|---:|---:|---:|---:|"]
    left = {r["id"]: r for r in before["results"]}
    right = {r["id"]: r for r in after["results"]}
    fidelity = []
    defects = []
    for ident in sorted(set(left) | set(right)):
        if ident not in left or ident not in right:
            lines.append(f"| {ident} | 전체 | — | — | — | — | — |")
            defects.append(f"{ident}: 한쪽 측정 자료 없음")
            continue
        a, b = left[ident], right[ident]
        a_stages = {s["name"]: s for s in a.get("com", {}).get("stages", [])}
        b_stages = {s["name"]: s for s in b.get("com", {}).get("stages", [])}
        for stage in sorted(set(a_stages) | set(b_stages)):
            av = float(a_stages.get(stage, {}).get("seconds", 0)); bv = float(b_stages.get(stage, {}).get("seconds", 0))
            pct = (bv - av) / av * 100 if av else None
            lines.append(f"| {ident} | {stage} | {av:.3f} | {bv:.3f} | {pct:+.1f}% | {a_stages.get(stage, {}).get('com_calls', 0)} | {b_stages.get(stage, {}).get('com_calls', 0)} |" if pct is not None else f"| {ident} | {stage} | {av:.3f} | {bv:.3f} | — | {a_stages.get(stage, {}).get('com_calls', 0)} | {b_stages.get(stage, {}).get('com_calls', 0)} |")
        av, bv = _seconds(a), _seconds(b)
        pct = (bv - av) / av * 100 if av else None
        lines.append(f"| {ident} | **전체** | **{av:.3f}** | **{bv:.3f}** | **{pct:+.1f}%** | **{a.get('com', {}).get('com_calls', 0)}** | **{b.get('com', {}).get('com_calls', 0)}** |" if pct is not None else f"| {ident} | **전체** | **{av:.3f}** | **{bv:.3f}** | — | — | — |")
        before_result = next(iter(a.get("result_files", [])), None)
        after_result = next(iter(b.get("result_files", [])), None)
        if before_result and after_result and Path(before_result).is_file() and Path(after_result).is_file():
            diff = compare_hwpx(Path(before_result), Path(after_result))
            hashes_a, hashes_b = xml_hashes(Path(before_result)), xml_hashes(Path(after_result))
            fidelity.append((ident, diff, hashes_a, hashes_b))
        else:
            fidelity.append((ident, None, {}, {}))
            defects.append(f"{ident}: 결과 HWPX 누락")
    lines += ["", "## 결과 문서 동일성", "", "| 문서 | 문단 수 전→후 | 변경 문단 | 텍스트 XML 해시 | 서식 XML 해시 |", "|---|---:|---:|---|---|"]
    for ident, diff, hashes_a, hashes_b in fidelity:
        if diff is None:
            lines.append(f"| {ident} | — | 측정 불가 | 측정 불가 | 측정 불가 |")
        else:
            counts = diff["paragraph_count"]
            text_equal = hashes_a.get("text_xml_sha256") == hashes_b.get("text_xml_sha256")
            format_equal = hashes_a.get("format_xml_sha256") == hashes_b.get("format_xml_sha256")
            lines.append(f"| {ident} | {counts['before']}→{counts['after']} | {len(diff['changed_paragraphs'])} | {'일치' if text_equal else '불일치'} | {'일치' if format_equal else '불일치'} |")
            if diff["changed_paragraphs"] or not text_equal or not format_equal:
                defects.append(f"{ident}: 결과 HWPX 차이 {len(diff['changed_paragraphs'])} 문단, 텍스트 XML 동일={text_equal}, 서식 XML 동일={format_equal}")
    lines += ["", "## 자동 기능 검수", "", "| 문서 | 항목 | 판정 |", "|---|---|---|"]
    for result_label, corpus in ((before["label"], left), (after["label"], right)):
        for ident, report in sorted(corpus.items()):
            result = Path(report["result_files"][0]) if report.get("result_files") else None
            for label, verdict in qa_checks(report, result):
                lines.append(f"| {result_label}/{ident} | {label} | {verdict} |")
                if verdict != "PASS": defects.append(f"{result_label}/{ident}: {label} {verdict}")
    lines += ["", "## 결함 목록", ""]
    lines.extend(f"- {item}" for item in defects) if defects else lines.append("- 확인된 결함 없음")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path); parser.add_argument("after", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    markdown = render(load_label(args.before), load_label(args.after), args.before, args.after)
    args.output.write_text(markdown, encoding="utf-8")
    print(markdown)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
