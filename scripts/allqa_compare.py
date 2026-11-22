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


def _local_name(name: str) -> str:
    return name.rsplit("}", 1)[-1]


def _element_signature(element: ET.Element, *, omit_para_id: bool = False) -> tuple:
    """Return an attribute-order independent, text preserving XML signature."""
    attrs = tuple(sorted(
        (key, value) for key, value in element.attrib.items()
        if not (omit_para_id and _local_name(key) == "id")
    ))
    text = "" if _local_name(element.tag) == "t" else (element.text or "")
    children = tuple(_element_signature(child) for child in element)
    return element.tag, attrs, text, children


def _format_signature(element: ET.Element, para_defs: dict[str, tuple],
                      referenced_table_ids: set[str]) -> tuple:
    """Canonicalize only HWPX paragraph-property IDs and unreferenced table IDs."""
    attrs = []
    for key, value in element.attrib.items():
        local = _local_name(key)
        if local == "paraPrIDRef":
            value = repr(para_defs.get(value, ("unresolved-paraPrIDRef", value)))
        if (_local_name(element.tag) == "tbl" and local == "id"
                and value not in referenced_table_ids):
            continue
        attrs.append((key, value))

    children = list(element)
    if _local_name(element.tag) == "paraProperties":
        # Hancom may serialize equivalent paraPr definitions in a different order.
        # Keep other children in place and sort definitions only within their slots.
        positions = [index for index, child in enumerate(children)
                     if _local_name(child.tag) == "paraPr"]
        definitions = sorted(
            (_element_signature(child, omit_para_id=True) for child in children
             if _local_name(child.tag) == "paraPr"), key=repr)
        for index, definition in zip(positions, definitions):
            children[index] = definition
        normalized_children = tuple(
            child if isinstance(child, tuple) else
            _format_signature(child, para_defs, referenced_table_ids)
            for child in children
        )
    else:
        normalized_children = tuple(
            _format_signature(child, para_defs, referenced_table_ids) for child in children
        )
    text = "" if _local_name(element.tag) == "t" else (element.text or "")
    return element.tag, tuple(sorted(attrs)), text, normalized_children


def xml_hashes(path: Path) -> dict[str, str]:
    """Return exact text, raw header, and semantically normalized formatting hashes.

    Hancom assigns paraPr IDs as local references, so equivalent definitions can be
    reordered and their references rewritten on save. Unreferenced table IDs identify
    table objects but do not encode table formatting; they are excluded only when no
    XML ID reference points to them. All formatting values and element order remain
    in the normalized hash.
    """
    text = hashlib.sha256()
    raw_format = hashlib.sha256()
    normalized_format = hashlib.sha256()
    with ZipFile(path) as archive:
        all_xml_names = sorted(n for n in archive.namelist() if n.endswith(".xml"))
        all_roots = {name: ET.fromstring(archive.read(name)) for name in all_xml_names}
        names = [name for name in all_xml_names if name.startswith("Contents/")]
        roots = {name: all_roots[name] for name in names}
        para_defs: dict[str, tuple] = {}
        header = roots.get("Contents/header.xml")
        if header is not None:
            for element in header.iter():
                if _local_name(element.tag) == "paraProperties":
                    for definition in element:
                        if _local_name(definition.tag) == "paraPr" and "id" in definition.attrib:
                            para_defs[definition.attrib["id"]] = _element_signature(
                                definition, omit_para_id=True)

        all_ids = {element.attrib["id"] for root in all_roots.values() for element in root.iter()
                   if _local_name(element.tag) == "tbl" and "id" in element.attrib}
        referenced_table_ids = {
            value for root in all_roots.values() for element in root.iter()
            for key, value in element.attrib.items()
            if _local_name(key).lower().endswith("idref") and value in all_ids
        }

        for name in names:
            root = roots[name]
            if re.fullmatch(r"Contents/section\d+\.xml", name):
                for elem in root.iter():
                    if _local_name(elem.tag) == "t":
                        text.update((elem.text or "").encode("utf-8"))
                text.update(b"\0")
            else:
                raw_format.update(name.encode("utf-8"))
                raw_format.update(ET.tostring(root, encoding="utf-8"))
            normalized_format.update(name.encode("utf-8"))
            normalized_format.update(repr(_format_signature(
                root, para_defs, referenced_table_ids
            )).encode("utf-8"))
            normalized_format.update(b"\0")
    return {
        "text_xml_sha256": text.hexdigest(),
        "raw_format_xml_sha256": raw_format.hexdigest(),
        "format_xml_sha256": normalized_format.hexdigest(),
    }


