"""Python 기준 변환과 K에이전트 Rust 변환 출력을 바이트 단위로 대조한다."""
from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIX = ROOT / "tests" / "fixtures"
PYTHON = ROOT / ".venv" / "Scripts" / "python.exe"


def samples():
    items = [(p.name, p.read_bytes()) for p in sorted(FIX.glob("*.cf_html"))]
    cases = {
        "synthetic_shading": "<table><col width='72'><tr><td style='background:#abc'>x</td></tr></table>",
        "synthetic_numeric_align": "<table><col width='72'><tr><td align='left'> 88,180 </td></tr></table>",
        "synthetic_merge": "<table><col width='40'><col width='60'><tr><td colspan='2'>x</td></tr></table>",
        "synthetic_padding": "<style>td {padding:1px;mso-ignore:padding}</style><table><col width='72'><tr><td>x</td></tr></table>",
        "synthetic_blank_row": "<table><col width='40'><tr><td> </td></tr><tr><td>x</td></tr></table>",
    }
    for name, body in cases.items():
        items.append((name, ("<html xmlns:x='urn:schemas-microsoft-com:office:excel'>" + body + "</html>").encode()))
    return items


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--shading", choices=["keep", "strip"], default="keep")
    ap.add_argument("--python", type=Path, default=PYTHON)
    args = ap.parse_args()
    rust = ROOT / "kagent" / "target" / "debug" / "xl_regress.exe"
    if os.name != "nt":
        rust = ROOT / "kagent" / "target" / "debug" / "xl_regress"
    # File transport avoids command line size limits on Windows.
    payload = [{"name": n, "input": base64.b64encode(b).decode(), "shading": args.shading} for n, b in samples()]
    with tempfile.TemporaryDirectory() as tmp:
        inp, outp = Path(tmp) / "input.json", Path(tmp) / "output.json"
        inp.write_text(json.dumps(payload), encoding="utf-8")
        with outp.open("wb") as out:
            subprocess.run([str(rust), str(inp)], stdout=out, stderr=subprocess.PIPE, check=True, cwd=ROOT)
        rust_bytes = outp.read_bytes()
    actual = {x["name"]: base64.b64decode(x["output"]) for x in json.loads(rust_bytes)}
    # Python reference executed in the requested application environment.
    script = "import sys,json,base64; from docfit_core.excel_paste_target import to_target_html; d=json.load(sys.stdin); print(json.dumps([{'name':x['name'],'output':base64.b64encode(to_target_html(base64.b64decode(x['input']),shading=sys.argv[1])).decode()} for x in d]))"
    py = subprocess.run([str(args.python), "-c", script, args.shading], input=json.dumps(payload).encode(), stdout=subprocess.PIPE, cwd=ROOT, check=True)
    expected = {x["name"]: base64.b64decode(x["output"]) for x in json.loads(py.stdout)}
    diffs = [n for n in expected if expected[n] != actual.get(n)]
    details = []
    for name in diffs:
        left, right = expected[name], actual.get(name, b"")
        first = next((i for i, (a, b) in enumerate(zip(left, right)) if a != b), min(len(left), len(right)))
        details.append({"name": name, "python_bytes": len(left), "rust_bytes": len(right), "first_difference": first})
    print(json.dumps({"examples": len(expected), "differences": diffs, "count": len(diffs), "details": details}, ensure_ascii=False, indent=2))
    return bool(diffs)


if __name__ == "__main__":
    raise SystemExit(main())

