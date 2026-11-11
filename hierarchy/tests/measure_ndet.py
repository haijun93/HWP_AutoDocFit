"""Recheck NDET through explicit release DLL/EXE paths; never rewrite the input."""
import collections
import hashlib
import json
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from docfit_app.hierarchy_bridge import run_library, run_executable
from docfit_core.style_hierarchy import analyze_hierarchy


def main():
    fixture = ROOT / "tests/fixtures/T1111_NDET.json"
    paragraphs = json.loads(fixture.read_text(encoding="utf-8"))
    normalize = lambda value: json.dumps(value, ensure_ascii=False, sort_keys=True)
    expected = normalize(analyze_hierarchy(paragraphs))
    report = {"fixture": str(fixture), "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest()}
    for name, path, call in [
        ("dll", ROOT / "hierarchy/target/release/hierarchy.dll", run_library),
        ("exe", ROOT / "hierarchy/target/release/hierarchy.exe", run_executable),
        ("python", None, lambda _, ps: analyze_hierarchy(ps)),
    ]:
        started = time.perf_counter()
        counts = collections.Counter(normalize(call(path, paragraphs)) for _ in range(1000))
        report[name] = {
            "path": str(path) if path else None,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest() if path else None,
            "calls": 1000, "distinct_results": len(counts),
            "python_matches": counts[expected], "errors": counts["null"],
            "elapsed_seconds": round(time.perf_counter() - started, 6),
            "variants": [{"count": n, "result": json.loads(v)} for v, n in counts.items()],
        }
        print(name, {k: v for k, v in report[name].items() if k != "variants"}, flush=True)
    (ROOT / "hierarchy/tests/ndet_measurements.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    assert all(report[n]["distinct_results"] == 1 and report[n]["python_matches"] == 1000
               and report[n]["errors"] == 0 for n in ("dll", "exe", "python"))


if __name__ == "__main__":
    main()
