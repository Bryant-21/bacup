from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

TIF_PATCH_CASES = {
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_S_0059B050_1': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00597334': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059733D': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00597340': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059898C': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059899C': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_005989AE': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D5A': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D5C': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D5D': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 20)',
        'Game.GetPlayer().AddItem(RewardRef1, 1)',
        'Game.GetPlayer().AddItem(RewardRef2, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D5F': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D61': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'Game.GetPlayer().AddItem(RewardRef1, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D62': ('Game.GetPlayer().AddItem(LL_TreasureMap, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D65': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598D67': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_00598DB2': ('Game.GetPlayer().AddItem(TestMag1, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B009': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B00A': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B00E': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B00F': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B012': ('Game.GetPlayer().AddItem(RewardRef1, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B015': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B016': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 10)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B01A': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B01C': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B01E': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B01F': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
        'Game.GetPlayer().AddItem(Addictol, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B020': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B021': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B023': ('Game.GetPlayer().AddItem(RewardRef1, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B02B': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B02D': ('Game.GetPlayer().AddItem(LL_CapsStash_Standard_Base, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B031': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B034': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B037': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
        'Game.GetPlayer().AddItem(Addictol, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B038': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B039': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 10)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B03C': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B03E': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 10)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B03F': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B041': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B045': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B047': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B04B': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B04D': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B051': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B054': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B055': ('Game.GetPlayer().AddItem(LL_Recipes_Cooking_Tasty, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B056': ('Game.GetPlayer().AddItem(LL_Recipes_Cooking_Gourmet, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B057': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B058': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
        'Game.GetPlayer().AddItem(Addictol, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B059': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B05B': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 10)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B061': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B062': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B063': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B066': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B06A': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B06F': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B076': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B078': ('Game.GetPlayer().AddItem(RewardRef1, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B079': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
        'Game.GetPlayer().AddItem(Addictol, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B07A': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B07B': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B081': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B086': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B08C': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B090': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B097': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B09A': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B09E': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0A8': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0A9': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0AE': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0B3': ('Game.GetPlayer().RemoveItem(RemoveRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0B4': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 10)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0BA': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0BF': ('ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0ED': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0EE': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'Game.GetPlayer().AddItem(W05_Denizen_LL_Chems, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0F0': (
        'Game.GetPlayer().RemoveItem(RemoveRef, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B0FB': (
        'Game.GetPlayer().RemoveItem(RemoveDrug, 1)',
        'ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()',
        'Game.GetPlayer().AddItem(Addictol, 1)',
    ),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059B114': ('(akSpeakerRef as Actor).AddToFaction(PlayerHostileFaction)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059BC92': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059BC96': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059BC97': ('Game.GetPlayer().AddItem(RewardRef, 1)',),
    'Fragments:TopicInfos:TIF_W05_DialogueDenizens_Sce_0059BC9C': (
        'Game.GetPlayer().RemoveItem(CapsRef, 20)',
        'Game.GetPlayer().AddItem(RewardRef, 1)',
    ),
}

MISC_PATCH_CASES = {
    "DenizenEffectScript": (
        "akTarget.HasKeyword(InitialQuestStartKeyword)",
        "DenizenDialogueScript ownerQuest = InitialQuest as DenizenDialogueScript",
        'DenizenDialogueScript:AliasStruct[] structs = ownerQuest.ArrayofAliasStructs as DenizenDialogueScript:AliasStruct[]',
        'structs.FindStruct("TargetActor", akTarget.GetActorBase())',
        "structs[found].DestAlias.ForceRefTo(akTarget)",
    ),
    "Fragments:Quests:QF_W05_DialogueDenizens_Scen_00597301": (
        "DenizenDialogueScript ownerQuest = Self as DenizenDialogueScript",
        "ownerQuest.RepConvoOutcomeDV = 0",
    ),
}

ALL_PATCH_CASES = {**TIF_PATCH_CASES, **MISC_PATCH_CASES}

EXPECTED_MEMBERS = {name: {"fragment_end"} for name in TIF_PATCH_CASES}
EXPECTED_MEMBERS["DenizenEffectScript"] = {"oneffectstart"}
EXPECTED_MEMBERS["Fragments:Quests:QF_W05_DialogueDenizens_Scen_00597301"] = {"fragment_stage_9000_item_00"}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_calls"), ALL_PATCH_CASES.items())
def test_denizens_patch_restores_confirmed_behavior(
    script_name: str, expected_calls: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert _member_names(patch) == EXPECTED_MEMBERS[script_name]
    for call in expected_calls:
        assert call in patch


@pytest.mark.parametrize("script_name", TIF_PATCH_CASES)
def test_denizens_tif_actions_are_none_guarded(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    action_lines = [
        line for line in patch.splitlines()
        if "Game.GetPlayer()." in line or ".AddToFaction(" in line
           or "RepConvoOutcomeDV = ownerQuest" in line
    ]
    assert action_lines
    guard_count = patch.count("!= None")
    assert guard_count >= len(action_lines)


@pytest.mark.parametrize("script_name", ALL_PATCH_CASES)
def test_denizens_patch_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_denizens_dialogue_script_itself_has_no_patch_file():
    # DenizenDialogueScript is a pure Auto-property data source consumed
    # cross-script by the TIF fragments, DenizenEffectScript, and the
    # quest stage-9000 fragment above -- Coordinator Check 2.
    assert _script_patch_source("DenizenDialogueScript") is None


def test_denizens_patch_count_matches_confirmed_batch():
    assert len(TIF_PATCH_CASES) == 87
    assert len(ALL_PATCH_CASES) == 89

