"""Isolate immediately preceding documents suspected of affecting later output."""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))
from scripts.allqa_run import read_docset, sha256  # noqa: E402
from scripts.allqa_compare import xml_hashes  # noqa: E402

PAIRS = [
    ("txt_short", "hwp_reference"),
    ("txt_long", "hwp_reference"),
    ("hwpx_continuation", "hwp_reference"),
    ("hwp_reference", "hwpx_continuation"),
    ("hwpx_shapes", "hwpx_continuation"),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--close-after-each", action="store_true")
    parser.add_argument("--restart-after-each", action="store_true")
    parser.add_argument("--only", help="run only the named pair label")
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Windows + 한/글 COM 환경이 필요합니다.")
    out = args.out.resolve()
    if out.exists():
        parser.error(f"결과 폴더가 이미 있습니다: {out}")
    out.mkdir(parents=True)
    rows = {row["id"]: row for row in read_docset(REPO / "docs/ALLQA_DOCSET.md")}
    sources = {ident: (REPO / rows[ident]["source"]).resolve(strict=True)
               for pair in PAIRS for ident in pair}
    before = {ident: sha256(path) for ident, path in sources.items()}
    summary: dict = {"pairs": [], "fixtures_unchanged": None}
    env = dict(os.environ, PYTHONUTF8="1")
    from docfit_core.save_guard import com_test_lock
    with com_test_lock("seqdiff1_pairs", "문서쌍으로 선행 문서의 직접 영향 분리"):
        for first, second in PAIRS:
            label = f"{first}_then_{second}"
            if args.only and label != args.only:
                continue
            folder = out / label
            docs = folder / "docs"
            docs.mkdir(parents=True)
            copies = []
            for ident in (first, second):
                target = docs / f"{ident}{sources[ident].suffix.lower()}"
                shutil.copy2(sources[ident], target)
                copies.append(target)
            report = folder / "report.json"
            cmd = [sys.executable, str(REPO / "scripts/seqdiff1_driver.py"),
                   *( ["--close-after-each"] if args.close_after_each else [] ),
                   *( ["--restart-after-each"] if args.restart_after_each else [] ),
                   "--report", str(report), *map(str, copies)]
            cp = subprocess.run(cmd, cwd=REPO, env=env, capture_output=True, text=True,
                                encoding="utf-8", errors="replace", timeout=args.timeout)
            (folder / "driver.out.txt").write_text(cp.stdout + "\n" + cp.stderr, encoding="utf-8")
            rep = json.loads(report.read_text(encoding="utf-8")) if report.exists() else {}
            results = {}
            for ident in (first, second):
                path = next(docs.glob(f"{ident}*(일괄적용).hwpx"), None)
                results[ident] = {"path": str(path), **xml_hashes(path)} if path else None
            summary["pairs"].append({"label": label, "order": [first, second],
                                     "returncode": cp.returncode,
                                     "saved": len(rep.get("saved", [])),
                                     "errors": rep.get("errors", []), "results": results})
            (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2),
                                               encoding="utf-8")
            print(label, "returncode", cp.returncode, "saved", len(rep.get("saved", [])),
                  "errors", len(rep.get("errors", [])), flush=True)
    summary["fixtures_unchanged"] = before == {ident: sha256(path) for ident, path in sources.items()}
    (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print("fixtures_unchanged", summary["fixtures_unchanged"], flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
