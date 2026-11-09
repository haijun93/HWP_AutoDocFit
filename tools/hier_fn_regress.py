#!/usr/bin/env python3
"""style_hierarchy의 함수별 Python/Rust 일치 회귀 도구."""
from __future__ import annotations
import argparse, json, os, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from docfit_core import style_hierarchy as py
from tools.hier_regress import source_corpus, normalize_paragraphs
from tools.hier_py_stages import run as python_stages

ORDER = (
    "canonical_marker",
    "normalize_leading_dot",
    "leading_marker",
    "display_role",
    "stored_role",
    "most_common",
    "_most_common",
    "document_type",
    "analyze_hierarchy",
    "hierarchy_summary",
)
ROLES = (
    "제목",
    "장",
    "중제목",
    "소제목",
    "본문",
    "내용",
    "부연설명",
    "1단계",
    "2단계",
    "3단계",
    "4단계",
    "5단계",
    "6단계",
    "미분류",
    "기타",
    "",
    "알수없음",
)


def _json_normalize(value):
    if isinstance(value, dict):
        return {key: _json_normalize(item) for key, item in sorted(value.items())}
    if isinstance(value, (list, tuple)):
        return [_json_normalize(item) for item in value]
    return value


def _freeze(v):
    if isinstance(v, list):
        return ("__list__", tuple(_freeze(x) for x in v))
    if isinstance(v, dict):
        return ("__dict__", tuple(sorted((k, _freeze(val)) for k, val in v.items())))
    return v


def _unfreeze(v):
    if isinstance(v, tuple) and len(v) == 2:
        tag, val = v
        if tag == "__list__":
            return [_unfreeze(x) for x in val]
        if tag == "__dict__":
            return {k: _unfreeze(item) for k, item in val}
    return v


def _stage_differences(left, right, path="$", out=None):
    """Return leaf paths whose JSON values differ."""
    if out is None:
        out = []
    if isinstance(left, dict) and isinstance(right, dict):
        for key in sorted(set(left) | set(right)):
            if key not in left or key not in right:
                out.append((f"{path}.{key}", left.get(key, "<missing>"), right.get(key, "<missing>")))
            else:
                _stage_differences(left[key], right[key], f"{path}.{key}", out)
    elif isinstance(left, list) and isinstance(right, list):
        if len(left) != len(right):
            out.append((f"{path}.length", len(left), len(right)))
        for index, (a, b) in enumerate(zip(left, right)):
            _stage_differences(a, b, f"{path}[{index}]", out)
    elif left != right:
        out.append((path, left, right))
    return out


def cases(name, paragraphs):
    if name == "canonical_marker":
        return [{"marker": x} for x in ("•", "·", "‧", "∙", "⋅", "ㆍ", "●", "□", "ㅁ", "■", "ㅇ", "x", "")]
    if name == "normalize_leading_dot":
        return [{"text": p["text"]} for p in paragraphs] + [{"text": x} for x in ("  • foo", "  ·", "a·b", "·x", "", None)]
    if name == "leading_marker":
        return [{"text": p["text"]} for p in paragraphs]
    if name == "display_role":
        return [{"role": x} for x in ROLES]
    if name == "stored_role":
        return [{"label": x} for x in ROLES]
    if name in ("most_common", "_most_common"):
        return [
            {"values": x, "default": d}
            for x, d in [
                ([], None),
                ([], "기본값"),
                ([1, 1, 2], None),
                (["a", "b"], None),
                (["b", "a"], None),
                ([None, None, 1], 7),
                ([[1], [2], [1]], None),
                (["장", "중제목", "중제목", "소제목"], "본문"),
                ([10.5, 10.5, 12.0], None),
                ([{"font": "함초롬바탕", "size": 10}, {"font": "함초롬바탕", "size": 10}], None),
            ]
        ]
    if name == "document_type":
        # Exhaust the roles observed in each source fixture (including the
        # empty set), then check both table-backed and paragraph midtitles.
        observed = {tuple(sorted(set()))}
        for role in ROLES:
            observed.add((role,))
        for start in range(0, len(paragraphs), 25):
            roles = sorted({str(p.get("role", "")) for p in paragraphs[start : start + 25]})
            observed.add(tuple(roles))
        return [
            {"roles": list(r), "has_midtitle_table": b}
            for r in sorted(observed)
            for b in (False, True)
        ]
    if name == "analyze_hierarchy":
        corpus = [paragraphs[i : i + 25] for i in range(0, len(paragraphs), 25)]
        return [{"paragraphs": x} for x in corpus]
    if name == "hierarchy_summary":
        return [{"analysis": py.analyze_hierarchy(paragraphs)}]
    return []


def python_value(name, c):
    if name == "canonical_marker":
        return py.canonical_marker(c["marker"])
    if name == "normalize_leading_dot":
        return py.normalize_leading_dot(c["text"])
    if name == "leading_marker":
        return list(py.leading_marker(c["text"]))
    if name == "display_role":
        return py.display_role(c["role"])
    if name == "stored_role":
        return py.stored_role(c["label"])
    if name in ("most_common", "_most_common"):
        from collections import Counter

        frozen = [_freeze(x) for x in c["values"]]
        counter = Counter(frozen)
        res = py._most_common(counter, _freeze(c["default"]))
        return _unfreeze(res)
    if name == "document_type":
        return py.document_type(c["roles"], c["has_midtitle_table"])
    if name == "analyze_hierarchy":
        return py.analyze_hierarchy(c["paragraphs"])
    return py.hierarchy_summary(c["analysis"])


