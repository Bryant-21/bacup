from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:Quests:QF_Moon_SQ01_Kieran_0068FD4A"
GENERATED_SOURCE = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "Quests"
    / "QF_Moon_SQ01_Kieran_0068FD4A.psc"
)
STAGE_MEMBERS = {
    "fragment_stage_0100_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0410_item_00",
    "fragment_stage_0420_item_00",
    "fragment_stage_0430_item_00",
    "fragment_stage_0440_item_00",
    "fragment_stage_0450_item_00",
    "fragment_stage_0475_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0610_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_9000_item_00",
}
HELPER_MEMBERS = {
    "registerforquestactors",
    "objectreference.onactivate",
    "fillquestobjectaliases",
    "fillquestobjectalias",
    "updatetoyprogress",
    "settoycountfromstages",
    "removetoy",
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name
    )
    return "\n".join(lines[start : end + 1])


def test_kieran_patch_is_member_only_and_covers_every_bound_stage_fragment():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert set(members) == STAGE_MEMBERS | HELPER_MEMBERS
    assert all(members.count(name) == 1 for name in members)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


def test_kieran_patch_preserves_objective_order_and_single_player_handoffs():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    objective_pairs = {
        "fragment_stage_0200_item_00": (100, 200),
        "fragment_stage_0300_item_00": (200, 300),
        "fragment_stage_0500_item_00": (300, 400),
        "fragment_stage_0600_item_00": (400, 500),
    }
    for member_name, (completed, displayed) in objective_pairs.items():
        body = _member_body(patch, member_name)
        assert body.index(f"SetObjectiveCompleted({completed})") < body.index(
            f"SetObjectiveDisplayed({displayed})"
        )

    dialogue = _member_body(patch, "objectreference.onactivate")
    assert "GetAlias(38)" in dialogue
    assert "GetAlias(39)" in dialogue
    assert "speaker.GetDialogueTarget() == player" in dialogue
    for stage in (200, 300, 600, 700):
        assert f"nextStage = {stage}" in dialogue

    toy_progress = _member_body(patch, "updatetoyprogress")
    assert "SetStage(475)" in toy_progress
    assert "SetStage(500)" in toy_progress
    toy_count = _member_body(patch, "settoycountfromstages")
    for stage in (410, 420, 430, 440, 450):
        assert f"IsStageDone({stage})" in toy_count


def test_kieran_patch_merges_once_and_native_compiles_for_fo4():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    skeleton = GENERATED_SOURCE.read_text(encoding="utf-8")
    merged = _merge_script_method_patches(skeleton, patch)

    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert set(members) == STAGE_MEMBERS | HELPER_MEMBERS
    assert all(members.count(name) == 1 for name in members)
    assert _merge_script_method_patches(merged, patch) == merged
    assert "actorvalue Property Moon_SQ01_Kieran_AV_KidsGotToys Auto" in merged
    assert "referencealias Property Alias_Dispenser_Truck_5 Auto mandatory" in merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Fragments/Quests/QF_Moon_SQ01_Kieran_0068FD4A.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
