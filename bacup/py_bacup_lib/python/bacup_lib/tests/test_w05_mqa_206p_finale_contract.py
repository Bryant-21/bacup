from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQA_206P_0054EDB9"
LIVE_OR_DIE_FRAGMENT = "Fragments:Scenes:SF_W05_MQA_206P_LiveOrDie_0054EE32"
JOHNNY_STANDOFF_FRAGMENT = (
    "Fragments:Scenes:SF_W05_MQA_206P_Johnny_002_S_0054EF63"
)
CHILD_FRAGMENT = "Fragments:Quests:QF_W05_MQR_205P_A_005588EF"

FINALE_PATCHES = (
    QUEST_FRAGMENT,
    LIVE_OR_DIE_FRAGMENT,
    JOHNNY_STANDOFF_FRAGMENT,
)


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_source(script_name: str) -> str:
    return _merge_script_method_patches(
        _production_skeleton(script_name), _patch(script_name)
    )


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _executable_lines(body: str) -> list[str]:
    return [
        line.strip()
        for line in body.splitlines()[1:-1]
        if line.strip() and not line.lstrip().startswith(";")
    ]


def test_finale_patch_keeps_all_74_members_and_repairs_checkpoint_route():
    patch = _patch(QUEST_FRAGMENT)
    names = _member_names(patch)

    assert len(names) == 74
    assert set(Counter(names).values()) == {1}

    body = _member_body(patch, "fragment_stage_0100_item_00")
    assert body.count("W05_MQR_205P.IsStageDone(9000)") == 1
    assert body.count("W05_MQS_205P.IsStageDone(9000)") == 1
    assert body.count("SetStage(7)") == 1
    assert body.count("SetStage(8)") == 1
    assert body.index("W05_MQR_205P.IsStageDone(9000)") < body.index("SetStage(7)")
    assert body.index("W05_MQS_205P.IsStageDone(9000)") < body.index("SetStage(8)")


def test_currency_checkpoint_does_not_churn_the_same_bound_item():
    body = _member_body(_patch(QUEST_FRAGMENT), "fragment_stage_0085_item_00")

    guard = "playerRef && GoldBar != Gold_Bullion"
    assert guard in body
    assert body.index(guard) < body.index("playerRef.RemoveItem(Gold_Bullion")
    assert body.index(guard) < body.index("playerRef.AddItem(GoldBar")
    assert "physical gold is retained locally" in body


def test_native_completion_flag_is_the_only_completion_owner():
    body = _member_body(_patch(QUEST_FRAGMENT), "fragment_stage_9000_item_00")

    assert not any(line.lower() == "completequest()" for line in _executable_lines(body))
    assert "native CompleteQuest() flag" in body
    assert "SetObjectiveCompleted(800)" in body


def test_bound_scene_callbacks_restore_only_the_proven_transitions():
    live_or_die = _patch(LIVE_OR_DIE_FRAGMENT)
    standoff = _patch(JOHNNY_STANDOFF_FRAGMENT)

    assert _member_names(live_or_die) == ["fragment_end"]
    assert _member_names(standoff) == ["fragment_phase_02_end"]

    live_end = _member_body(live_or_die, "fragment_end")
    assert "owningQuest.IsStageDone(585)" in live_end
    assert "!owningQuest.IsStageDone(590)" in live_end
    assert live_end.count("owningQuest.SetStage(590)") == 1

    phase_end = _member_body(standoff, "fragment_phase_02_end")
    assert "owningQuest.JohnnyRobberyFurniture.GetReference()" in phase_end
    assert "owningQuest.Alias_JohnnyGoldMarker.GetReference()" in phase_end
    assert phase_end.count("robberyFurniture.MoveTo(robberyMarker)") == 1
    assert "SetStage(" not in phase_end


def test_child_lou_instance_keeps_native_completion_ownership():
    child = _patch(CHILD_FRAGMENT)
    names = _member_names(child)

    assert len(names) == 7
    assert set(Counter(names).values()) == {1}
    stage_9000 = _member_body(child, "fragment_stage_9000_item_00")
    assert not any(
        line.lower() == "completequest()" for line in _executable_lines(stage_9000)
    )


@pytest.mark.parametrize("script_name", FINALE_PATCHES)
def test_finale_production_merge_is_complete_and_idempotent(script_name: str):
    skeleton = _production_skeleton(script_name)
    patch = _patch(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    for member_name in _member_names(patch):
        assert _member_names(merged).count(member_name) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", FINALE_PATCHES)
def test_finale_full_merged_source_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
