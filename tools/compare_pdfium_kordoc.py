"""Compare pypdfium2 with the installed kordoc runtime on synthetic PDFs.

Run with the project Python: python tools/compare_pdfium_kordoc.py
Fixtures are generated in the system temporary directory and removed afterwards.
"""
from __future__ import annotations

import difflib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from docfit_core import kordoc_runtime
from docfit_core.pdf_extract import extract_text


def _pdf(path: Path, pages: list[list[tuple[str, str]]]) -> None:
    """Write a tiny PDF using built-in Helvetica and explicit text objects."""
    objects = [b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n"]
    page_ids = []
    next_id = 3
    for lines in pages:
        page_id, content_id = next_id, next_id + 1
        next_id += 2
        page_ids.append(page_id)
        content = bytearray(b"BT /F1 12 Tf 72 740 Td\n")
        for index, (line, _font) in enumerate(lines):
            if index:
                content.extend(b"0 -24 Td\n")
            content.extend(b"(" + line.encode("ascii").replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)") + b") Tj\n")
        content.extend(b"ET")
        objects.append(
            f"{page_id} 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Resources<</Font<</F1 {next_id} 0 R>>>>/Contents {content_id} 0 R>>endobj\n".encode()
        )
        objects.append(f"{content_id} 0 obj<</Length {len(content)}>>stream\n".encode() + content + b"\nendstream endobj\n")
        objects.append(f"{next_id} 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n".encode())
        next_id += 1
    kids = b" ".join(f"{item} 0 R".encode() for item in page_ids)
    objects.insert(1, f"2 0 obj<</Type/Pages/Kids[{kids.decode()}]/Count {len(page_ids)}>>endobj\n".encode())
    path.write_bytes(b"%PDF-1.4\n" + b"".join(objects) + b"trailer<</Root 1 0 R>>\n%%EOF")


def _similarity(expected: str, actual: str) -> float:
    return difflib.SequenceMatcher(None, expected, actual, autojunk=False).ratio() * 100


def main() -> int:
    runtime = kordoc_runtime.find_runtime("4.21.30")
    if runtime is None:
        print(json.dumps({"status": "skip", "reason": "런타임 없음"}, ensure_ascii=False))
        return 0
    node, cli = runtime
    samples = {
        "한글 본문": [[("Hangul body PDF", "regular")]],
        "표": [[("Item Amount", "regular"), ("Apple 1200", "regular"), ("Pear 2300", "regular")]],
        "여러 쪽": [[("First page content", "regular")], [("Second page content", "regular")], [("Third page content", "regular")]],
        "특수문자": [[("Special: & < > ( ) [ ] { } / + - = ! ?", "regular")]],
        "빈 쪽": [[], [("Content after blank page", "regular")]],
    }
    results = []
    with tempfile.TemporaryDirectory(prefix="kordoc3b-") as folder:
        root = Path(folder)
        for name, pages in samples.items():
            source = root / f"sample-{len(results)}.pdf"
            _pdf(source, pages)
            expected = extract_text(source)
            completed = subprocess.run(
                [node, cli, str(source), "--format", "markdown", "--plain", "--silent"],
                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120,
                env={**os.environ, "DOCFIT_OFFLINE": "1", "ORT_DISABLE_TELEMETRY": "1"},
            )
            if completed.returncode:
                results.append({"kind": name, "error": (completed.stderr or completed.stdout).strip()})
                continue
            actual = completed.stdout.strip()
            expected = expected.replace("\r\n", "\n")
            actual = actual.replace("\r\n", "\n")
            matcher = difflib.SequenceMatcher(None, expected, actual, autojunk=False)
            missing = [line for line in expected.splitlines() if line.strip() and line not in actual]
            results.append({"kind": name, "similarity_percent": round(_similarity(expected, actual), 4),
                            "pdfium_characters": len(expected), "kordoc_characters": len(actual),
                            "missing_lines": missing, "pdfium_text": expected, "kordoc_text": actual,
                            "opcodes": matcher.get_opcodes()})
    print(json.dumps({"status": "complete", "runtime_version": "4.21.30", "results": results}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