def run(exe, name, c):
    # Rust CLI fn accepts either canonical name or alias
    fn_name = "most_common" if name == "_most_common" else name
    p = subprocess.run(
        [str(exe), "fn", fn_name],
        input=json.dumps(c, ensure_ascii=False, allow_nan=False),
        text=True,
        encoding="utf-8",
        capture_output=True,
    )
    if p.returncode:
        raise RuntimeError(p.stderr)
    return json.loads(p.stdout)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("func_name", nargs="?", choices=ORDER, help="Function name to test")
    ap.add_argument("--function", choices=ORDER)
    ap.add_argument("--executable", type=Path)
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--stages", action="store_true", help="Compare analyze_hierarchy stage intermediates")
    ap.add_argument("--stage", choices=[f"stage_{i:02}" for i in range(1, 7)], help="Limit stage comparison")
    ap.add_argument("--dump", action="store_true", help="Print differing stage leaf paths and values")
    ap.add_argument("--report", type=Path, default=REPO.parent / "reports" / "HIER6.final.md")
    args = ap.parse_args()
    target_func = args.func_name or args.function

    exe = args.executable or REPO / "hierarchy" / "target" / "debug" / ("hierarchy.exe" if os.name == "nt" else "hierarchy")
    if not args.no_build:
        env = os.environ.copy()
        env["PATH"] = (
            str(Path(os.environ.get("LOCALAPPDATA", "")) / "DocFitTools" / "mingw64" / "bin")
            + os.pathsep
            + str(Path.home() / ".cargo" / "bin")
            + os.pathsep
            + env.get("PATH", "")
        )
        subprocess.run(["cargo", "build", "--manifest-path", str(REPO / "hierarchy" / "Cargo.toml")], cwd=REPO, env=env, check=True)
    corpus = source_corpus()
    all_paras = normalize_paragraphs([p for _, ps, _ in corpus for p in ps])
    assert len(all_paras) == 549, len(all_paras)
    rows = []
    if args.stages:
        stage_rows = []
        batches = [("549 paragraphs", all_paras)] if args.stage else [("549 paragraphs", all_paras)] + [(name, ps) for name, ps, _ in corpus]
        for label, selected in batches:
            for offset in range(0, len(selected), 25):
                chunk = selected[offset:offset + 25]
                chunk_label = f"{label}[{offset}:{offset + len(chunk)}]"
                stages, py_result = python_stages(chunk)
                if py.analyze_hierarchy(chunk) != py_result:
                    raise AssertionError(f"Python staged copy differs from original for {chunk_label}")
                proc = subprocess.run(
                    [str(exe), "fn", "analyze_hierarchy_stages"],
                    input=json.dumps({"paragraphs": chunk}, ensure_ascii=False, allow_nan=False),
                    text=True, encoding="utf-8", capture_output=True,
                )
                if proc.returncode:
                    raise RuntimeError(proc.stderr)
                rust_stages = json.loads(proc.stdout)
                rust_stage_values = rust_stages.get("stages", rust_stages)
                if "stage_01" in rust_stage_values and isinstance(rust_stage_values["stage_01"], dict):
                    rust_stage_values = {**rust_stage_values, **rust_stage_values["stage_01"]}
                for stage_name, py_value in stages.items():
                    if args.stage and stage_name != args.stage:
                        continue
                    rust_value = rust_stage_values.get(stage_name)
                    diff = _json_normalize(py_value) != _json_normalize(rust_value)
                    stage_rows.append((label, stage_name, diff))
                    if diff:
                        print(f"{chunk_label}: {stage_name} difference=1")
                    if diff and args.dump:
                        for path, py_item, rust_item in _stage_differences(py_value, rust_value):
                            print(f"  {path}: python={json.dumps(py_item, ensure_ascii=False)} rust={json.dumps(rust_item, ensure_ascii=False)}")
            print(f"{label}: batches={(len(selected)+24)//25}")
        print(f"stage comparisons={len(stage_rows)} differences={sum(row[2] for row in stage_rows)}")
        return int(any(row[2] for row in stage_rows))
    # If no target function given, run canonical list (excluding _most_common alias duplicate in full run)
    run_list = (target_func,) if target_func else [f for f in ORDER if f != "_most_common"]
    for name in run_list:
        cs = cases(name, all_paras)
        diffs = []
        for i, c in enumerate(cs):
            expected = python_value(name, c)
            actual = run(exe, name, c)
            if expected != actual:
                diffs.append((i, c, expected, actual))
        rows.append((name, len(cs), len(diffs), diffs[:3]))
        print(f"{name}: cases={len(cs)} differences={len(diffs)}")
        if diffs:
            for i, c, e, a in diffs[:3]:
                print(f"  case {i}: input={str(c)[:180]}\n    python={str(e)[:300]}\n    rust={str(a)[:300]}")
            break
    lines = [
        "# HIER6 함수별 이식 결과",
        "",
        "기준: `docfit_core/style_hierarchy.py`; 비교 자료: fixture 502문단 + 골든 47문단 = 549문단.",
        "",
        "| 함수 | 비교 사례 | 차이 | 상태 |",
        "|---|---:|---:|---|",
    ]
    for n, total, diff, _ in rows:
        lines.append(f"| `{n}` | {total} | {diff} | {'일치' if diff == 0 else '진행 필요'} |")
    lines.extend(["", "전체 bridge 경로 측정 결과는 `hier_regress.py` 실행 후 별도로 기록합니다."])
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return int(any(r[2] for r in rows))


if __name__ == "__main__":
    raise SystemExit(main())
