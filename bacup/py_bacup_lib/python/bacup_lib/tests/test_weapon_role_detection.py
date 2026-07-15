from __future__ import annotations

from types import SimpleNamespace

from bacup_lib.models import DependencyGraph, RecordNode
from bacup_lib.workflows.asset_phases import _weapon_role_for_asset


def _weap(editor_id: str, weapon_role: str, modl: str, model_mod1: str | None = None):
    node = RecordNode(
        form_key=f"{editor_id}:000001",
        editor_id=editor_id,
        record_type="WEAP",
    )
    row = {
        "source_form_key": node.form_key,
        "editor_id": editor_id,
        "base_model": modl,
        "model_mod1": model_mod1 or "",
        "model_mod2": "",
        "model_mod3": "",
        "weapon_role": weapon_role,
    }
    return node, row


def test_weapon_role_for_asset_classifies_gun_and_combo_paths() -> None:
    weapon, row = _weap(
        "WeapNV10mmPistol",
        "gun",
        "weapons/10mm.nif",
        "weapons/10mm_extmag.nif",
    )
    orchestrator = SimpleNamespace(
        graph=DependencyGraph(root=weapon, all_records=[weapon], all_assets=[], errors=[]),
        _weapon_metadata_index={weapon.form_key: row},
    )
    assert _weapon_role_for_asset(orchestrator, "Weapons\\10mm.NIF") == "gun"
    assert _weapon_role_for_asset(orchestrator, "weapons/10mm_extmag.nif") == "gun"


def test_weapon_role_for_asset_classifies_melee() -> None:
    weapon, row = _weap("WeapNVBat", "melee", "weapons/bat.nif")
    orchestrator = SimpleNamespace(
        graph=DependencyGraph(root=weapon, all_records=[weapon], all_assets=[], errors=[]),
        _weapon_metadata_index={weapon.form_key: row},
    )
    assert _weapon_role_for_asset(orchestrator, "weapons/bat.nif") == "melee"


def test_weapon_role_for_asset_returns_none_for_unmapped_asset() -> None:
    weapon, row = _weap("WeapNV10mmPistol", "gun", "weapons/10mm.nif")
    orchestrator = SimpleNamespace(
        graph=DependencyGraph(root=weapon, all_records=[weapon], all_assets=[], errors=[]),
        _weapon_metadata_index={weapon.form_key: row},
    )
    assert _weapon_role_for_asset(orchestrator, "weapons/other.nif") is None
