from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


SCRIPT_NAME = "W05_QT_TriggerScript"
REPO_ROOT = Path(__file__).resolve().parents[5]
SKELETON_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / f"{SCRIPT_NAME}.psc"
)

CHAIN_BINDINGS = {
    "W05_MQ_102P": (
        (58, 57, (), "54E495", "54E493"),
        (59, 57, (58,), "54E496", "54E497"),
        (60, 57, (58, 59), "54E499", "54E498"),
        (62, 57, (58, 59, 60), "54E49E", "54E49D"),
        (64, 63, (), "54E4A8", "54E4A9"),
        (65, 63, (64,), "54E4AB", "54E4AA"),
        (66, 63, (64, 65), "54E4AD", "54E4AC"),
        (67, 63, (64, 65, 66), "54E4AF", "54E4AE"),
        (68, 63, (64, 65, 66, 67), "54E4B2", "54E4B3"),
        (71, 70, (), "54ED5C", "54ED5D"),
        (72, 70, (71,), "54ED5E", "4004A1"),
    ),
    "W05_MQS_201P": (
        (88, 87, (), "5A12B1", "5A12B5"),
        (90, 87, (88,), "5A12B2", "5A12B6"),
        (91, 87, (88, 90), "5A12B7", "5A12B3"),
    ),
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(
        SKELETON_PATH.read_text(encoding="utf-8"), patch
    )


def test_live_vmad_inventory_covers_all_14_ordered_breadcrumb_bindings():
    bindings = [binding for quest in CHAIN_BINDINGS.values() for binding in quest]

    assert len(bindings) == 14
    assert len({(quest, binding[0]) for quest, chain in CHAIN_BINDINGS.items() for binding in chain}) == 14
    assert len({binding[3] for binding in bindings}) == 14
    assert len({binding[4] for binding in bindings}) == 14

    for chain in CHAIN_BINDINGS.values():
        aliases_by_target: dict[int, list[int]] = {}
        for alias, current_target, previous, _trigger_ref, _linked_target in chain:
            seen = aliases_by_target.setdefault(current_target, [])
            assert previous == tuple(seen)
            seen.append(alias)


def test_patch_is_a_single_exact_player_trigger_member():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert _member_names(patch) == ["ontriggerenter"]
    assert patch.splitlines()[0] == "Event OnTriggerEnter(ObjectReference akActionRef)"
    assert "Scriptname " not in patch
    assert " Property " not in patch
    assert "State " not in patch


def test_patch_forces_the_unkeyed_linked_target_before_ordered_retirement():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    player_guard = patch.index("akActionRef != Game.GetPlayer()")
    linked_target = patch.index("triggerRef.GetLinkedRef()")
    force_target = patch.index("CurrentTarget.ForceRefTo(linkedTarget)")
    optional_history_guard = patch.index("PreviousTriggerVolumes == None")
    loop = patch.index("While index < PreviousTriggerVolumes.Length")
    retire = patch.index("previousTrigger.TryToDisableNoWait()")

    assert (
        player_guard
        < linked_target
        < force_target
        < optional_history_guard
        < loop
        < retire
    )
    assert "GetLinkedRef(" in patch
    assert "GetLinkedRef()" in patch
    assert "SetStage(" not in patch
    assert "Start()" not in patch


def test_patch_guards_missing_bindings_and_is_repeat_safe():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "triggerRef == None || CurrentTarget == None" in patch
    assert "linkedTarget == None" in patch
    assert "PreviousTriggerVolumes == None" in patch
    assert "previousTrigger != None" in patch
    assert "previousTriggerRef != None && !previousTriggerRef.IsDisabled()" in patch
    assert "CurrentTarget.GetRef() != linkedTarget" in patch
    assert patch.index("CurrentTarget.GetRef() != linkedTarget") < patch.index(
        "CurrentTarget.ForceRefTo(linkedTarget)"
    )
    assert patch.index(
        "previousTriggerRef != None && !previousTriggerRef.IsDisabled()"
    ) < patch.index("previousTrigger.TryToDisableNoWait()")
    assert patch.count("CurrentTarget.ForceRefTo(linkedTarget)") == 1
    assert patch.count("previousTrigger.TryToDisableNoWait()") == 1
    assert "GetRef().Disable" not in patch
    assert "Self.Disable" not in patch


def test_patch_merges_once_idempotently_into_the_full_production_skeleton():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    skeleton = SKELETON_PATH.read_text(encoding="utf-8")
    merged = _merge_script_method_patches(skeleton, patch)

    assert _member_names(skeleton) == []
    assert _member_names(merged) == ["ontriggerenter"]
    assert merged.count("ReferenceAlias Property CurrentTarget Auto mandatory") == 1
    assert merged.count("referencealias[] Property PreviousTriggerVolumes Auto") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_full_production_skeleton_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)

    assert result.ok, diagnostics
    assert result.pex_bytes is not None
