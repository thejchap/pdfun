"""Shared PDF-inspection helpers for the test suite."""

from __future__ import annotations

import fitz


def content_stream(pdf_bytes: bytes) -> bytes:
    """Return the concatenated, decompressed content stream of every page.

    Use for assertions that scan drawn PDF operators (``Tj``, ``Td``, ``re``,
    ``rg``, ``Do`` …). Dictionary-level checks (``/Title``, ``/FlateDecode``)
    should still scan ``doc.to_bytes()`` directly — those live in object
    bodies, not in content streams.
    """
    doc = fitz.open(stream=pdf_bytes, filetype="pdf")
    try:
        parts = [page.read_contents() for page in doc]
    finally:
        doc.close()
    # Normalize with leading and trailing newlines so callers can assert on
    # patterns like `b"\nf\n"` (the original uncompressed stream always ended
    # with a newline; pymupdf's decompressed form may not preserve it).
    return b"\n" + b"\n".join(parts) + b"\n"
