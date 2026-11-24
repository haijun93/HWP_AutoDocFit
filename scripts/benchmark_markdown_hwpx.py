"""KORDOCC2 합성 19종 생성·동일 harness 비교·선택적 한/글 COM 검증.

python scripts/benchmark_markdown_hwpx.py generate --out NEW_DIR [--reference]
python scripts/benchmark_markdown_hwpx.py com --out EXISTING_DIR

reference는 설치된 kordoc만 사용한다. COM은 기본 공용 잠금 아래 합성 생성본만
열고 별도 roundtrip 폴더에 저장한다. 산출 폴더는 저장소 밖으로 지정한다.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import time
from zipfile import ZipFile, ZIP_STORED

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from defusedxml import ElementTree as ET
from docfit_core import conversion_fidelity as cf
from docfit_core.hwpx import validate_hwpx
from docfit_core.markdown_hwpx import BODY_WIDTH, HP, HH, HC, generate_hwpx, render_hwpx
from docfit_core.markdown_hwpx_fixtures import fixtures_v2 as fixtures, write_images


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def features(path: Path) -> dict:
    """일반 harness에 없는 대체글/바이너리/줄바꿈/코드 글꼴을 별도 대조한다."""
    with ZipFile(path) as archive:
        section = ET.fromstring(archive.read('Contents/section0.xml'))
        header = ET.fromstring(archive.read('Contents/header.xml'))
        package = ET.fromstring(archive.read('Contents/content.hpf'))
        opf = '{http://www.idpf.org/2007/opf/}'
        items = {i.get('id'): i for i in package.findall(f'{opf}manifest/{opf}item')}
        pictures = []
        for pic in section.findall(f'.//{{{HP}}}pic'):
            ref = pic.find(f'{{{HC}}}img').get('binaryItemIDRef')
            item = items[ref]
            size = pic.find(f'{{{HP}}}sz')
            pictures.append({'sha256': hashlib.sha256(archive.read(item.get('href'))).hexdigest(),
                             'alt': pic.findtext(f'{{{HP}}}shapeComment', ''),
                             'width': int(size.get('width')), 'height': int(size.get('height'))})
        chars = {c.get('id'): c for c in header.findall(f'.//{{{HH}}}charPr')}
        fonts = {f.get('lang'): {n.get('id'): n.get('face') for n in f} for f in header.findall(f'.//{{{HH}}}fontface')}
        code_runs = []
        for run in section.findall(f'.//{{{HP}}}run'):
            char = chars.get(run.get('charPrIDRef'))
            if char is None:
                continue
            ref = char.find(f'{{{HH}}}fontRef')
            if ref is not None and fonts.get('HANGUL', {}).get(ref.get('hangul')) == '굴림체':
                text = ''.join(run.itertext())
                if text:
                    code_runs.append(text)
        return {'pictures': pictures, 'linebreaks': len(section.findall(f'.//{{{HP}}}lineBreak')),
                'code_runs': code_runs}


def validate_structure(path: Path) -> dict:
    result = validate_hwpx(path, structure_only=True, parse_xml=True)
    ns = {"hp": HP}
    with ZipFile(path) as archive:
        first = archive.infolist()[0]
        if first.filename != "mimetype" or first.compress_type != ZIP_STORED:
            raise ValueError("mimetype은 첫 항목·무압축이어야 합니다.")
        if archive.read("mimetype") != b"application/hwp+zip":
            raise ValueError("잘못된 mimetype")
        opf = "{http://www.idpf.org/2007/opf/}"
        package = ET.fromstring(archive.read("Contents/content.hpf"))
        if package.tag != opf + "package":
            raise ValueError("한/글 OPF 네임스페이스 불일치")
        for item in package.findall(f"{opf}manifest/{opf}item"):
            if item.get("href") not in archive.namelist():
                raise ValueError("패키지 manifest의 파일 경로 불일치")
        root = ET.fromstring(archive.read("Contents/section0.xml"))
        if root.findall(".//hp:tab", ns):
            raise ValueError("항목기호와 본문 사이에 탭이 있습니다.")
        for table in root.findall(".//hp:tbl", ns):
            rows = table.findall("hp:tr", ns)
            cols = int(table.get("colCnt"))
            if len(rows) != int(table.get("rowCnt")):
                raise ValueError("표 행 개수 불일치")
            for r, row in enumerate(rows):
                cells = row.findall("hp:tc", ns)
                if len(cells) != cols:
                    raise ValueError("표 열 개수 불일치")
                widths = []
                for c, cell in enumerate(cells):
                    addr = cell.find("hp:cellAddr", ns)
                    if (int(addr.get("rowAddr")), int(addr.get("colAddr"))) != (r, c):
                        raise ValueError("표 셀 좌표 불일치")
                    widths.append(int(cell.find("hp:cellSz", ns).get("width")))
                if min(widths) <= 0 or sum(widths) != BODY_WIDTH:
                    raise ValueError("표 너비가 본문 170mm와 다릅니다.")
    return {"valid": True, "entries": result.get("entry_count")}


def generate(out: Path, reference: bool) -> int:
    out.mkdir(parents=True, exist_ok=False)
    sources = out / "sources"
    sources.mkdir()
    write_images(sources)
    cases = fixtures()
    manifest = {"suite": "KORDOCC2-synthetic-v2", "fixtures": [],
                "assets": {p.relative_to(sources).as_posix(): sha(p) for p in sorted(sources.rglob('*')) if p.is_file()}}
    expected = {}
    for name, (source, structure) in cases.items():
        path = sources / f"{name}.md"
        path.write_text(source, encoding="utf-8", newline="\n")
        manifest["fixtures"].append({"id": name, "format": "md", "file": path.name,
                                     "sha256": sha(path), "expected": structure})
        expected[name] = {"hwpx": dict(structure, form="hwpx")}
    write_json(out / "manifest.json", manifest)
    cf.save_golden(out / "expected.json", "합성 정답", "v1", expected)
    generations = []
    candidate_hashes = []
    for repeat in range(3):
        folder = out / ("candidate" if repeat == 0 else f"candidate{repeat + 1}")
        folder.mkdir()
        hashes = {}
        for name, (source, _) in cases.items():
            target = folder / f"{name}.hwpx"
            start = time.perf_counter()
            result = generate_hwpx(source, target, image_dir=sources)
            valid = validate_structure(target)
            hashes[name] = sha(target)
            if repeat == 0:
                generations.append({"fixture": name, **valid, "warnings": result.warnings,
                                    "seconds": round(time.perf_counter() - start, 6), "sha256": hashes[name],
                                    "features": features(target)})
        candidate_hashes.append(hashes)
    candidate = cf.collect_outputs(out / "candidate")
    cf.save_golden(out / "candidate.json", "DocFit Markdown", "1", candidate)
    groups = cf.groups_from_manifest(manifest)

    def compare(label, ref, cand, ref_label, cand_label):
        rows = cf.compare_sets(ref, cand, groups=groups)
        write_json(out / f"{label}.json", rows)
        (out / f"{label}.md").write_text(cf.render_report(rows, ref_label=ref_label, cand_label=cand_label), encoding="utf-8")
        return sum(row["verdict"]["pass"] for row in rows)

    passed = compare("expected_vs_candidate", expected, candidate, "합성 정답", "DocFit Markdown")
    summary = {"fixtures": len(cases), "structure_valid": len(generations),
               "candidate_repeats_byte_identical": candidate_hashes[0] == candidate_hashes[1] == candidate_hashes[2],
               "expected_vs_candidate_pass": passed, "generation": generations}
    if reference:
        from docfit_core import kordoc_bridge as kb
        version = kb.engine_version()
        if version != kb.KORDOC_VERSION:
            raise RuntimeError(f"kordoc 버전 불일치: {version}")
        ref_runs = []
        executions = []
        for repeat in range(3):
            folder = out / ("reference" if repeat == 0 else f"reference{repeat + 1}")
            folder.mkdir()
            for name in cases:
                target = folder / f"{name}.hwpx"
                args = ["generate", str(sources / f"{name}.md"), "-o", str(target),
                        "--preset", "보고서", "--silent", "--image-dir", str(sources)]
                try:
                    done = kb.run_kordoc(args, timeout=90)
                    validate_hwpx(target, structure_only=True, parse_xml=True)
                    executions.append({"fixture": name, "repeat": repeat + 1, "returncode": done.returncode,
                                       "stdout": done.stdout, "stderr": done.stderr})
                except Exception as exc:
                    executions.append({"fixture": name, "repeat": repeat + 1, "error": str(exc)})
                print(f"kordoc {repeat + 1}/3 {name}", flush=True)
            ref_runs.append(cf.collect_outputs(folder))
        ref = ref_runs[0]
        cf.save_golden(out / "reference.json", "kordoc", version, ref)
        summary.update(kordoc_version=version, reference_repeats_structure_identical=ref_runs[0] == ref_runs[1] == ref_runs[2],
                       expected_vs_reference_pass=compare("expected_vs_reference", expected, ref, "합성 정답", f"kordoc {version}"),
                       reference_vs_candidate_pass=compare("reference_vs_candidate", ref, candidate, f"kordoc {version}", "DocFit Markdown"))
        # 실행 명령과 경로는 재현 가능한 상대 이름으로 기록한다.
        summary["reference_command"] = ["generate", "sources/<id>.md", "-o", "reference/<id>.hwpx", "--preset", "보고서", "--silent", "--image-dir", "sources"]
        write_json(out / "reference_executions.json", executions)
    write_json(out / "summary.json", summary)
    print(json.dumps({k: v for k, v in summary.items() if k != "generation"}, ensure_ascii=False, indent=2))
    # 합성 정답 대비 불합격을 성공으로 숨기지 않는다.
    return 0 if passed == len(cases) else 1


def com(out: Path) -> int:
    from docfit_core.save_guard import com_test_lock
    import pythoncom
    import win32com.client
    # 이 스크립트가 만든 합성 입력임을 바이트로 확인한다.
    manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("suite") != "KORDOCC2-synthetic-v2":
        raise ValueError("KORDOCC2 합성 묶음만 시험할 수 있습니다.")
    for name, (source, _) in fixtures().items():
        if (out / "sources" / f"{name}.md").read_text(encoding="utf-8") != source:
            raise ValueError(f"합성 원문 불일치: {name}")
        generated, _ = render_hwpx(source, image_dir=out / 'sources')
        if (out / 'candidate' / f'{name}.hwpx').read_bytes() != generated:
            raise ValueError(f"합성 생성본 불일치: {name}")
    for name, expected_sha in manifest['assets'].items():
        if sha(out / 'sources' / name) != expected_sha:
            raise ValueError(f"합성 그림 불일치: {name}")
    roundtrip = out / "roundtrip"
    roundtrip.mkdir(exist_ok=False)
    results = []
    with com_test_lock("KORDOCC2", "합성 Markdown HWPX 열기·사본 저장", timeout_sec=60):
        pythoncom.CoInitialize()
        app = None
        try:
            app = win32com.client.DispatchEx("HwpFrame.HwpObject")
            from verify_table_catalog_paste import _register
            _register(app)
            app.SetMessageBoxMode(0x00020000)
            version = str(app.Version)
            for name in fixtures():
                source = out / "candidate" / f"{name}.hwpx"
                target = roundtrip / source.name
                row = {"fixture": name, "hancom_version": version}
                try:
                    if not app.Open(str(source.resolve()), "HWPX", "forceopen:true;versionwarning:false"):
                        raise RuntimeError("한/글 Open이 False를 반환했습니다.")
                    row.update(open=True, pages=int(app.PageCount))
                    if not app.SaveAs(str(target.resolve()), "HWPX", "") or not target.is_file():
                        raise RuntimeError("한/글 SaveAs 실패")
                    row["save"] = True
                    validate_hwpx(target, structure_only=True, parse_xml=True)
                    row["fidelity"] = cf.compare(cf.extract_hwpx(source), cf.extract_hwpx(target))
                    row['features_before'] = features(source)
                    row['features_after'] = features(target)
                    row['features_preserved'] = row['features_before'] == row['features_after']
                except Exception as exc:
                    row["error"] = f"{type(exc).__name__}: {exc}"
                finally:
                    app.Clear(1)
                results.append(row)
                write_json(out / "com.json", results)
                print(json.dumps(row, ensure_ascii=False), flush=True)
        finally:
            if app is not None:
                try:
                    app.Clear(1)
                    app.Quit()
                finally:
                    pythoncom.CoUninitialize()
            else:
                pythoncom.CoUninitialize()
    return 0 if all(r.get("save") and r.get("pages", 0) > 0 and r["fidelity"]["verdict"]["pass"]
                    and r.get('features_preserved') for r in results) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("generate", "com"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--reference", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    if out.is_relative_to(ROOT):
        parser.error("산출 폴더는 저장소 밖에 지정하세요.")
    return generate(out, args.reference) if args.mode == "generate" else com(out)


if __name__ == "__main__":
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")
    raise SystemExit(main())
