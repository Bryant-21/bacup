from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Fragments:Quests:QF_COMP_Quest_Outro_Full_Bec_005A05DE"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "beckett-second-half-finale-repair-2026-09-03.md"
)
FRAGMENTS = {
    "fragment_stage_0001_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0350_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0650_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0705_item_00",
    "fragment_stage_0710_item_00",
    "fragment_stage_0720_item_00",
    "fragment_stage_0750_item_00",
    "fragment_stage_0755_item_00",
    "fragment_stage_0760_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_9999_item_00",
}


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


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        SCRIPT_NAME, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


def _body(source: str, member: str) -> str:
    return source.split(f"Function {member}()", 1)[1].split("EndFunction", 1)[0]


def test_all_21_bound_fragments_merge_once_and_idempotently() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "scriptname " not in patch.lower()
    assert FRAGMENTS <= set(_member_names(patch))

    merged = _merged()
    assert merged.lower().count("scriptname ") == 1
    for fragment in FRAGMENTS:
        assert _member_names(merged).count(fragment) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_finale_reaches_each_record_owned_transition_without_direct_start() -> None:
    source = _merged()

    assert "SetStage(100)" in _body(source, "Fragment_Stage_0001_Item_00")
    assert "COMP_Quest_Outro_Full_Beckett_RonnyLeaves" in _body(
        source, "Fragment_Stage_0300_Item_00"
    )
    assert "pCOMP_Quest_Outro_Full_Beckett_Confrontation" in _body(
        source, "Fragment_Stage_0650_Item_00"
    )
    assert "pCOMP_Quest_Outro_Full_Beckett_KillFrankie" in _body(
        source, "Fragment_Stage_0705_Item_00"
    )
    assert "COMP_Quest_Outro_Full_Beckett_SaveFrankie" in _body(
        source, "Fragment_Stage_0710_Item_00"
    )
    assert "pCOMP_Quest_Outro_Full_Beckett_FrankieDead" in _body(
        source, "Fragment_Stage_0720_Item_00"
    )
    assert "pCOMP_Quest_Outro_Full_Beckett_FrankieLives" in _body(
        source, "Fragment_Stage_0750_Item_00"
    )
    assert "SetProtected(False)" in _body(
        source, "Fragment_Stage_0755_Item_00"
    )
    assert "pCOMP_Quest_Camp_Full_Beckett.SetStage(9000)" in _body(
        source, "Fragment_Stage_0900_Item_00"
    )
    camp_return = _body(source, "Fragment_Stage_0900_Item_00")
    assert (
        "pCOMP_Quest_Camp_Full_Beckett.GetAlias(1) as ReferenceAlias"
        in camp_return
    )
    assert "If campBeckettAlias" in camp_return
    assert "campBeckettAlias.GetActorReference()" in camp_return
    assert "Alias_BeckettAtCAMP.ForceRefIfEmpty(campBeckett)" in camp_return
    assert "Alias_Beckett.GetActorReference()" not in camp_return
    assert "ForceRefTo" not in camp_return
    completion = _body(source, "Fragment_Stage_1000_Item_00")
    assert "player.SetValue(pCOMP_AV_Beckett_FinaleComplete, 1.0)" in completion
    assert "pCOMP_Quest_Camp_Full_Beckett.SetStage(9100)" in completion
    assert "pCOMP_Quest_Camp_Full_Beckett.SetStage(9999)" in completion
    assert completion.find("SetStage(9100)") < completion.find("SetStage(9999)")
    assert "CompleteQuest()" in completion
    assert "SetStage(9999)" in completion
    assert ".Start()" not in source.split("Actor Function GetPlayerActor", 1)[0].replace(
        "targetScene.Start()", ""
    )


def test_repeat_delivery_guards_key_spawns_scenes_and_shutdown() -> None:
    source = _merged()

    assert "player.GetItemCount(pWU_GarageAKey) == 0" in source
    assert "Alias_RaiderAllies.GetCount() > 0" in source
    assert "!targetScene.IsPlaying()" in source
    assert "!IsStageDone(9999)" in source
    assert "If !IsStopped()" in source
    assert "Alias_BeckettAtCAMP.GetReference() == None" in source
    assert "!pCOMP_Quest_Camp_Full_Beckett.IsStageDone(9999)" in source


def test_camp_completion_stage_owns_caps_and_completion_xp() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "B21:QuestRewards` with `CapsStages=[9999]`" in contract
    assert "QuestCompletionXP" in contract
    assert "stage `9999` carries `CompleteQuest`" in contract


def test_second_half_titles_map_to_specific_alias_quests() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")
    mappings = {
        "00596713": "00582165 COMP_RQ_Kill_SpecificAliases_Beckett_005_Blood",
        "0059670B": "0058215A COMP_RQ_Rescue_SpecificAliases_Beckett_006_Pet",
        "0059670A": "0058215E COMP_RQ_Kill_SpecificAliases_Beckett_007_DJ",
        "00596709": "0058216A COMP_RQ_Rescue_SpecificAliases_Beckett_008_MissNanny",
        "00596708": "00582168 COMP_RQ_Fetch_SpecificAliases_Beckett_009_Holotapes",
        "00596707": "0058215F COMP_RQ_Fetch_SpecificAliases_Beckett_010_PoisonedFood",
        "00596712": "0058215D COMP_RQ_Kill_SpecificAliases_Beckett_011_Eye",
    }
    for title_form_id, quest in mappings.items():
        assert title_form_id in contract
        assert quest in contract
    assert "No per-title fragment is added" in contract


def test_full_merged_finale_compiles_for_fo4(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
