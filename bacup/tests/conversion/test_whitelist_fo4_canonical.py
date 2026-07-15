"""Diagnostic whitelist artifact checks.

The converter no longer loads generated field whitelists at runtime. These
files remain as diagnostic snapshots for now, so this test only verifies their
basic shape and that runtime modules do not import the old target-field filter.
"""
from __future__ import annotations

import ast
import re
from pathlib import Path

import pytest
import yaml

_REPO_ROOT = Path(__file__).resolve().parents[3]
_WHITELIST = (
    _REPO_ROOT
    / "bacup/py_bacup_lib/python/bacup_lib/record/whitelists/fo4.yaml"
)
_CONVERSION_ROOT = _REPO_ROOT / "bacup/py_bacup_lib/python/bacup_lib"
_SIG_PAT = re.compile(r"^[A-Z_]{4}$")


@pytest.fixture(scope="module")
def whitelist() -> dict:
    with open(_WHITELIST, encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def test_diagnostic_whitelist_has_expected_shape(whitelist: dict) -> None:
    assert whitelist["game"] == "fo4"
    assert isinstance(whitelist.get("record_types"), dict)
    assert isinstance(whitelist.get("nested"), dict)
    assert isinstance(whitelist.get("canonical_order"), dict)


def test_diagnostic_whitelist_record_types_are_sig_codes(whitelist: dict) -> None:
    for section in ("record_types", "nested", "canonical_order"):
        bad = [
            key
            for key in (whitelist.get(section) or {})
            if not _SIG_PAT.match(str(key))
        ]
        assert bad == []


def test_runtime_modules_do_not_import_field_whitelist_tools() -> None:
    offenders: list[str] = []
    for path in _CONVERSION_ROOT.rglob("*.py"):
        rel = path.relative_to(_CONVERSION_ROOT).as_posix()
        if rel.startswith("tests/"):
            continue
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom) and node.module:
                if "whitelist" in node.module:
                    offenders.append(f"{rel}: {node.module}")
            if isinstance(node, ast.Import):
                offenders.extend(
                    f"{rel}: {alias.name}"
                    for alias in node.names
                    if "whitelist" in alias.name
                )
    assert offenders == []
