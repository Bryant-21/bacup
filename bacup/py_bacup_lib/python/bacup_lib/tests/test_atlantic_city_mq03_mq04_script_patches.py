from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_CASES = (
    "Fragments:Quests:QF_AC_MQ03_HonorBound_00732A5B",
    "Quests:AC_MQ03_HonorBound:QuestScript",
    "Fragments:Packages:PF_AC_MQ03_HonorBound_MuniTr_007456E3",
    "Fragments:Quests:QF_AC_MQ04_Sins_006F7BE3",
    "AC_MQ04_Sins_QuestScript",
    "Quests:AC_MQ04_SinsOfTheFather:PlayerScript",
    "Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript",
)


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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, header: str, end_keyword: str = "EndFunction") -> str:
    start = source.find(header)
    assert start != -1, f"{header!r} not found"
    end = source.find(end_keyword, start)
    assert end != -1, f"{end_keyword!r} not found after {header!r}"
    return source[start:end]


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_mq03_mq04_patches_merge_idempotently_and_follow_conventions(
    script_name: str,
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)

    merged = _merge_script_method_patches(skeleton, patch)
    assert merged.lower().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_mq03_covers_every_fragment_and_restores_progression_handoff():
    source = _merged_source(
        "Fragments:Quests:QF_AC_MQ03_HonorBound_00732A5B"
    )
    assert source.count("Function Fragment_Stage_") == 51
    assert "EnableCollection(Alias_Actors_OvergrownAmbush_Enemies, True)" in source
    assert "EnableCollection(Alias_Actors_LostMunis)" in source
    assert "controller.BeginMuniRescue()" in source
    assert "controller.BeginBackDoorPuzzle()" in source
    assert "controller.BeginValvePuzzle()" in source
    assert "GiveWarehouseKeyIfMissing()" in source
    assert "EnableCollection(Alias_Actors_ChemLabEnemies, True)" in source
    assert "ChemLabEntranceExplosion" in source

    dialogue_caps = _member_body(
        source, "Function Fragment_Stage_0250_Item_00()"
    )
    assert "player.AddItem(Caps001, VinCapsAmt" in dialogue_caps
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    story_event = "AC_MQ04_Sins_StartKeyword.SendStoryEvent(None, player, player)"
    assert completion.count(story_event) == 1
    assert "AddItem(" not in completion
    assert "Caps001" not in completion


def test_mq03_root_restores_munis_four_knocks_and_bound_valve_order():
    source = _merged_source("Quests:AC_MQ03_HonorBound:QuestScript")
    valve = _member_body(source, "Function HandleValveActivation(")
    assert "ValveActivationList[ValvesActivatedCounter]" in valve
    assert "ValvesActivatedCounter += 1" in valve
    assert "ValvesActivatedCounter = 0" in valve
    assert "SetStage(ChemLabFloodedStage)" in valve
    door = _member_body(source, "Function HandleBackDoorActivation()")
    assert "DoorActivationCount += 1" in door
    assert "DoorActivationCount >= NumTimesToActivateDoor" in door
    assert "SetStage(OpenedBackDoorStage)" in door
    muni = _member_body(source, "Function HelpMuni(")
    assert "RemoveKeyword(AC_MQ03_HonorBound_MuniDowned_Keyword)" in muni
    assert "MunisHelpedCount += 1" in muni
    assert "SetStage(MunisHelpedStage)" in muni


def test_mq03_vanish_package_disables_actor_and_other_shells_stay_unpatched():
    source = _merged_source(
        "Fragments:Packages:PF_AC_MQ03_HonorBound_MuniTr_007456E3"
    )
    fragment = _member_body(source, "Function Fragment_End(")
    assert "akActor.Disable()" in fragment
    assert (
        _script_patch_source(
            "Fragments:Packages:PF_AC_MQ03_HonorBound_GeneMa_007452BE"
        )
        is None
    )
    assert (
        _script_patch_source(
            "Fragments:Packages:PF_AC_MQ04_AntonioPostGeneCo_006F9A01"
        )
        is None
    )


