from __future__ import annotations

import ast
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[3]
CONVERSION_ROOT = PROJECT_ROOT / "bacup/py_bacup_lib/python/bacup_lib"
TOOLS_ROOT = PROJECT_ROOT / "tools"

ALLOWED_LEGACY_PATH_PREFIXES = (
    "data/",
    "docs/",
    "external_mods/",
    "references/",
    "bacup/tests/fixtures/",
    "whitelists/",
)


def _active_text_files() -> list[Path]:
    roots = [CONVERSION_ROOT, PROJECT_ROOT / "bacup/tests/conversion"]
    files: list[Path] = []
    for root in roots:
        for path in root.rglob("*"):
            if not path.is_file() or path.suffix != ".py":
                continue
            rel = path.relative_to(PROJECT_ROOT).as_posix()
            if path == Path(__file__):
                continue
            if "bacup/py_bacup_lib/python/bacup_lib/tests" in rel:
                continue
            if any(rel.startswith(prefix) for prefix in ALLOWED_LEGACY_PATH_PREFIXES):
                continue
            files.append(path)
    return files


def test_removed_legacy_files_are_absent() -> None:
    assert not (CONVERSION_ROOT / "translation_maps/_vocab_fo4.yaml").exists()
    assert not (TOOLS_ROOT / "migrate_translation_map.py").exists()
    assert not (TOOLS_ROOT / "migrate_whitelist.py").exists()


def test_active_conversion_code_has_no_legacy_vocab_terms() -> None:
    banned = ("_vocab_fo4", "type_renames", "field_renames", "Spr" "iggit", "Muta" "gen")
    hits: list[str] = []
    for path in _active_text_files():
        text = path.read_text(encoding="utf-8")
        for term in banned:
            if term in text:
                hits.append(f"{path.relative_to(PROJECT_ROOT)} contains {term}")
    assert hits == []


def test_runtime_does_not_import_whitelist_tools() -> None:
    offenders: list[str] = []
    for path in (CONVERSION_ROOT / "orchestrator.py",):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom) and node.module and "whitelist" in node.module:
                offenders.append(f"{path.name}: {node.module}")
            if isinstance(node, ast.Import):
                offenders.extend(
                    f"{path.name}: {alias.name}"
                    for alias in node.names
                    if "whitelist" in alias.name
                )
    assert offenders == []
