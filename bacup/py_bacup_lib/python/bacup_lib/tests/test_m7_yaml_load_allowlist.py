from __future__ import annotations

import ast
from pathlib import Path


_ALLOWED_YAML_LOAD_REASONS = {
    "animation/event_mapper.py": "config",
    "animation/weapon_family_classifier.py": "config",
    "behavior/driver_synth.py": "config",
    "behavior/templates/_schema.py": "config",
    "creature/catalog.py": "config",
    "equipment/omod_filter.py": "config",
    "omod_filter.py": "config",
    "face/bone_solve.py": "config",
    "face/hair_table.py": "config",
    "fixups.py": "config or generated output validation only",
    "record/translation_map_data.py": "config",
    "record/translation_map_validator.py": "developer config validation",
    "skeleton/skeleton_mapper.py": "config",
    "upgrade_manifest.py": "config",
    "workflows/asset_phases.py": "config",
    "walker/policy.py": "config",
    "yaml_util.py": "loader implementation",
}

_FORBIDDEN_IMPORTS = {
    "creation_lib.db.record_loader",
}

_FORBIDDEN_NAMES = {
    "RecordLoader",
    "load_full_yaml",
}

_FORBIDDEN_TEXT = {
    "record_refs",
    "yaml_path",
    "source_records_db",
    "target_records_db",
}


def _call_name(node: ast.AST) -> str | None:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        parent = _call_name(node.value)
        return f"{parent}.{node.attr}" if parent else node.attr
    return None


def _is_yaml_load(call_name: str | None) -> bool:
    return call_name in {"yaml.load", "yaml.safe_load", "safe_load_hex"}


def test_conversion_runtime_has_no_record_db_or_record_yaml_reads() -> None:
    conversion_root = Path(__file__).resolve().parents[1]
    failures: list[str] = []

    for path in conversion_root.rglob("*.py"):
        rel = path.relative_to(conversion_root).as_posix()
        if "tests" in path.relative_to(conversion_root).parts:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        tree = ast.parse(text, filename=str(path))

        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom) and node.module in _FORBIDDEN_IMPORTS:
                failures.append(f"{rel}:{node.lineno}: forbidden import {node.module}")
            if isinstance(node, ast.Name) and node.id in _FORBIDDEN_NAMES:
                failures.append(f"{rel}:{node.lineno}: forbidden name {node.id}")
            if isinstance(node, ast.Attribute) and node.attr in _FORBIDDEN_NAMES:
                failures.append(f"{rel}:{node.lineno}: forbidden attr {node.attr}")
            if isinstance(node, ast.Call) and _is_yaml_load(_call_name(node.func)):
                if rel not in _ALLOWED_YAML_LOAD_REASONS:
                    failures.append(f"{rel}:{node.lineno}: unclassified YAML load")

        for needle in _FORBIDDEN_TEXT:
            scan_text = text
            if needle == "record_refs":
                scan_text = (
                    scan_text
                    .replace("conversion_record_refs_by_signature", "")
                    .replace("conversion_record_refs_by_form_keys", "")
                )
            if needle in scan_text:
                failures.append(f"{rel}: forbidden runtime text {needle}")

    assert failures == []
