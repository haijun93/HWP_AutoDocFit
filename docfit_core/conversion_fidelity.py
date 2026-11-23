"""변환 품질 비교 harness (KORDOC2D 카드 B).

기준 경로(kordoc)와 후보 경로(대체 변환)의 출력(HWPX / 마크다운 / 텍스트)에서 구조를 뽑아
제목·목록·표·그림·텍스트 지표를 계산하고 합격 여부를 판정한다. 순수 표준 라이브러리 + defusedxml.

구조(Structure) = JSON 직렬화 가능한 dict:
    {"form": "hwpx|markdown|text",
     "headings": [[level, text], ...],
     "lists":    [[depth, ordered, text], ...],     # depth 0부터
     "tables":   [[[cell, ...], ...], ...],          # 표 → 행 → 셀 텍스트
     "images":   int,
     "text":     "본문 전체(구조 요소 포함, 문서 순서)"}
텍스트 형식은 구조를 알 수 없으므로 headings/lists/tables/images 가 None 이다(해당 지표는 'n/a').
"""
from __future__ import annotations

import difflib
import json
import re
import zipfile
from collections import Counter
from pathlib import Path
from typing import Any

from defusedxml import ElementTree as ET

# 설계 4.3 잠정 합격선(사용자 승인 전)
DEFAULT_THRESHOLDS = {"structure": 1.0, "recall": 0.995, "precision": 0.99}

STRUCTURE_METRICS = ("heading", "list_text", "list_shape", "table_count", "table_shape", "table_cells", "images")
METRIC_LABELS = {
    "heading": "제목", "list_text": "목록 텍스트", "list_shape": "목록 깊이·유형",
    "table_count": "표 개수", "table_shape": "표 행×열", "table_cells": "표 셀 텍스트",
    "images": "그림 수", "recall": "재현율", "precision": "정밀도",
}
EXACT_LCS_LIMIT = 30000  # 이보다 긴 텍스트는 순서 무시 문자 빈도 비교로 대체(결과에 방식 기록)


def normalize(text: str) -> str:
    """공백·줄바꿈을 모두 제거한 비교용 문자열."""
    return re.sub(r"\s+", "", text or "")


def _inline_text(text: str) -> str:
    """마크다운 인라인 표식 제거: 강조·코드·링크·이미지 대체문."""
    text = re.sub(r"!\[([^\]]*)\]\([^)]*\)", "", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"(\*\*|__|\*|`|~~)", "", text)
    text = text.replace("\\|", "|")
    return text.strip()


