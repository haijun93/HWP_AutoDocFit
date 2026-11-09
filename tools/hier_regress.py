#!/usr/bin/env python3
"""Compare the legacy Python hierarchy analysis with the Rust engine."""

from __future__ import annotations

import argparse
import csv
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import zipfile

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))

from docfit_core.style_hierarchy import analyze_hierarchy, leading_marker  # noqa: E402

ROLE_LEVEL = {"제목": 0, "장": 1, "중제목": 2, "소제목": 3, "본문": 4, "내용": 5,
              "부연설명": 6, "미분류": 7}
ENGINE_ROLE = {0: "제목", 1: "장", 2: "중제목", 3: "소제목", 4: "소제목", 5: "본문",
               6: "내용", 7: "부연설명", 8: "미분류", 9: "미분류", 10: "미분류"}
ENGINE_LEVEL = {0: 0, 1: 1, 2: 2, 3: 3, 4: 3, 5: 4, 6: 5, 7: 6, 8: 7, 9: 7, 10: 7}


def decode_text(raw: bytes) -> str:
    for encoding in ("utf-8-sig", "utf-16", "cp949"):
        try:
            return raw.decode(encoding)
        except UnicodeDecodeError:
            pass
    return raw.decode("utf-8", errors="replace")


def load_txt(path: Path) -> list[dict]:
    text = decode_text(path.read_bytes()).replace("\r\n", "\n").replace("\r", "\n")
    return [{"text": line, "left": 0, "indent": 0, "font": None, "size_pt": 0}
            for line in text.split("\n")]


def load_hwpx(path: Path) -> list[dict]:
    from defusedxml import ElementTree as ET

    paragraphs = []
    with zipfile.ZipFile(path) as archive:
        names = sorted((n for n in archive.namelist()
                        if re.fullmatch(r"Contents/section\d+\.xml", n)),
                       key=lambda n: int(re.search(r"section(\d+)", n).group(1)))
        for name in names:
            root = ET.fromstring(archive.read(name))
            for node in root.iter():
                if node.tag.rsplit("}", 1)[-1] != "p":
                    continue
                parts = []
                for child in node.iter():
                    kind = child.tag.rsplit("}", 1)[-1]
                    if kind == "t":
                        parts.append(child.text or "")
                    elif kind in {"tab", "fwSpace"}:
                        parts.append("\t" if kind == "tab" else " ")
                    elif kind == "lineBreak":
                        parts.append("\n")
                text = "".join(parts)
                paragraphs.append({"text": text, "left": 0, "indent": 0,
                                   "font": None, "size_pt": 0})
    return paragraphs


def source_corpus() -> list[tuple[str, list[dict]]]:
    corpus = []
    golden = json.loads((REPO / "hierarchy" / "tests" / "golden.json").read_text(encoding="utf-8"))
    golden_paras = [{"text": case["text"], "left": 0, "indent": 0, "font": None,
                     "size_pt": 0} for case in golden["cases"]]
    corpus.append(("hierarchy/tests/golden.json", golden_paras, None))
    for path in sorted((REPO / "hierarchy").rglob("*")):
        if path.is_file() and path.suffix.lower() in {".hwpx", ".txt"} and "target" not in path.parts:
            loader = load_hwpx if path.suffix.lower() == ".hwpx" else load_txt
            corpus.append((path.relative_to(REPO).as_posix(), loader(path), None))
    for path in sorted((REPO / "tests" / "fixtures").rglob("*")):
        if path.is_file() and path.suffix.lower() in {".hwpx", ".txt"}:
            loader = load_hwpx if path.suffix.lower() == ".hwpx" else load_txt
            corpus.append((path.relative_to(REPO).as_posix(), loader(path), None))
    return corpus


def normalize_paragraphs(paragraphs: list[dict]) -> list[dict]:
    return [{"text": str(p.get("text", "")),
             "left": int(p.get("left", p.get("indent", 0)) or 0),
             "indent": int(p.get("indent", 0) or 0),
             "font": p.get("font"), "size_pt": float(p.get("size_pt", p.get("font_size", 0)) or 0)}
            for p in paragraphs]


def python_rows(paragraphs: list[dict]) -> list[dict]:
    result = analyze_hierarchy(paragraphs)
    rows = []
    for item, role in zip(paragraphs, result["role_sequence"]):
        marker, _ = leading_marker(item["text"])
        rows.append({"level": ROLE_LEVEL.get(role, 7), "marker": marker,
                     "classification": role})
    return rows


