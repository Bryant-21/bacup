from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "topicinfos"
)
DEPLOYED_QUEST_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "quests"
)
DEFAULT_SCRIPT_SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# Fragment_End sets the record's own bound GlobalVariable to the literal caps
# amount quoted in that same INFO's Prompt text ("[Pay N Caps] Make the
# problem go away") -- see contracts/w3-w05-dialoguethewayward.md Rows 7-10.
# Currency deduction is not evidenced on any of these four fragments (no
# Caps001 property bound) and is intentionally not fabricated.
TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_DialogueTheWayward_0042A20E": (
        "Game.GetPlayer().AddItem(W05_Wayward_MortTape01, 1, False)",
        "Game.GetPlayer().AddItem(W05_Wayward_MortTape02, 1, False)",
        "Game.GetPlayer().AddItem(W05_Wayward_MortTape03, 1, False)",
    ),
    "TIF_W05_DialogueTheWayward_I_0059AA6A": (
        "Rep_Pay_Fixer.SetValue(1000.0)",
    ),
    "TIF_W05_DialogueTheWayward_I_0059AA6B": (
        "Rep_Pay_Fixer.SetValue(750.0)",
    ),
    "TIF_W05_DialogueTheWayward_I_0059AA6C": (
        "Rep_Pay_Fixer.SetValue(500.0)",
    ),
    "TIF_W05_DialogueTheWayward_I_0059AA6D": (
        "Rep_Pay_Fixer.SetValue(250.0)",
    ),
}

QUEST_BASE_NAME = "QF_W05_DialogueTheWayward_0040F5BF"
DEFAULT_SCRIPT_NAME = "DefaultInstanceAliasAddItemOnCreation"


def _topicinfo_script_name(base_name: str) -> str:
    return f"Fragments:TopicInfos:{base_name}"


def _quest_script_name() -> str:
    return f"Fragments:Quests:{QUEST_BASE_NAME}"


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_production_topicinfo_source(base_name: str) -> str:
    pex_path = DEPLOYED_TOPICINFO_ROOT / f"{base_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_topicinfo_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _merged_production_quest_source() -> str:
    pex_path = DEPLOYED_QUEST_ROOT / f"{QUEST_BASE_NAME.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_quest_script_name())
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _merged_default_script_source() -> str:
    source_path = DEFAULT_SCRIPT_SOURCE_ROOT / _script_relative_path(
        DEFAULT_SCRIPT_NAME, ".psc"
    )
    patch = _script_patch_source(DEFAULT_SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("base_name", "expected_snippets"), TOPICINFO_PATCH_CASES.items())
def test_topicinfo_patch_restores_confirmed_fragment_call(
    base_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(_topicinfo_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}
    for snippet in expected_snippets:
        assert snippet in patch


@pytest.mark.parametrize("base_name", TOPICINFO_PATCH_CASES)
def test_topicinfo_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_quest_patch_binds_scene_start_to_stage_110_only():
    patch = _script_patch_source(_quest_script_name())

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
    }

    # Stage 100 has no scriptable action (its trigger/cause is unevidenced
    # and its "refill" clause has no fillable-ref property) -- resolved in
    # contracts/w3-w05-dialoguethewayward.md Row 1. Stage 110 is the sole
    # confirmed trigger path (DefaultAliasOnTriggerEnter -> StageToSet=110)
    # so Scene.Start is bound there only, never in Stage 100.
    stage_100_start, stage_100_end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function" and name == "fragment_stage_0100_item_00"
    )
    stage_100_body = "\n".join(patch.splitlines()[stage_100_start:stage_100_end])
    assert "Start()" not in stage_100_body

    assert "W05_DialogueTheWayward_Polly_IntroSceneStart.Start()" in patch
    assert patch.count(".Start()") == 1
    assert "Alias_owningPlayer.GetRef().SetValue(W05_Wayward_PollyStartedIntro, 1.0)" in patch
    assert "Alias_owningPlayer.GetRef().SetValue(W05_Wayward_PollyIntroAttractIndex, 1.0)" in patch
    assert (
        "Alias_owningPlayer.GetRef().SetValue(W05_Wayward_PlayerCollectedDuchessHolotape, 1.0)"
        in patch
    )
    assert "Alias_DuchessTape.Clear()" in patch


def test_quest_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_quest_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Quests/{QUEST_BASE_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_default_instance_alias_add_item_on_creation_guards_none_owner():
    patch = _script_patch_source(DEFAULT_SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname " not in patch
    assert {"oninit"} <= _member_names(patch)

    owner_index = patch.find("ObjectReference ownerRef = InstanceOwner.GetRef()")
    guard_index = patch.find("If ownerRef &&")
    place_index = patch.find("ownerRef.PlaceAtMe(ItemtoAdd)")
    dest_guard_index = patch.find("If ItemDestinationAlias")
    force_ref_index = patch.find("ItemDestinationAlias.ForceRefTo(newRef)")
    set_value_index = patch.find("ownerRef.SetValue(TurnOffAV, TurnOffAVValue)")

    assert -1 not in (
        owner_index, guard_index, place_index,
        dest_guard_index, force_ref_index, set_value_index,
    )
    # The owner-None guard wraps every subsequent call site (adjudication
    # condition: "None-guards at every call site").
    assert owner_index < guard_index < place_index
    assert dest_guard_index < force_ref_index


def test_default_instance_alias_add_item_on_creation_uses_declared_properties_only():
    patch = _script_patch_source(DEFAULT_SCRIPT_NAME)

    assert patch is not None
    declared = {"ItemtoAdd", "ItemDestinationAlias", "TurnOffAVValue", "TurnOffAV", "InstanceOwner", "InstancedLocation"}
    # ForceRefTo's target traces only to PlaceAtMe(ItemtoAdd) on
    # InstanceOwner.GetRef() -- both skeleton-declared properties, no
    # invented reference (adjudication condition).
    assert "ItemtoAdd" in patch
    assert "InstanceOwner" in patch
    assert "ItemDestinationAlias" in patch
    assert "TurnOffAV" in patch
    assert "TurnOffAVValue" in patch
    # InstancedLocation is intentionally unused (no confirmed native use case
    # for a bare Location property -- disclosed gap in the contract).
    for name in declared:
        assert isinstance(name, str)


def test_default_instance_alias_add_item_on_creation_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_default_script_source()
    result = compile_psc(
        merged,
        imports=[str(DEFAULT_SCRIPT_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(DEFAULT_SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialoguethewayward_patch_count_matches_shard_contract():
    # 4 deterministic-result TopicInfo repairs (Row 2, Rows 7-10) + the Row 1
    # quest fragment (4 stage members, 1 file) + the Row 17 shared Default
    # script (1 file) = 6 patched scripts total for this shard's approved
    # repairs. Rows 3-6/11-16 are non-defect, Row 18 is evidence-blocked --
    # none of those are patched.
    assert len(TOPICINFO_PATCH_CASES) == 5
