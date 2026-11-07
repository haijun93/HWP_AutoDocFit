# Threat model — HWP AutoDocFit

## What the project is
A Windows desktop tool used in Korean public offices to clean up and reformat official documents
(Hancom Office HWP/HWPX), convert file formats, manage clipboard history and keep a local document
library/notebook. UI: Flutter (WebView2 via pywebview) + Python services; helper engines in Rust
(`klib/` library & notebook engine, `kagent/` clipboard agent, `hierarchy/`, `editor_core/`).
Everything runs locally on the user's PC; there is no server component.

## Where untrusted input enters (in scope)
1. **Documents the user opens or drops**: HWPX/DOCX/XLSX/PPTX/ODT (ZIP + XML), PDF, HWP, Markdown/TXT/HTML,
   images. Parsed by `docfit_core/` (XML via `defusedxml` only; ZIP via `open_hwpx_safely`/`open_zip_safely`
   size/ratio/entry limits).
2. **Clipboard content** (HTML Format/CF_HTML from Excel/web, text, images, file lists) handled by `kagent/`
   and `docfit_core` converters (e.g. Excel table → Hancom paste conversion).
3. **Update metadata and downloads** from GitHub/GitLab releases (`docfit_core/updater.py`): host allow-list,
   HTTPS only, SHA-256 check, optional Ed25519-signed `latest.json` (flag, off by default).
4. **Local IPC**: named pipes of `klib`/`kagent` (per-user SID in the pipe name) and the pywebview JS bridge
   (`desktop_web_ui.py` exposes an allow-listed API).
5. **Output of external converters** (kordoc/Node, Pandoc, PDF libraries) fed back into the app.

## Out of scope
Hancom Office itself, Windows, WebView2/Chromium, Tk, and the third-party converters' own bugs; attacks that
already require running code as the same Windows user (except where noted below); the marketing website.

## Severity guidance
- **Critical**: code execution from opening/dropping a document or pasting clipboard content; update
  substitution that installs attacker code.
- **High**: arbitrary file write/overwrite outside the chosen output folder (path traversal via ZIP/HWPX
  entries, file names), reading arbitrary files via the JS bridge or pipes, cross-user access to the
  named pipes.
- **Medium**: denial of service from a crafted document (zip bomb, XML expansion, unbounded memory/CPU),
  leaking document contents to temp folders that are not cleaned up.
- **Low**: crashes from malformed input without other impact, UI-only issues, missing hardening headers.
Reports without a working reproducer are capped at medium.

## Known issues (please do not re-report)
From our review (`docs/보안성_검토_최고수준_2026-10-12.md`): S1 single trust root for update hashes
(signed metadata implemented but disabled pending key management), S6 no CSP on the WebView pages
(pywebview's bridge needs `unsafe-eval`; under review), S9 vendor DLL provenance documentation.
S2–S5, S7, S8 are fixed.

## How we would like reports
A minimal reproducer file (synthetic — never real office documents), the affected module/function, and a
suggested patch with a unit test under `tests/`. Tests run on Linux with
`python scripts/linux_stub_test_runner.py discover -s tests` (Windows-only paths are stubbed).
