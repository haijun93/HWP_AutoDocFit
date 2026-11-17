"""Lightweight plain-text extraction for digital PDFs."""

from __future__ import annotations

from pathlib import Path


def extract_text(source: str | Path) -> str:
    """Extract the text layer in page order; callers decide whether to fall back."""
    import pypdfium2 as pdfium

    pages: list[str] = []
    document = pdfium.PdfDocument(str(source))
    try:
        for page in document:
            text_page = page.get_textpage()
            try:
                text = text_page.get_text_bounded()
            finally:
                text_page.close()
            if text.strip():
                pages.append(text.strip())
            page.close()
    finally:
        document.close()
    return "\n\n".join(pages)


def extract_to_file(source: str | Path, target: str | Path, expected_text: str | None = None) -> bool:
    """Write nonempty text, optionally requiring exact parity with the legacy result."""
    try:
        text = extract_text(source)
    except Exception:
        return False
    if not text.strip():
        return False
    if expected_text is not None and text.rstrip() != expected_text.rstrip():
        return False
    destination = Path(target)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(text.rstrip() + "\n", encoding="utf-8")
    return True
