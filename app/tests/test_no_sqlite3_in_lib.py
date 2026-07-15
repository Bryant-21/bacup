"""Guard test: no runtime library module may import stdlib ``sqlite3``.

All DB work must route through ``creation_lib.db.native_runtime`` (Rust ``db_native``).
Test fixtures under ``bacup/py_bacup_lib/python/bacup_lib/tests/`` are allowed to build
synthetic fixture DBs via ``sqlite3`` — they are not runtime code.
"""

from __future__ import annotations

import re
from pathlib import Path

from app.paths import get_app_root

_IMPORT_RE = re.compile(r"^(?:import|from)\s+sqlite3\b", re.MULTILINE)

# Directories whose non-test Python files must be sqlite3-free.
RUNTIME_DIRS = (
    "py_creation_lib/python/creation_lib/db",
    "py_creation_lib/python/creation_lib/preprocessor",
    "bacup/py_bacup_lib/python/bacup_lib",
    "py_creation_lib/python/creation_lib/mod",
    "py_creation_lib/python/creation_lib/papyrus_lsp",
    "py_creation_lib/python/creation_lib/ba2",
)


def _is_test_file(path: Path) -> bool:
    return "tests" in path.parts or path.name.startswith("test_")


def test_no_sqlite3_imports_in_runtime_lib():
    root = get_app_root()
    offenders: list[str] = []
    for rel in RUNTIME_DIRS:
        base = root / rel
        if not base.is_dir():
            continue
        for py_file in base.rglob("*.py"):
            if _is_test_file(py_file):
                continue
            text = py_file.read_text(encoding="utf-8", errors="replace")
            if _IMPORT_RE.search(text):
                offenders.append(str(py_file.relative_to(root)))
    assert not offenders, (
        "runtime library modules must not import sqlite3 directly "
        "(use creation_lib.db.native_runtime): " + ", ".join(offenders)
    )