def load_label(path: Path) -> dict:
    return json.loads((path / "allqa.json").read_text(encoding="utf-8"))


def _seconds(report: dict) -> float:
    return float(report.get("seconds", report.get("com", {}).get("total_seconds", 0)) or 0)


def _result_path(report: dict, label_dir: Path, ident: str) -> Path | None:
    for candidate in report.get("result_files", []):
        path = Path(candidate)
        if path.is_file():
            return path
    # ALLQA bundles are often moved after a run. Resolve stale absolute paths from
    # the portable label/document layout before declaring the result missing.
    candidates = sorted((label_dir / ident).glob("result*.hwpx"))
    return candidates[0] if candidates else None


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
        before_result = _result_path(a, before_dir, ident)
        after_result = _result_path(b, after_dir, ident)
        if before_result and after_result:
            diff = compare_hwpx(before_result, after_result)
            hashes_a, hashes_b = xml_hashes(before_result), xml_hashes(after_result)
            fidelity.append((ident, diff, hashes_a, hashes_b))
        else:
            fidelity.append((ident, None, {}, {}))
            defects.append(f"{ident}: 결과 HWPX 누락")
    lines += ["", "## 결과 문서 동일성", "",
              "서식 동일성은 `paraPr` 정의 내용으로 ID 참조를 풀고, 정의 순서를 정렬한 뒤 비교한다. "
              "다른 서식 값·요소 순서는 그대로 비교하며, 참조되지 않는 표 `id`만 해시에서 제외한다. "
              "원시 서식 해시는 진단용으로 함께 표시한다.", "",
              "| 문서 | 문단 수 전→후 | 변경 문단 | 텍스트 XML 해시 | 원시 서식 해시 | 정규화 서식 해시 |",
              "|---|---:|---:|---|---|---|"]
    for ident, diff, hashes_a, hashes_b in fidelity:
        if diff is None:
            lines.append(f"| {ident} | — | 측정 불가 | 측정 불가 | 측정 불가 |")
        else:
            counts = diff["paragraph_count"]
            text_equal = hashes_a.get("text_xml_sha256") == hashes_b.get("text_xml_sha256")
            format_equal = hashes_a.get("format_xml_sha256") == hashes_b.get("format_xml_sha256")
            raw_equal = hashes_a.get("raw_format_xml_sha256") == hashes_b.get("raw_format_xml_sha256")
            lines.append(f"| {ident} | {counts['before']}→{counts['after']} | {len(diff['changed_paragraphs'])} | "
                         f"{'일치' if text_equal else '불일치'} | {'일치' if raw_equal else '불일치'} | "
                         f"{'일치' if format_equal else '불일치'} |")
            if diff["changed_paragraphs"] or not text_equal or not format_equal:
                defects.append(f"{ident}: 결과 HWPX 차이 {len(diff['changed_paragraphs'])} 문단, 텍스트 XML 동일={text_equal}, 서식 XML 동일={format_equal}")
    lines += ["", "## 자동 기능 검수", "", "| 문서 | 항목 | 판정 |", "|---|---|---|"]
    for result_label, corpus in ((before["label"], left), (after["label"], right)):
        label_dir = before_dir if result_label == before["label"] else after_dir
        for ident, report in sorted(corpus.items()):
            result = _result_path(report, label_dir, ident)
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
