from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import parse_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Fragments:Quests:QF_AC_MQ01_Opportunity_006C1F50"
SOURCE_PATH = (
    SOURCE_ROOT
    / "fragments"
    / "Quests"
    / "QF_AC_MQ01_Opportunity_006C1F50.psc"
)
CONTROLLER_HEADER = (
    "Quests:AC_MQ01_Opportunity:QuestScript Function QuestController()"
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


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(
        SOURCE_PATH.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, header: str) -> str:
    start = source.find(header)
    assert start != -1, f"{header!r} not found"
    end = source.find("EndFunction", start)
    assert end != -1, f"EndFunction not found after {header!r}"
    return source[start:end]


def test_namespaced_controller_helper_is_delivered_once_and_idempotently():
    skeleton = SOURCE_PATH.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    merged = _merge_script_method_patches(skeleton, patch)

    assert merged.count(CONTROLLER_HEADER) == 1
    assert merged.count("Function Fragment_Stage_") == 27
    assert merged.count("Function ") == 34
    assert merged.count("QuestController()") == 5
    assert _merge_script_method_patches(merged, patch) == merged


def test_missing_controller_aborts_before_the_ambush_hostility_switch():
    source = _merged_source()
    stage_600 = _member_body(source, "Function Fragment_Stage_0600_Item_00()")

    assert stage_600.index("QuestController()") < stage_600.index(
        "SetHitmenCombatState(True)"
    )
    assert "controller.EndPerformanceDucking()" in stage_600

    hostile = _member_body(source, "Function SetHitmenCombatState(Bool hostile)")
    assert "Alias_Actors_AllHitmen.GetCount()" in hostile
    assert "hitman.SetGhost(!hostile)" in hostile
    assert "hitman.RemoveFromFaction(CaptiveFaction)" in hostile
    assert "hitman.AddToFaction(AC_MQ01_Opportunity_EnemyFaction)" in hostile


def test_hitman_death_and_survivor_bleedout_stages_converge_on_interrogation():
    source = _merged_source()
    finish = _member_body(source, "Function TryFinishHitmanFight()")
    assert "IsStageDone(670)" in finish
    assert "IsStageDone(680)" in finish
    assert "IsStageDone(690)" in finish
    assert "SetStage(700)" in finish

    for stage in (670, 680, 690):
        callback = _member_body(
            source, f"Function Fragment_Stage_0{stage}_Item_00()"
        )
        assert callback.count("TryFinishHitmanFight()") == 1

    interrogation = _member_body(
        source, "Function Fragment_Stage_0700_Item_00()"
    )
    assert "SetObjectiveCompleted(60)" in interrogation
    assert "SetObjectiveDisplayed(70)" in interrogation
    assert "leader.StopCombat()" in interrogation
    assert "leader.RemoveFromFaction(AC_MQ01_Opportunity_EnemyFaction)" in interrogation
    assert "leader.AddToFaction(CaptiveFaction)" in interrogation
    assert "AC_MQ01_Opportunity_AfterFightCommentary" in interrogation


def test_full_merged_quest_fragment_compiles_with_controller_member(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="fragments/Quests/QF_AC_MQ01_Opportunity_006C1F50.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None

    pex_path = tmp_path / "QF_AC_MQ01_Opportunity_006C1F50.pex"
    pex_path.write_bytes(result.pex_bytes)
    pex = parse_pex(pex_path)
    members = [
        str(function.name).lower()
        for obj in pex.objects
        for state in obj.states
        for function in state.functions
    ]
    assert len(members) == 34
    assert members.count("questcontroller") == 1
