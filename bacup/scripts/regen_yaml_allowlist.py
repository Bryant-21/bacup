"""Regenerate the file allowlist used by the M7 YAML-load test."""
from __future__ import annotations

import ast
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "py_bacup_lib" / "python"))

from bacup_lib.tests.test_m7_yaml_load_allowlist import (
    _ALLOWED_YAML_LOAD_REASONS,
    _call_name,
    _is_yaml_load,
)

conv_root = Path(__file__).resolve().parents[1] / "py_bacup_lib" / "python" / "bacup_lib"

hits: set[str] = set()
for path in conv_root.rglob("*.py"):
    if "tests" in path.relative_to(conv_root).parts:
        continue
    rel = path.relative_to(conv_root).as_posix()
    text = path.read_text(encoding="utf-8", errors="replace")
    tree = ast.parse(text, filename=str(path))
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if not _is_yaml_load(_call_name(node.func)):
            continue
        hits.add(rel)

print("_ALLOWED_YAML_LOAD_REASONS = {")
for rel in sorted(hits):
    reason = _ALLOWED_YAML_LOAD_REASONS.get(rel, "<missing allowlist reason>")
    print(f"    {rel!r}: {reason!r},")
print("}")

missing = sorted(rel for rel in hits if rel not in _ALLOWED_YAML_LOAD_REASONS)
if missing:
    print("\n# WARNING: missing reasons for:", file=sys.stderr)
    for rel in missing:
        print(f"#   {rel}", file=sys.stderr)
