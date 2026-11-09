"""Check the preview arrow characters against the configured font assets."""
from __future__ import annotations

from pathlib import Path
import ast
import re

from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[1]
FONT_PATHS = (
    ROOT / "assets/NotoSansKR.ttf",
    ROOT / "assets/fonts/NotoSansSymbols2-Regular.ttf",
    ROOT / "assets/fonts/STIX2Math.otf",
)
ARROW_SOURCE = ROOT.parent / "docfit_core/table_arrows.py"


def arrow_characters() -> set[str]:
    source = ARROW_SOURCE.read_text(encoding="utf-8")
    symbols = re.search(r"ARROW_SYMBOLS\s*=\s*(\([^)]+\))", source, re.S)
    known = re.search(r"_KNOWN\s*=\s*set\(ARROW_SYMBOLS\)\s*\|\s*set\(([^)]+)\)", source)
    if not symbols or not known:
        raise RuntimeError("Could not extract ARROW_SYMBOLS and _KNOWN from table_arrows.py")
    chosen = ast.literal_eval(symbols.group(1))
    detected = ast.literal_eval(known.group(1))
    return set(chosen) | set(detected)


def main() -> int:
    required = arrow_characters()
    supported: set[str] = set()
    for path in FONT_PATHS:
        font = TTFont(path)
        cmap = set(font.getBestCmap())
        chars = {char for char in required if ord(char) in cmap}
        supported |= chars
        print(f"{path.name}: {len(chars)}/{len(required)} required arrow glyphs")
    missing = sorted(required - supported)
    if missing:
        print("Missing glyphs: " + ", ".join(f"U+{ord(c):04X} {c}" for c in missing))
        return 1
    print(f"All {len(required)} arrow glyphs are available in the configured font stack.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
