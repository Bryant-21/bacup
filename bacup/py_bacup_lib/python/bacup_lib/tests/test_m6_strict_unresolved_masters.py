"""Unresolved master references stay strict when requested."""
from __future__ import annotations

import pytest

import creation_lib.esp.native_runtime as native_runtime
from creation_lib.esp.plugin import Plugin


def _native_available() -> bool:
    try:
        native_runtime.load_native_module()
    except Exception:
        return False
    return True


pytestmark = pytest.mark.skipif(
    not _native_available(),
    reason="esp_authoring_core native module is not installed",
)


def _policy() -> str:
    return (
        '{"follow_signatures": null, "asset_kinds": null, "reverse_passes": [], '
        '"behavior_bundle": false, "character_assets": false, '
        '"animation_lookup": false, "creature_dir_scan": false, "max_depth": null}'
    )


def test_strict_unresolved_master_reference_reports_missing_form_key() -> None:
    plugin = Plugin.new("M6Strict.esp", game="fo4", masters=["MissingMaster.esm"])
    root = plugin.new_record("WEAP")
    root.editor_id = "RootWeapon"
    root.add_subrecord("CNAM", (0x00000900).to_bytes(4, "little"), semantic_type="formid")
    plugin.add_record(root)

    result = plugin.walk_dependencies(
        root_form_keys=[f"M6Strict.esp:{root.form_id & 0x00FFFFFF:06X}"],
        policy_json=_policy(),
        strict=True,
    )

    assert result["errors"] == ["Unresolved FormKey: MissingMaster.esm:000900"]
    assert result["unresolved_form_keys"] == []
