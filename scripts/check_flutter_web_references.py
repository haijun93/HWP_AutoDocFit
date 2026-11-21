"""Compare local references in Flutter's web entry files with a built bundle.

This is a static inventory aid. Runtime-generated URLs and browser implicit
requests (for example /favicon.ico) still require request URL logging to prove.
"""
from __future__ import annotations

import argparse
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit


class References(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.values: list[tuple[str, str]] = []

    def handle_starttag(self, tag: str, attrs) -> None:
        attrs = dict(attrs)
        for key in ("src", "href"):
            value = attrs.get(key)
            if value:
                self.values.append((f"<{tag}> {key}", value))


def local_path(value: str) -> str | None:
    parsed = urlsplit(value)
    if parsed.scheme or value in ("", ".", "./") or value.startswith(("#", "data:")):
        return None
    return parsed.path.lstrip("/")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path, help="Flutter build/web directory")
    args = parser.parse_args()
    root = args.bundle.resolve()
    html = References()
    html.feed((root / "index.html").read_text(encoding="utf-8"))
    refs = list(html.values)
    bootstrap = root / "flutter_bootstrap.js"
    if bootstrap.is_file():
        source = bootstrap.read_text(encoding="utf-8", errors="replace")
        # Flutter's generated loader defaults to these local entry resources.
        for path in ("main.dart.js", "flutter_service_worker.js"):
            if path in source:
                refs.append(("flutter_bootstrap.js runtime default", path))
    failed = False
    for origin, value in refs:
        path = local_path(value)
        if path is None:
            print(f"SKIP {origin}: {value}")
            continue
        exists = (root / path).is_file()
        print(f"{'OK' if exists else 'MISSING'} {origin}: {path}")
        failed |= not exists
    favicon = root / "favicon.ico"
    print(f"{'PRESENT' if favicon.is_file() else 'ABSENT'} browser implicit candidate: favicon.ico")
    print("NOTE: Static inspection cannot establish which URL returned HTTP 404.")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
