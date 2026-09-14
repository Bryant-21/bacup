from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _fo76_to_fo4_script_type,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex, parse_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "W05_004P_CraneAliasScript"
SOURCE_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "w05_004p_cranealiasscript.pex"
)


def _production_skeleton() -> str:
    assert SOURCE_PEX.is_file(), SOURCE_PEX
    skeleton = decompile_pex(
        SOURCE_PEX,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    return _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, skeleton)


def _merged_source() -> tuple[str, str]:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch, _merge_script_method_patches(_production_skeleton(), patch)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def test_production_skeleton_preserves_crane_stage_defaults() -> None:
    skeleton = _production_skeleton()

    assert "Scriptname W05_004P_CraneAliasScript Extends ReferenceAlias" in skeleton
    assert "Int Property StagetoSetOnHit = 399 Auto" in skeleton
    assert "Int Property RegistrationStage = 102 Auto" in skeleton
    assert "ReferenceAlias Property Sol Auto mandatory" in skeleton
    assert "ReferenceAlias Property OwningPlayer Auto mandatory" in skeleton


def test_patch_uses_complete_fo4_onhit_signature_and_merges_idempotently() -> None:
    patch, merged = _merged_source()
    signature = (
        "Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, "
        "Form akSource, Projectile akProjectile, Bool abPowerAttack, "
        "Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, "
        "String apMaterial)"
    )

    assert signature in patch
    assert _member_names(patch) == ["onhit", "ondeath"]
    assert _member_names(merged).count("onhit") == 1
    assert _member_names(merged).count("ondeath") == 1
    assert "Int Property StagetoSetOnHit = 399 Auto" in merged
    assert "Int Property RegistrationStage = 102 Auto" in merged
    assert _merge_script_method_patches(merged, patch) == merged


def test_full_production_merge_native_compiles_and_delivers_pex(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    _patch, merged = _merged_source()

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None

    delivered_pex = tmp_path / f"{SCRIPT_NAME}.pex"
    delivered_pex.write_bytes(result.pex_bytes)
    functions = {
        function.name.casefold(): function
        for obj in parse_pex(delivered_pex).objects
        for state in obj.states
        for function in state.functions
    }
    on_hit = functions["onhit"]
    assert [parameter.type.casefold() for parameter in on_hit.params] == [
        "objectreference",
        "objectreference",
        "form",
        "projectile",
        "bool",
        "bool",
        "bool",
        "bool",
        "string",
    ]
