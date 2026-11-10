import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.rename_regress import compare  # noqa: E402

KLIB = ROOT / "klib" / "target" / "release" / "klib.exe"
names = "\n".join(f"renamed_{i:04d}.jpg" for i in range(1000))
cases = [
    ("replace", {"kind": "replace", "find": "IMG_", "to": "Image_"}, "plain"),
    ("add", {"kind": "add", "text": "_done", "position": "suffix"}, "plain"),
    ("uuid_replace", {"kind": "uuid_replace", "text": "uuid"}, "plain"),
    ("clear", {"kind": "clear"}, "plain"),
    ("del_pos", {"kind": "del_pos", "start": 2, "end": 4}, "plain"),
    ("del_between", {"kind": "del_between", "open": "(", "close": ")"}, "plain"),
    ("digits_keep", {"kind": "digits_keep"}, "plain"),
    ("digits_delete", {"kind": "digits_delete"}, "plain"),
    ("digits_pad", {"kind": "digits_pad", "digits": 5}, "plain"),
    ("number", {"kind": "number", "digits": 4}, "plain"),
    ("number_shift", {"kind": "number_shift", "delta": 1}, "plain"),
    ("ext_del", {"kind": "ext_del"}, "plain"),
    ("ext_add", {"kind": "ext_add", "ext": ".bak"}, "plain"),
    ("ext_replace", {"kind": "ext_replace", "ext": ".png"}, "plain"),
    ("ext_case", {"kind": "ext_case", "mode": "lower"}, "plain"),
    ("date", {"kind": "date", "source": "mtime", "format": "YYYYMMDD", "position": "prefix"}, "date"),
    ("exif_date", {"kind": "exif_date", "format": "YYYYMMDD", "position": "prefix"}, "exif_date"),
    ("cleanup", {"kind": "cleanup"}, "plain"),
    ("case", {"kind": "case", "mode": "lower"}, "plain"),
    ("strip_result_suffix", {"kind": "strip_result_suffix"}, "plain"),
    ("doc_title", {"kind": "doc_title", "position": "replace"}, "doc_title"),
    ("template", {"kind": "template", "template": "{date}_{title}"}, "template"),
    ("names_list", {"kind": "names_list", "names": names}, "plain"),
]

results = []
for label, rule, fixture in cases:
    result = compare(1000, KLIB, [rule], {"on_conflict": "block"}, 0, fixture)
    result["rule"] = label
    results.append(result)
print(json.dumps(results, ensure_ascii=False, indent=2))
sys.exit(0 if all(x["differences"] == 0 for x in results) else 1)
