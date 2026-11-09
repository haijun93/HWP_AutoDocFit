"""Compare selected style_inventory helpers against the Rust C ABI."""
from __future__ import annotations

import argparse
import ctypes
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from docfit_core import style_inventory


def _tag_value(element):
    return element.tag


def _tree(element):
    return {
        "tag": element.tag,
        "text": element.text,
        "tail": element.tail,
        "attrib": dict(element.attrib),
        "children": [_tree(child) for child in element],
    }


def collect(fixtures: Path, fn_name: str):
    cases = []
    for path in sorted(fixtures.rglob("*.hwpx")):
        from zipfile import ZipFile
        from defusedxml import ElementTree as ET

        with ZipFile(path) as archive:
            for member in sorted(n for n in archive.namelist() if n.startswith("Contents/") and n.endswith(".xml")):
                try:
                    root = ET.fromstring(archive.read(member))
                except Exception as exc:
                    print(f"skip {path.name}:{member}: {exc}", file=sys.stderr)
                    continue
                if fn_name == "_tag":
                    for element in root.iter():
                        cases.append({"file": path.name, "member": member,
                                      "input": {"tag": _tag_value(element)},
                                      "expected": style_inventory._tag(element)})
                elif fn_name == "_first":
                    names = sorted({style_inventory._tag(e) for e in root.iter()})
                    serialized = _tree(root)
                    for name in names:
                        found = style_inventory._first(root, name)
                        cases.append({"file": path.name, "member": member,
                                      "input": {"tree": serialized, "name": name},
                                      "expected": _tree(found) if found is not None else None})
                elif fn_name == "_mm":
                    values = {e.get("pos") for e in root.iter() if e.get("pos") is not None}
                    values |= {e.get("width") for e in root.iter() if e.get("width") is not None}
                    values |= {e.get("value") for e in root.iter() if e.get("value") is not None}
                    for value in sorted(values):
                        cases.append({"file": path.name, "member": member,
                                      "input": value, "expected": style_inventory._mm(value)})
                elif fn_name == "_text_of":
                    for element in root.iter():
                        cases.append({"file": path.name, "member": member,
                                      "input": {"tree": _tree(element)},
                                      "expected": style_inventory._text_of(element)})
                elif fn_name == "_run_text":
                    for element in root.iter():
                        if style_inventory._tag(element) == "run":
                            cases.append({"file": path.name, "member": member,
                                          "input": {"tree": _tree(element)},
                                          "expected": style_inventory._run_text(element)})
                elif fn_name == "_parse_fonts" and member == "Contents/header.xml":
                    cases.append({"file": path.name, "member": member,
                                  "input": {"tree": _tree(root)},
                                  "expected": style_inventory._parse_fonts(root)})
    # Include documented coercion/error edges in addition to fixture-derived values.
    if fn_name == "_mm":
        for value in (None, "invalid", "", 0, 1000):
            cases.append({"file": "<edge>", "member": "", "input": value,
                          "expected": style_inventory._mm(value)})
    return cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("function", choices=("_tag", "_first", "_mm", "_text_of", "_run_text", "_parse_fonts", "_lang_values"))
    ap.add_argument("--fixtures", type=Path, default=ROOT / "tests" / "fixtures")
    ap.add_argument("--dll", type=Path, default=ROOT / "hwpx_core" / "target" / "release" / "hwpx_core.dll")
    ap.add_argument("--cases", type=Path, help="write collected inputs and expected Python values")
    ns = ap.parse_args()
    cases = collect(ns.fixtures, ns.function)
    if ns.cases:
        ns.cases.parent.mkdir(parents=True, exist_ok=True)
        ns.cases.write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    if not ns.dll.exists():
        print(f"Rust DLL not found: {ns.dll}", file=sys.stderr)
        return 2
    lib = ctypes.CDLL(str(ns.dll))
    lib.hwpx_call.argtypes = (ctypes.c_char_p, ctypes.c_char_p)
    lib.hwpx_call.restype = ctypes.c_void_p
    lib.hwpx_last_error.restype = ctypes.c_char_p
    lib.hwpx_free.argtypes = (ctypes.c_void_p,)
    matches = differences = errors = 0
    for case in cases:
        payload = json.dumps(case["input"], ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        ptr = lib.hwpx_call(ns.function.encode("ascii"), payload)
        if not ptr:
            errors += 1
            print(f"ERROR {case['file']}:{case['member']}: {(lib.hwpx_last_error() or b'unknown').decode('utf-8', 'replace')}")
            continue
        try:
            actual = json.loads(ctypes.string_at(ptr).decode("utf-8"))
        finally:
            lib.hwpx_free(ptr)
        if actual == case["expected"]:
            matches += 1
        else:
            differences += 1
            print(json.dumps({"case": case["file"], "member": case["member"],
                              "input": case["input"], "python": case["expected"], "rust": actual},
                             ensure_ascii=False))
    print(json.dumps({"function": ns.function, "cases": len(cases), "matches": matches,
                      "differences": differences, "errors": errors}, ensure_ascii=False))
    return 0 if differences == 0 and errors == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
