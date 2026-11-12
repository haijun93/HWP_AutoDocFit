"""Measure source startup stages without opening the application's GUI."""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
IMPORT_TIME_RE = re.compile(r"^import time:\s+(\d+)\s+\|\s+(\d+)\s+\|\s+(.+)$")


def parse_importtime_output(stderr_text: str, limit: int = 30) -> list[dict]:
    """Parse CPython -X importtime output and return largest cumulative entries."""
    modules = []
    for line in stderr_text.splitlines():
        match = IMPORT_TIME_RE.match(line.strip())
        if match is None:
            continue
        self_us, cumulative_us, name = match.groups()
        modules.append({
            "name": name.strip(),
            "self_us": int(self_us),
            "cumulative_us": int(cumulative_us),
        })
    return sorted(modules, key=lambda item: item["cumulative_us"], reverse=True)[:limit]


def measure(python_exe: Path, source: Path) -> dict:
    """Measure a plain source import (not __main__), suppressing all UI creation."""
    code = (
        "import importlib.util, pathlib, sys; "
        "p=pathlib.Path(sys.argv[1]).resolve(); sys.path.insert(0, str(p.parent)); "
        "s=importlib.util.spec_from_file_location('hwp_auto_docfit', p); "
        "m=importlib.util.module_from_spec(s); sys.modules[s.name]=m; s.loader.exec_module(m)"
    )
    started = time.perf_counter()
    result = subprocess.run(
        [str(python_exe), "-X", "importtime", "-c", code, str(source)],
        capture_output=True, text=True, encoding="utf-8", errors="replace", check=False,
        cwd=str(source.parent),
    )
    wall_ms = (time.perf_counter() - started) * 1000
    if result.returncode:
        raise RuntimeError(f"source import failed ({result.returncode}):\n{result.stderr}")
    return {
        "measurement_scope": "source module import only; application main() and GUI not run",
        "interpreter_to_entry_ms": round(wall_ms, 2),
        "importtime_top_30_cumulative": parse_importtime_output(result.stderr),
        "importtime_entry_count": sum(
            1 for line in result.stderr.splitlines() if IMPORT_TIME_RE.match(line.strip())
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--source", type=Path, default=ROOT / "hwp-auto-docfit.py")
    parser.add_argument("--json", type=Path, default=ROOT / "docs" / "startup-breakdown.json")
    args = parser.parse_args()
    data = measure(args.python.resolve(), args.source.resolve())
    data["python"] = str(args.python.resolve())
    data["source"] = str(args.source.resolve())
    args.json.parent.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(data, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