def test_mq04_covers_every_fragment_pheromones_boss_blood_and_endings():
    source = _merged_source("Fragments:Quests:QF_AC_MQ04_Sins_006F7BE3")
    assert source.count("Function Fragment_Stage_") == 68
    for stage in (150, 152, 154, 156, 158):
        fragment = _member_body(
            source, f"Function Fragment_Stage_{stage:04d}_Item_00()"
        )
        assert f", {stage})" in fragment
    boss = _member_body(source, "Function Fragment_Stage_0170_Item_00()")
    assert "controller.BeginJerseyDevilFight()" in boss
    downed = _member_body(source, "Function Fragment_Stage_0180_Item_00()")
    assert "controller.DownJerseyDevil()" in downed
    blood = _member_body(source, "Function Fragment_Stage_0190_Item_00()")
    assert "player.PlayIdleAction(ActionExtractBlood" in blood
    assert "GiveIfMissing(DevilsBloodVial)" in blood
    choice = _member_body(source, "Function Fragment_Stage_0281_Item_00()")
    assert "SetStage(300)" in choice
    assert "SetStage(320)" in choice
    assert "SetStage(340)" in choice
    finale_presence = _member_body(source, "Function SetFinaleActorPresence()")
    assert "IsStageDone(300)" in finale_presence
    assert "IsStageDone(320)" in finale_presence
    assert "abbieMansion.SetValue(AbbieAway, 1.0)" in finale_presence
    finale = _member_body(source, "Function Fragment_Stage_0430_Item_00()")
    assert "AC_MQ04_Sins_RoseRoomFinale_VinsEnding" in finale
    assert "SetStage(9000)" not in finale


def test_mq04_completion_only_cleans_up_and_does_not_duplicate_rewards():
    source = _merged_source("Fragments:Quests:QF_AC_MQ04_Sins_006F7BE3")
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    assert "CompleteAllObjectives()" in completion
    assert "controller.CleanupQuest()" in completion
    assert "AddItem(" not in completion
    assert "Caps001" not in completion
    assert "SendStoryEvent" not in completion


def test_mq04_root_rebuilds_pheromones_and_owns_boss_lifecycle():
    source = _merged_source("AC_MQ04_Sins_QuestScript")
    rebuild = _member_body(source, "Function RebuildPheromoneCounter()")
    for stage in (150, 152, 154, 156, 158):
        assert f"IsStageDone({stage})" in rebuild
    record = _member_body(source, "Function RecordPheromonePlaced(")
    assert "pheromoneCounter >= pheromoneTotal" in record
    assert "SetStage(pheromonesPlacedStage)" in record
    assert "controller.PrepareForFight()" in source
    assert "controller.EnterDownedState()" in source
    assert "controller.ReleaseAfterHarvest()" in source
    assert "controller.CleanupBoss()" in source


def test_mq04_boss_has_combat_downed_exit_and_cleanup_contracts():
    source = _merged_source(
        "Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript"
    )
    combat = _member_body(source, "Function PrepareForFight()")
    assert 'GoToState("jerseydevilcombat")' in combat
    assert "boss.AddKeyword(JerseyDevilCannotBePacified)" in combat
    downed = _member_body(source, "Function EnterDownedState()")
    assert "boss.PlayIdle(JerseyDevil_BleedOut_Enter)" in downed
    assert "AC_MQ04_Sins.SetStage(180)" in downed
    release = _member_body(source, "Function ReleaseAfterHarvest()")
    assert "boss.PlayIdle(JerseyDevil_BleedOut_Exit)" in release
    animation = _member_body(source, "Event OnAnimationEvent(", "EndEvent")
    assert "boss.PlaceAtMe(JerseyDevilExitVFX)" in animation
    assert "boss.Disable()" in animation


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_mq03_mq04_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