# ───────────────────────── 마크다운 ─────────────────────────
_HEAD = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
_LIST = re.compile(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$")
_SEP = re.compile(r"^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$")


def _split_row(line: str) -> list[str]:
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|") and not line.endswith("\\|"):
        line = line[:-1]
    cells = re.split(r"(?<!\\)\|", line)
    return [_inline_text(c) for c in cells]


def extract_markdown(source: str) -> dict[str, Any]:
    headings, lists, tables, texts = [], [], [], []
    images = len(re.findall(r"!\[[^\]]*\]\([^)]*\)", source)) + len(re.findall(r"<img\b", source, re.I))
    lines = source.replace("\r\n", "\n").split("\n")
    indent_stack: list[int] = []
    i = 0
    in_code = False
    while i < len(lines):
        line = lines[i]
        if line.strip().startswith("```"):
            in_code = not in_code
            i += 1
            continue
        if in_code:
            if line.strip():
                texts.append(line.strip())
            i += 1
            continue
        if "|" in line and i + 1 < len(lines) and _SEP.match(lines[i + 1]) and "-" in lines[i + 1]:
            rows = [_split_row(line)]
            i += 2
            while i < len(lines) and "|" in lines[i] and lines[i].strip():
                rows.append(_split_row(lines[i]))
                i += 1
            tables.append(rows)
            texts.extend(c for r in rows for c in r if c)
            continue
        m = _HEAD.match(line)
        if m:
            text = _inline_text(m.group(2))
            headings.append([len(m.group(1)), text])
            texts.append(text)
            indent_stack = []
            i += 1
            continue
        m = _LIST.match(line)
        if m:
            width = len(m.group(1).replace("\t", "    "))
            while indent_stack and indent_stack[-1] >= width:
                indent_stack.pop()
            if not indent_stack or indent_stack[-1] < width:
                indent_stack.append(width)
            depth = len(indent_stack) - 1
            text = _inline_text(m.group(3))
            lists.append([depth, m.group(2)[0].isdigit(), text])
            texts.append(text)
            i += 1
            continue
        if line.strip():
            if not line.startswith((" ", "\t")):
                indent_stack = []
            plain = _inline_text(re.sub(r"<[^>]+>", "", line))
            if plain:
                texts.append(plain)
        else:
            indent_stack = []
        i += 1
    return {"form": "markdown", "headings": headings, "lists": lists, "tables": tables,
            "images": images, "text": "\n".join(texts)}


def extract_text(source: str) -> dict[str, Any]:
    return {"form": "text", "headings": None, "lists": None, "tables": None, "images": None, "text": source}


# ───────────────────────── HWPX ─────────────────────────
def _local(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


_STYLE_HEADING = re.compile(r"^(?:개요|제목|heading|outline)\s*(\d)$", re.I)
_MARK_BULLET = re.compile(r"^(\s*)([•·‣▪○●■□◦◆◇\-*]|[ㅇ-])\s+(.*)$")
_MARK_NUMBER = re.compile(r"^(\s*)(\d+[.)]|[가-힣][.)]|\(\d+\)|[①-⑳])\s+(.*)$")


def _paragraph_text(p) -> str:
    parts = []
    for el in p.iter():
        if _local(el.tag) == "t" and el.text:
            parts.append(el.text)
        elif _local(el.tag) in ("lineBreak", "tab"):
            parts.append(" ")
    return "".join(parts)


def _header_info(zf: zipfile.ZipFile) -> tuple[dict[str, int], dict[str, dict]]:
    """스타일 id → 제목 수준, paraPr id → {kind, level}. header.xml 이 없으면 빈 값."""
    style_level: dict[str, int] = {}
    para_pr: dict[str, dict] = {}
    for name in zf.namelist():
        if not re.fullmatch(r"Contents/header\d*\.xml", name):
            continue
        root = ET.fromstring(zf.read(name))
        for el in root.iter():
            tag = _local(el.tag)
            if tag == "style":
                for key in ("name", "engName"):
                    m = _STYLE_HEADING.match((el.get(key) or "").strip())
                    if m:
                        style_level[el.get("id", "")] = int(m.group(1))
                        break
            elif tag == "paraPr":
                info = {}
                for child in el.iter():
                    if _local(child.tag) == "heading":
                        info = {"kind": (child.get("type") or "").upper(), "level": int(child.get("level") or 0)}
                para_pr[el.get("id", "")] = info
    return style_level, para_pr


def extract_hwpx(path: str | Path) -> dict[str, Any]:
    headings, lists, tables, texts = [], [], [], []
    images = 0
    with zipfile.ZipFile(path) as zf:
        style_level, para_pr = _header_info(zf)
        sections = sorted((n for n in zf.namelist() if re.fullmatch(r"Contents/section\d+\.xml", n)),
                          key=lambda n: int(re.findall(r"\d+", n)[-1]))
        bin_count = len([n for n in zf.namelist() if n.startswith("BinData/") and not n.endswith("/")])
        for name in sections:
            root = ET.fromstring(zf.read(name))
            _walk_hwpx(root, style_level, para_pr, headings, lists, tables, texts, in_table=False)
            images += sum(1 for el in root.iter() if _local(el.tag) == "pic")
    if images == 0:
        images = bin_count  # 그림 요소 없이 BinData 만 있으면 그 개수로 대체
    return {"form": "hwpx", "headings": headings, "lists": lists, "tables": tables,
            "images": images, "text": "\n".join(texts)}


def _walk_hwpx(node, style_level, para_pr, headings, lists, tables, texts, in_table) -> None:
    for child in node:
        tag = _local(child.tag)
        if tag == "tbl" and not in_table:
            rows = []
            for tr in (c for c in child.iter() if _local(c.tag) == "tr"):
                row = []
                for tc in (c for c in tr if _local(c.tag) == "tc"):
                    cell = " ".join(t for t in (_paragraph_text(p).strip() for p in tc.iter() if _local(p.tag) == "p") if t)
                    row.append(cell)
                if row:
                    rows.append(row)
            tables.append(rows)
            texts.extend(c for r in rows for c in r if c)
        elif tag == "p" and not in_table:
            _hwpx_paragraph(child, style_level, para_pr, headings, lists, tables, texts)
        else:
            _walk_hwpx(child, style_level, para_pr, headings, lists, tables, texts, in_table or tag == "tbl")


def _hwpx_paragraph(p, style_level, para_pr, headings, lists, tables, texts) -> None:
    # 문단 안에 표가 들어 있는 경우(한/글은 표를 run 안에 둔다)를 먼저 처리
    inner_tables = [el for el in p.iter() if _local(el.tag) == "tbl"]
    if inner_tables:
        _walk_hwpx(p, style_level, para_pr, headings, lists, tables, texts, in_table=False)
        return
    raw = _paragraph_text(p).rstrip()
    text = raw.strip()
    if not text:
        return
    level = style_level.get(p.get("styleIDRef", ""))
    info = para_pr.get(p.get("paraPrIDRef", ""), {})
    if level is None and info.get("kind") == "OUTLINE":
        level = info.get("level", 0) + 1
    if level:
        headings.append([level, text])
        texts.append(text)
        return
    kind = info.get("kind")
    if kind in ("BULLET", "NUMBER"):
        lists.append([info.get("level", 0), kind == "NUMBER", text])
        texts.append(text)
        return
    for pattern, ordered in ((_MARK_NUMBER, True), (_MARK_BULLET, False)):
        m = pattern.match(raw)
        if m:
            depth = len(m.group(1).replace("\t", "    ")) // 2
            lists.append([depth, ordered, m.group(3).strip()])
            texts.append(m.group(3).strip())
            return
    texts.append(text)


# ───────────────────────── 파일 → 구조 ─────────────────────────
def extract_file(path: str | Path) -> dict[str, Any]:
    path = Path(path)
    suffix = path.suffix.lower()
    if suffix == ".hwpx":
        return extract_hwpx(path)
    if suffix in (".md", ".markdown"):
        return extract_markdown(path.read_text(encoding="utf-8-sig"))
    if suffix == ".txt":
        return extract_text(path.read_text(encoding="utf-8-sig"))
    raise ValueError(f"지원하지 않는 출력 형식: {path.suffix}")


# ───────────────────────── 지표 ─────────────────────────
def _multiset_ratio(ref: list[tuple], cand: list[tuple]) -> float | None:
    if not ref:
        return None
    hit = sum((Counter(ref) & Counter(cand)).values())
    return hit / len(ref)


def text_scores(ref_text: str, cand_text: str) -> dict[str, Any]:
    a, b = normalize(ref_text), normalize(cand_text)
    if not a and not b:
        return {"recall": 1.0, "precision": 1.0, "method": "empty"}
    if not a or not b:
        return {"recall": 0.0 if a else 1.0, "precision": 0.0 if b else 1.0, "method": "empty"}
    if max(len(a), len(b)) <= EXACT_LCS_LIMIT:
        matched = sum(blk.size for blk in difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks())
        method = "sequence"
    else:
        matched = sum((Counter(a) & Counter(b)).values())
        method = "char-multiset"
    return {"recall": matched / len(a), "precision": matched / len(b), "method": method}


def compute_metrics(ref: dict[str, Any], cand: dict[str, Any]) -> dict[str, Any]:
    """기준(ref) 대비 후보(cand) 지표. 값 None = 해당 없음(기준에 요소가 없거나 형식이 구조를 못 담음)."""
    m: dict[str, Any] = {k: None for k in STRUCTURE_METRICS}
    structural = cand.get("headings") is not None and ref.get("headings") is not None
    if structural:
        m["heading"] = _multiset_ratio([(l, normalize(t)) for l, t in ref["headings"]],
                                       [(l, normalize(t)) for l, t in cand["headings"]])
        m["list_text"] = _multiset_ratio([(normalize(t),) for _, _, t in ref["lists"]],
                                         [(normalize(t),) for _, _, t in cand["lists"]])
        m["list_shape"] = _multiset_ratio([(d, o, normalize(t)) for d, o, t in ref["lists"]],
                                          [(d, o, normalize(t)) for d, o, t in cand["lists"]])
        rt, ct = ref["tables"], cand["tables"]
        m["table_count"] = (1.0 if len(rt) == len(ct) else min(len(rt), len(ct)) / max(len(rt), len(ct))) if (rt or ct) else None
        if rt:
            shape_hit, cell_total, cell_hit = 0, 0, 0
            for idx, table in enumerate(rt):
                other = ct[idx] if idx < len(ct) else None
                shape = (len(table), max((len(r) for r in table), default=0))
                if other is not None and shape == (len(other), max((len(r) for r in other), default=0)):
                    shape_hit += 1
                for r, row in enumerate(table):
                    for c, cell in enumerate(row):
                        cell_total += 1
                        if other is not None and r < len(other) and c < len(other[r]) \
                                and normalize(other[r][c]) == normalize(cell):
                            cell_hit += 1
            m["table_shape"] = shape_hit / len(rt)
            m["table_cells"] = cell_hit / cell_total if cell_total else 1.0
        elif ct:  # 기준엔 표가 없는데 후보에 생김
            m["table_shape"] = m["table_cells"] = 0.0
        ri, ci = ref["images"] or 0, cand["images"] or 0
        m["images"] = None if ri == 0 and ci == 0 else (1.0 if ri == ci else min(ri, ci) / max(ri, ci))
    ts = text_scores(ref["text"], cand["text"])
    m["recall"], m["precision"], m["text_method"] = ts["recall"], ts["precision"], ts["method"]
    return m


def judge(metrics: dict[str, Any], thresholds: dict[str, float] | None = None) -> dict[str, Any]:
    """지표별 합격 여부 {metric: True/False/None(n/a)} 와 전체 합격."""
    th = {**DEFAULT_THRESHOLDS, **(thresholds or {})}
    verdict: dict[str, Any] = {}
    for key in STRUCTURE_METRICS:
        v = metrics.get(key)
        verdict[key] = None if v is None else v >= th["structure"] - 1e-12
    verdict["recall"] = metrics["recall"] >= th["recall"]
    verdict["precision"] = metrics["precision"] >= th["precision"]
    verdict["pass"] = all(v is not False for k, v in verdict.items())
    return verdict


def compare(ref: dict[str, Any], cand: dict[str, Any], thresholds: dict[str, float] | None = None) -> dict[str, Any]:
    metrics = compute_metrics(ref, cand)
    return {"metrics": metrics, "verdict": judge(metrics, thresholds),
            "ref_form": ref["form"], "cand_form": cand["form"]}


# ───────────────────────── 골든 JSON ─────────────────────────
def save_golden(path: str | Path, engine: str, version: str, results: dict[str, dict[str, dict]]) -> None:
    """results = {fixture_id: {form: structure}}; form 은 hwpx/markdown/text."""
    Path(path).write_text(json.dumps({"engine": engine, "version": version, "results": results},
                                     ensure_ascii=False, indent=1), encoding="utf-8")


def load_golden(path: str | Path) -> dict[str, Any]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if "results" not in data:
        raise ValueError("골든 JSON 에 'results' 가 없습니다.")
    return data


def collect_outputs(folder: str | Path) -> dict[str, dict[str, dict]]:
    """<folder>/<fixture_id>.<hwpx|md|txt> 를 구조로 읽어 {fixture_id: {form: structure}}."""
    out: dict[str, dict[str, dict]] = {}
    for f in sorted(Path(folder).iterdir()):
        if f.suffix.lower() in (".hwpx", ".md", ".txt"):
            s = extract_file(f)
            out.setdefault(f.stem, {})[s["form"]] = s
    return out


def compare_sets(ref: dict[str, dict[str, dict]], cand: dict[str, dict[str, dict]],
                 thresholds: dict[str, float] | None = None,
                 groups: dict[str, str] | None = None) -> list[dict[str, Any]]:
    """같은 fixture·같은 출력 형식끼리 비교. 후보에 빠진 항목은 누락(불합격)으로 기록한다."""
    rows = []
    for fid in sorted(ref):
        for form, rs in sorted(ref[fid].items()):
            cs = cand.get(fid, {}).get(form)
            row = {"fixture": fid, "group": (groups or {}).get(fid, ""), "form": form}
            if cs is None:
                row.update(missing=True, metrics=None, verdict={"pass": False})
            else:
                row.update(missing=False, **compare(rs, cs, thresholds))
            rows.append(row)
    return rows


# ───────────────────────── 보고서 ─────────────────────────
def _cell(row: dict, key: str) -> str:
    if row.get("missing"):
        return "누락"
    v = row["metrics"].get(key)
    ok = row["verdict"].get(key)
    if v is None or ok is None:
        return "n/a"
    return f"{v * 100:.2f}% {'✅' if ok else '❌'}"


def render_report(rows: list[dict[str, Any]], *, ref_label: str, cand_label: str,
                  thresholds: dict[str, float] | None = None) -> str:
    th = {**DEFAULT_THRESHOLDS, **(thresholds or {})}
    keys = list(STRUCTURE_METRICS) + ["recall", "precision"]
    lines = ["# 변환 품질 비교 보고서", "",
             f"- 기준 경로: {ref_label}", f"- 후보 경로: {cand_label}",
             f"- 합격선(잠정): 구조 지표 {th['structure']*100:.0f}%, 재현율 {th['recall']*100:.1f}%, 정밀도 {th['precision']*100:.1f}%",
             "- n/a: 기준에 해당 요소가 없거나 출력 형식(텍스트)이 구조를 담지 못해 판정하지 않음. 평균으로 상쇄하지 않고 항목별로 판정.", "",
             "## 형식·출력별 요약", "", "| 입력 형식 | 출력 | 합격/전체 | 불합격 fixture |", "|---|---|---|---|"]
    combos: dict[tuple[str, str], list[dict]] = {}
    for r in rows:
        combos.setdefault((r["group"] or "-", r["form"]), []).append(r)
    for (group, form), items in sorted(combos.items()):
        ok = sum(1 for r in items if r["verdict"]["pass"])
        bad = ", ".join(r["fixture"] for r in items if not r["verdict"]["pass"]) or "-"
        lines.append(f"| {group} | {form} | {ok}/{len(items)} | {bad} |")
    lines += ["", "## fixture별 지표", "",
              "| fixture | 입력 | 출력 | " + " | ".join(METRIC_LABELS[k] for k in keys) + " | 판정 |",
              "|---|---|---|" + "---|" * (len(keys) + 1)]
    for r in rows:
        cells = " | ".join(_cell(r, k) for k in keys)
        lines.append(f"| {r['fixture']} | {r['group'] or '-'} | {r['form']} | {cells} | "
                     f"{'합격' if r['verdict']['pass'] else '**불합격**'} |")
    total = len(rows)
    passed = sum(1 for r in rows if r["verdict"]["pass"])
    lines += ["", f"**전체: {passed}/{total} 합격**" + ("" if passed == total else " — 하나라도 불합격이면 해당 형식은 전환 보류(설계 7절)."), ""]
    return "\n".join(lines)


def reference_from_manifest(manifest: dict[str, Any], cand: dict[str, dict[str, dict]]) -> dict[str, dict[str, dict]]:
    """기준 = 합성 fixture 의 기대 구조(정답). 후보가 가진 출력 형식마다 같은 형식의 기준을 만든다."""
    ref: dict[str, dict[str, dict]] = {}
    for entry in manifest["fixtures"]:
        fid = entry["id"]
        for form in cand.get(fid, {}):
            exp = dict(entry["expected"], form=form)
            if form == "text":
                exp.update(headings=None, lists=None, tables=None, images=None)
            ref.setdefault(fid, {})[form] = exp
    return ref


def groups_from_manifest(manifest: dict[str, Any]) -> dict[str, str]:
    return {e["id"]: e["format"] for e in manifest["fixtures"]}


def load_side(spec: str | Path, manifest: dict[str, Any] | None = None,
              cand: dict[str, dict[str, dict]] | None = None) -> tuple[dict[str, dict[str, dict]], str]:
    """비교 한쪽을 읽는다. spec: 골든 JSON 파일 | 출력 폴더 | 'expected'(기준 전용, manifest 필요)."""
    if str(spec) == "expected":
        if manifest is None or cand is None:
            raise ValueError("expected 기준은 manifest 와 후보가 필요합니다.")
        return reference_from_manifest(manifest, cand), "합성 fixture 기대 구조(정답)"
    path = Path(spec)
    if path.is_dir():
        return collect_outputs(path), f"출력 폴더 {path.name}"
    golden = load_golden(path)
    return golden["results"], f"골든 {golden.get('engine', '?')} {golden.get('version', '')}".strip()
