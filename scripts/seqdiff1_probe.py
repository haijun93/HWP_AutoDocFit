"""Compare sequential document processing orders and isolated runs (Windows + Hancom COM).

All files are copied from the ALLQA synthetic corpus into --out. The single
COM lock is held for the complete measurement; inputs in the repository stay
unchanged.
"""
from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))
from scripts.allqa_run import read_docset, sha256  # noqa: E402
from scripts.allqa_compare import xml_hashes  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=1500)
    parser.add_argument("--ids", nargs="+", default=[
        "txt_short", "txt_long", "hwp_reference", "hwpx_continuation", "hwpx_shapes"
    ])
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Windows + 한/글 COM 환경이 필요합니다.")
    out = args.out.resolve()
    if out.exists():
        parser.error(f"결과 폴더가 이미 있습니다: {out}")
    out.mkdir(parents=True)
    rows = {r["id"]: r for r in read_docset(REPO / "docs/ALLQA_DOCSET.md")}
    ids = list(args.ids)
    sources = {ident: (REPO / rows[ident]["source"]).resolve(strict=True) for ident in ids}
    before = {ident: sha256(path) for ident, path in sources.items()}
    shuffled = ids.copy()
    random.Random(2401).shuffle(shuffled)
    orders = {"forward": ids, "reverse": list(reversed(ids)), "random_seed_2401": shuffled}
    env = dict(os.environ, PYTHONUTF8="1")
    summary: dict = {"ids": ids, "orders": orders, "runs": [], "singles": []}

    def run(label: str, chosen: list[str], single: bool = False) -> None:
        folder = out / label
        docs = folder / "docs"
        docs.mkdir(parents=True)
        copies = []
        for ident in chosen:
            target = docs / f"{ident}{sources[ident].suffix.lower()}"
            shutil.copy2(sources[ident], target)
            copies.append(target)
        report = folder / "report.json"
        cmd = [sys.executable, str(REPO / "scripts/seqdiff1_driver.py"),
               "--report", str(report), *map(str, copies)]
        cp = subprocess.run(cmd, cwd=REPO, env=env, capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=args.timeout)
        (folder / "driver.out.txt").write_text(cp.stdout + "\n" + cp.stderr, encoding="utf-8")
        rep = json.loads(report.read_text(encoding="utf-8")) if report.exists() else {}
        result_map = {}
        for ident in chosen:
            path = docs / f"{ident}(일괄적용).hwpx"
            result_map[ident] = {"path": str(path), **xml_hashes(path)} if path.is_file() else None
        row = {"label": label, "order": chosen, "returncode": cp.returncode,
               "saved": len(rep.get("saved", [])), "errors": rep.get("errors", []),
               "results": result_map}
        (summary["singles"] if single else summary["runs"]).append(row)
        (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
        print(label, "returncode", cp.returncode, "saved", row["saved"],
              "errors", len(row["errors"]), flush=True)

    from docfit_core.save_guard import com_test_lock
    with com_test_lock("seqdiff1_probe", "정순·역순·고정무작위와 단독 처리 순서 의존 비교"):
        for label, order in orders.items():
            run(label, order)
        for ident in ids:
            run(f"single_{ident}", [ident], single=True)
    summary["fixtures_unchanged"] = before == {ident: sha256(path) for ident, path in sources.items()}
    baseline = summary["singles"]
    single_hashes = {ident: row["results"][ident] for row in baseline for ident in row["results"]}
    summary["order_hash_diffs"] = [
        {"label": row["label"], "id": ident,
         "expected": single_hashes.get(ident), "actual": result}
        for row in summary["runs"]
        for ident, result in row["results"].items()
        if result is None or single_hashes.get(ident) is None
        or result["text_xml_sha256"] != single_hashes[ident]["text_xml_sha256"]
        or result["format_xml_sha256"] != single_hashes[ident]["format_xml_sha256"]
    ]
    (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print("fixtures_unchanged", summary["fixtures_unchanged"], flush=True)
    print("order_hash_diffs", len(summary["order_hash_diffs"]), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