def rust_rows(executable: Path, paragraphs: list[dict]) -> list[dict]:
    payload = {"paragraphs": [{"text": p["text"],
                               "indent": p["left"] + p["indent"],
                               "font_size": p["size_pt"]} for p in paragraphs]}
    proc = subprocess.run([str(executable)], input=json.dumps(payload, ensure_ascii=False),
                          text=True, encoding="utf-8", capture_output=True, check=True)
    result = json.loads(proc.stdout)
    if len(result.get("levels", [])) != len(paragraphs):
        raise RuntimeError(f"Rust output length mismatch: {len(result.get('levels', []))} != {len(paragraphs)}")
    return [{"level": ENGINE_LEVEL[int(level)], "marker": result["markers"][i],
             "classification": ENGINE_ROLE[int(level)]}
            for i, level in enumerate(result["levels"])]


def compare(corpus, executable):
    differences = []
    docs = 0
    paragraph_count = 0
    for source, original, _golden_expected in corpus:
        paragraphs = normalize_paragraphs(original)
        if not paragraphs:
            continue
        docs += 1
        py_rows = python_rows(paragraphs)
        rust = rust_rows(executable, paragraphs)
        paragraph_count += len(paragraphs)
        for index, (item, py, rs) in enumerate(zip(paragraphs, py_rows, rust), 1):
            fields = [name for name in ("level", "marker", "classification") if py[name] != rs[name]]
            if fields:
                differences.append({"source": source, "paragraph": index, "text": item["text"],
                                    "python_level": py["level"], "rust_level": rs["level"],
                                    "python_marker": py["marker"], "rust_marker": rs["marker"],
                                    "python_classification": py["classification"],
                                    "rust_classification": rs["classification"],
                                    "different_fields": ",".join(fields)})
    return docs, paragraph_count, differences


def build_release() -> Path:
    cargo = os.environ.get("CARGO", "cargo")
    subprocess.run([cargo, "build", "--release", "--manifest-path",
                    str(REPO / "hierarchy" / "Cargo.toml")], check=True, cwd=REPO)
    executable = REPO / "hierarchy" / "target" / "release" / ("hierarchy.exe" if os.name == "nt" else "hierarchy")
    if not executable.is_file():
        raise FileNotFoundError(executable)
    return executable


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--csv", type=Path, default=REPO / "reports" / "hier_regress.csv")
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--measure", type=int, default=0,
                        help="measure Python/Rust one-pass classification for N fixture files")
    args = parser.parse_args()
    executable = args.executable or (REPO / "hierarchy" / "target" / "release" /
                                     ("hierarchy.exe" if os.name == "nt" else "hierarchy"))
    if not args.no_build:
        executable = build_release()
    corpus = source_corpus()
    started = time.perf_counter()
    docs, paragraphs, differences = compare(corpus, executable)
    elapsed = time.perf_counter() - started
    args.csv.parent.mkdir(parents=True, exist_ok=True)
    with args.csv.open("w", encoding="utf-8-sig", newline="") as stream:
        fields = ["source", "paragraph", "text", "python_level", "rust_level", "python_marker",
                  "rust_marker", "python_classification", "rust_classification", "different_fields"]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(differences)
    print(f"documents={docs} paragraphs={paragraphs} differences={len(differences)} elapsed_seconds={elapsed:.3f}")
    print(f"csv={args.csv}")
    if args.measure:
        fixtures = [(name, ps) for name, ps, _ in corpus if name.startswith("tests/fixtures/")][:args.measure]
        samples = []
        for name, raw in fixtures:
            ps = normalize_paragraphs(raw)
            t0 = time.perf_counter(); python_rows(ps); py_elapsed = time.perf_counter() - t0
            t0 = time.perf_counter(); rust_rows(executable, ps); rust_elapsed = time.perf_counter() - t0
            samples.append((name, len(ps), py_elapsed, rust_elapsed))
        print(f"measurement_documents={len(samples)} python_seconds={sum(x[2] for x in samples):.6f} "
              f"rust_seconds={sum(x[3] for x in samples):.6f}")
        for name, count, py_elapsed, rust_elapsed in samples:
            print(f"measure {name} paragraphs={count} python_seconds={py_elapsed:.6f} rust_seconds={rust_elapsed:.6f}")
    return 1 if differences else 0


if __name__ == "__main__":
    raise SystemExit(main())
