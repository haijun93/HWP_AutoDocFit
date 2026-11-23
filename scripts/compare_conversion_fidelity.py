#!/usr/bin/env python
"""변환 품질 비교 harness CLI (KORDOC2D 카드 B). 사용법은 docs/KORDOCB2_RESULT_2026-11-23.md 참고.

  generate       합성 fixture 와 manifest.json 생성
  run-reference  (PC, kordoc 필요) fixture 를 kordoc 으로 변환해 출력 폴더와 골든 JSON 저장
  extract        출력 폴더를 구조 JSON(골든)으로 저장
  compare        기준 대 후보 비교 → markdown 보고서(+JSON). 불합격이 있으면 종료 코드 1
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from docfit_core import conversion_fidelity as cf  # noqa: E402
from docfit_core import conversion_fixtures as fx  # noqa: E402

DEFAULT_FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "conversion_fidelity"


def cmd_generate(args) -> int:
    manifest = fx.generate_fixtures(args.out)
    print(f"fixture {len(manifest['fixtures'])}개 생성: {args.out}")
    return 0


def cmd_run_reference(args) -> int:
    from docfit_core import kordoc_bridge as kb  # 지연 import: kordoc 이 없는 환경에서도 다른 명령은 동작
    status = kb.engine_status()
    if not status.get("available"):
        print(f"kordoc 사용 불가: {status.get('reason')}", file=sys.stderr)
        return 2
    manifest = json.loads((Path(args.fixtures) / "manifest.json").read_text(encoding="utf-8"))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    skipped = []
    for entry in manifest["fixtures"]:
        src = Path(args.fixtures) / entry["file"]
        fid, fmt = entry["id"], entry["format"]
        try:
            if fmt == "html":
                skipped.append((fid, "kordoc 입력 지원 목록에 없음"))
                continue
            if fmt == "md":
                kb.generate_hwpx(src, "보고서", out / f"{fid}.hwpx", image_dir=args.fixtures)
            else:
                kb.parse_document(src, out / f"{fid}.md", output_format="markdown")
                kb.convert_to_hwpx(src, out / f"{fid}.hwpx")
        except Exception as exc:  # 실패한 fixture 는 보고서에서 '누락'으로 드러난다
            skipped.append((fid, f"{type(exc).__name__}: {exc}"))
    for fid, why in skipped:
        print(f"건너뜀 {fid}: {why}", file=sys.stderr)
    version = kb.engine_version()
    cf.save_golden(args.golden or out / "golden.json", "kordoc", version, cf.collect_outputs(out))
    print(f"kordoc {version} 기준 결과 저장: {out}")
    return 0


def cmd_extract(args) -> int:
    cf.save_golden(args.golden, args.engine, args.version, cf.collect_outputs(args.folder))
    print(f"골든 저장: {args.golden}")
    return 0


def cmd_compare(args) -> int:
    manifest_path = Path(args.manifest) if args.manifest else DEFAULT_FIXTURES / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.is_file() else None
    cand, cand_label = cf.load_side(args.candidate)
    ref, ref_label = cf.load_side(args.reference, manifest, cand)
    th = {k: v for k, v in (("recall", args.recall), ("precision", args.precision), ("structure", args.structure)) if v is not None}
    rows = cf.compare_sets(ref, cand, th, cf.groups_from_manifest(manifest) if manifest else None)
    report = cf.render_report(rows, ref_label=ref_label, cand_label=cand_label, thresholds=th)
    if args.report:
        Path(args.report).write_text(report, encoding="utf-8")
    else:
        print(report)
    if args.json:
        Path(args.json).write_text(json.dumps(rows, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0 if all(r["verdict"]["pass"] for r in rows) and rows else 1


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("generate"); g.add_argument("--out", default=str(DEFAULT_FIXTURES)); g.set_defaults(fn=cmd_generate)
    r = sub.add_parser("run-reference")
    r.add_argument("--fixtures", default=str(DEFAULT_FIXTURES)); r.add_argument("--out", required=True)
    r.add_argument("--golden"); r.set_defaults(fn=cmd_run_reference)
    e = sub.add_parser("extract")
    e.add_argument("folder"); e.add_argument("--golden", required=True)
    e.add_argument("--engine", default="candidate"); e.add_argument("--version", default=""); e.set_defaults(fn=cmd_extract)
    c = sub.add_parser("compare")
    c.add_argument("--reference", required=True, help="골든 JSON | 출력 폴더 | expected")
    c.add_argument("--candidate", required=True, help="골든 JSON | 출력 폴더")
    c.add_argument("--manifest"); c.add_argument("--report"); c.add_argument("--json")
    c.add_argument("--recall", type=float); c.add_argument("--precision", type=float); c.add_argument("--structure", type=float)
    c.set_defaults(fn=cmd_compare)
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
