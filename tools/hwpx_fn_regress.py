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
            if fn_name == "_sections":
                cases.append({"file": path.name, "member": "",
                              "input": {"names": archive.namelist()},
                              "expected": style_inventory._sections(archive)})
                continue
            chars = {}
            defs = {}
            if fn_name in ("_table_signature", "_table_info", "inline_effects", "paragraph_style_type", "_mode"):
                try:
                    header_root = ET.fromstring(archive.read("Contents/header.xml"))
                    defs = style_inventory.parse_header(header_root)
                    chars = defs.get("chars", {})
                except Exception:
                    chars = {}
                    defs = {}
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
                elif fn_name in ("_parse_char", "_parse_para", "_parse_border_fill", "_parse_numbering", "_parse_tab") and member == "Contents/header.xml":
                    target = {"_parse_char": "charPr", "_parse_para": "paraPr", "_parse_border_fill": "borderFill", "_parse_numbering": "numbering", "_parse_tab": "tabPr"}[fn_name]
                    fonts = style_inventory._parse_fonts(root)
                    for element in root.iter():
                        if style_inventory._tag(element) == target:
                            if fn_name == "_parse_char":
                                args = {"tree": _tree(element), "fonts": fonts}
                                expected = style_inventory._parse_char(element, fonts)
                            else:
                                args = {"tree": _tree(element)}
                                expected = getattr(style_inventory, fn_name)(element)
                            cases.append({"file": path.name, "member": member, "input": args, "expected": expected})
                elif fn_name == "parse_header" and member == "Contents/header.xml":
                    cases.append({"file": path.name, "member": member,
                                  "input": {"tree": _tree(root)},
                                  "expected": style_inventory.parse_header(root)})
                elif fn_name == "_walk":
                    items = []
                    style_inventory._walk(root, "본문", items)
                    cases.append({"file": path.name, "member": member,
                                  "input": {"tree": _tree(root), "context": "본문"},
                                  "expected": [[_tree(p), ctx] for p, ctx in items]})
                elif fn_name == "_paragraph_runs":
                    for element in root.iter():
                        if style_inventory._tag(element) == "p":
                            cases.append({"file": path.name, "member": member,
                                          "input": {"tree": _tree(element)},
                                          "expected": [_tree(r) for r in style_inventory._paragraph_runs(element)]})
                elif fn_name == "_char_signature":
                    for element in root.iter():
                        if style_inventory._tag(element) == "p":
                            runs = style_inventory._paragraph_runs(element)
                            seq, main = style_inventory._char_signature(runs, chars)
                            cases.append({"file": path.name, "member": member,
                                          "input": {"runs": [_tree(r) for r in runs]},
                                          "expected": [list(seq), main]})
                elif fn_name == "_table_signature":
                    for element in root.iter():
                        if style_inventory._tag(element) == "tbl":
                            col_cnt, border_fill, cell_fills, first_row_chars, wide = style_inventory._table_signature(element, chars)
                            cases.append({"file": path.name, "member": member,
                                          "input": {"tbl": _tree(element), "chars": chars},
                                          "expected": [col_cnt, border_fill, list(cell_fills), list(first_row_chars) if isinstance(first_row_chars, tuple) else first_row_chars, wide]})
                elif fn_name == "_table_info":
                    for element in root.iter():
                        if style_inventory._tag(element) == "tbl":
                            cases.append({"file": path.name, "member": member,
                                          "input": {"tbl": _tree(element), "chars": chars},
                                          "expected": style_inventory._table_info(element, chars)})
                elif fn_name == "_marker_class":
                    for element in root.iter():
                        if style_inventory._tag(element) == "p":
                            text = style_inventory._text_of(element).strip()
                            if text:
                                marker, _ = style_inventory.leading_marker(text)
                                cases.append({"file": path.name, "member": member,
                                              "input": marker,
                                              "expected": style_inventory._marker_class(marker)})
                elif fn_name == "inline_effects":
                    seen_pairs = set()
                    for element in root.iter():
                        if style_inventory._tag(element) == "p":
                            runs = style_inventory._paragraph_runs(element)
                            seq, main = style_inventory._char_signature(runs, chars)
                            if main and main in chars:
                                for cid in seq:
                                    if cid and cid in chars and cid != main:
                                        pair = (main, cid)
                                        if pair not in seen_pairs:
                                            seen_pairs.add(pair)
                                            main_char = chars[main]
                                            other_char = chars[cid]
                                            cases.append({
                                                "file": path.name, "member": member,
                                                "input": {"main": main_char, "other": other_char},
                                                "expected": sorted(list(style_inventory.inline_effects(main_char, other_char)))
                                            })
                elif fn_name == "paragraph_style_type":
                    if defs:
                        for element in root.iter():
                            if style_inventory._tag(element) == "p":
                                typed = style_inventory.paragraph_style_type(element, defs)
                                expected = None
                                if typed is not None:
                                    key, meas = typed
                                    expected = [
                                        list(key),
                                        {
                                            **meas,
                                            "char_seq": list(meas["char_seq"]),
                                            "effects": list(meas["effects"]),
                                        }
                                    ]
                                cases.append({
                                    "file": path.name, "member": member,
                                    "input": {"para": _tree(element), "defs": defs},
                                    "expected": expected
                                })
                elif fn_name == "_mode":
                    if defs:
                        from collections import Counter
                        para_counter = Counter()
                        style_counter = Counter()
                        for element in root.iter():
                            if style_inventory._tag(element) == "p":
                                pid = element.get("paraPrIDRef")
                                sid = element.get("styleIDRef")
                                if pid:
                                    para_counter[pid] += 1
                                if sid:
                                    style_counter[sid] += 1
                        if para_counter:
                            cases.append({
                                "file": path.name, "member": member,
                                "input": {"counter": list(para_counter.items()), "default": None},
                                "expected": style_inventory._mode(para_counter, None)
                            })
                        if style_counter:
                            cases.append({
                                "file": path.name, "member": member,
                                "input": {"counter": list(style_counter.items()), "default": None},
                                "expected": style_inventory._mode(style_counter, None)
                            })
                elif fn_name == "_shade":
                    values = {e.get("shadeColor") for e in root.iter() if e.get("shadeColor") is not None}
                    for value in sorted(values):
                        cases.append({"file": path.name, "member": member, "input": value, "expected": style_inventory._shade(value)})
                elif fn_name == "_lang_values":
                    if any(element.get(lang) is not None for element in root.iter()
                           for lang in style_inventory.LANGS):
                        for element in root.iter():
                            if any(element.get(lang) is not None for lang in style_inventory.LANGS):
                                for cast_name, cast in (("int", int), ("str", str)):
                                    cases.append({"file": path.name, "member": member,
                                                  "input": {"tree": _tree(element), "cast": cast_name},
                                                  "expected": style_inventory._lang_values(element, cast)})
    # Include documented coercion/error edges in addition to fixture-derived values.
    if fn_name == "_mm":
        for value in (None, "invalid", "", 0, 1000):
            cases.append({"file": "<edge>", "member": "", "input": value,
                          "expected": style_inventory._mm(value)})
    if fn_name == "_shade":
        values = {case["input"] for case in cases}
        values.update((None, "none", "#FFFFFF", "#ffffff", "red", ""))
        for value in sorted(values, key=lambda v: "" if v is None else str(v)):
            cases.append({"file": "<edge>", "member": "", "input": value, "expected": style_inventory._shade(value)})
    if fn_name == "_lang_values":
        for cast_name, cast in (("int", int), ("str", str)):
            cases.append({"file": "<edge>", "member": "",
                          "input": {"tree": None, "cast": cast_name},
                          "expected": style_inventory._lang_values(None, cast)})
    if fn_name == "_sections":
        class _MockZip:
            def __init__(self, names):
                self._names = names
            def namelist(self):
                return self._names

        for idx, edge in enumerate([
            ["Contents/section10.xml", "Contents/section2.xml", "Contents/section1.xml", "Contents/section0.xml", "Contents/header.xml", "settings.xml"],
            [],
            ["Contents/sectionA.xml", "Contents/section_1.xml", "section1.xml", "Contents/section.xml"],
            ["Contents/section0.xml", "Contents/section1.xml", "Contents/section01.xml", "Contents/section2.xml"],
        ]):
            cases.append({"file": f"<edge_{idx}>", "member": "",
                          "input": {"names": edge},
                          "expected": style_inventory._sections(_MockZip(edge))})
    if fn_name == "_char_signature":
        cases.append({"file": "<edge>", "member": "",
                      "input": {"runs": []},
                      "expected": [[], None]})
    if fn_name == "_marker_class":
        for marker in ("", "1.", "12.", "1.2", "1) ", "(1)", "(15)", "가.", "나.", "하.", "가) ", "(가)", "①", "②", "⑳", "㉠", "㉡", "㉻", "• "):
            cases.append({"file": "<edge>", "member": "",
                          "input": marker,
                          "expected": style_inventory._marker_class(marker)})
    if fn_name == "inline_effects":
        sample_char = {
            "font": {"hangul": "함초롬바탕"},
            "size_pt": 10.0,
            "bold": False,
            "italic": False,
            "underline": "NONE",
            "color": "#000000",
            "shade": None,
            "superscript": False,
            "subscript": False,
        }
        all_on_char = {
            "font": {"hangul": "함초롬바탕"},
            "size_pt": 10.0,
            "bold": True,
            "italic": True,
            "underline": "BOTTOM",
            "color": "#FF0000",
            "shade": "#FFFF00",
            "superscript": True,
            "subscript": True,
        }
        cases.append({"file": "<edge>", "member": "none_main", "input": {"main": None, "other": sample_char}, "expected": []})
        cases.append({"file": "<edge>", "member": "none_other", "input": {"main": sample_char, "other": None}, "expected": []})
        cases.append({"file": "<edge>", "member": "both_none", "input": {"main": None, "other": None}, "expected": []})
        cases.append({"file": "<edge>", "member": "same_char", "input": {"main": sample_char, "other": sample_char}, "expected": []})
        cases.append({"file": "<edge>", "member": "all_effects", "input": {"main": sample_char, "other": all_on_char},
                      "expected": sorted(list(style_inventory.inline_effects(sample_char, all_on_char)))})
    if fn_name == "paragraph_style_type":
        cases.append({
            "file": "<edge>", "member": "empty_para",
            "input": {"para": {"tag": "hp:p", "attrib": {}, "children": []}, "defs": {"chars": {}, "paras": {}}},
            "expected": None,
        })
    if fn_name == "_mode":
        from collections import Counter
        cases.append({"file": "<edge>", "member": "empty_default_none", "input": {"counter": [], "default": None}, "expected": None})
        cases.append({"file": "<edge>", "member": "empty_default_val", "input": {"counter": [], "default": "fallback"}, "expected": "fallback"})
        c1 = Counter({"a": 2, "b": 5, "c": 1})
        cases.append({"file": "<edge>", "member": "dict_counter", "input": {"counter": list(c1.items()), "default": None}, "expected": style_inventory._mode(c1)})
        c2 = Counter()
        c2["first"] = 3
        c2["second"] = 3
        c2["third"] = 1
        cases.append({"file": "<edge>", "member": "tie_break", "input": {"counter": list(c2.items()), "default": None}, "expected": style_inventory._mode(c2)})
        c3 = Counter({-5: 2, 0: 7, 5: 3})
        cases.append({"file": "<edge>", "member": "int_keys", "input": {"counter": list(c3.items()), "default": None}, "expected": style_inventory._mode(c3)})
    return cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("function", choices=("_tag", "_first", "_mm", "_text_of", "_run_text", "_parse_fonts", "_lang_values", "_shade", "_parse_char", "_parse_para", "_parse_border_fill", "_parse_numbering", "_parse_tab", "parse_header", "_sections", "_walk", "_paragraph_runs", "_char_signature", "_table_signature", "_table_info", "_marker_class", "inline_effects", "paragraph_style_type", "_mode"))
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
