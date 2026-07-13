# ADR-117 §7.2 — v1.99.0 tombstone.
#
# This module is part of the `wifi-densepose==1.99.0` PyPI release.
# Its ONLY job is to raise ImportError on import so any project that
# upgraded from the legacy 1.x line gets a clear migration error
# rather than a silent broken import.
#
# The real package lives at `wifi-densepose>=2.0.0` (built by the
# PyO3+maturin pipeline in `python/`).
raise ImportError(
    "wifi-densepose 1.x has been superseded by v2.0.0 which wraps the Rust-based stack.\n"
    "\n"
    "  pip install wifi-densepose==2.0.0\n"
    "\n"
    "Migration guide: https://github.com/Gestusition/KdView/blob/main/docs/pip-migration.md\n"
    "Modernization rationale: https://github.com/Gestusition/KdView/blob/main/docs/adr/ADR-117-pip-wifi-densepose-modernization.md\n"
    "Legacy v1 source (archived): https://github.com/Gestusition/KdView/tree/main/archive/v1\n"
)
