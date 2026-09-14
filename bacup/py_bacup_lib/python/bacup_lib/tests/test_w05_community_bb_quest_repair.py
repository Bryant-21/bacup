from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CONTRACT = "contracts/w05-community-bed-and-breakfast.md"
SCRIPT_NAMES = (
    "W05_Community_BB_Quest_Script",
    "Fragments:Quests:QF_W05_Community_BB_Quest_00548A55",
    "Fragments:Scenes:SF_W05_Community_BB_Quest_Do_0054B106",
    "Fragments:Packages:PF_W05_Community_BB_TravelTo_0054E4F9",
    "Fragments:Packages:PF_W05_Community_BB_TravelTo_0054F9B2",
)


def _merged_source(script_name: str) -> str:
    source_path = (GENERATED_ROOT / Path(*script_name.split(":"))).with_suffix(
        ".psc"
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def test_bed_and_breakfast_controller_restores_the_local_quest_spine():
    merged = _merged_source(SCRIPT_NAMES[0])

    assert "RegisterForPlayerSleep()" in merged
    assert "akBed == RoomBed.GetReference()" in merged
    assert "SetStage(20)" in merged
    assert "Event ReferenceAlias.OnDeath" in merged
    assert "SetStage(80)" in merged
    assert "CannibalReachedRoom" in merged
    assert "W05_Community_BB_Quest_Door_Scene.Start()" in merged
    assert "MakeCannibalsHostile()" in merged
    assert "MakeCannibalsFlee()" in merged
    assert "SetStage(100)" in merged


def test_bed_and_breakfast_fragments_preserve_record_owned_transitions():
    quest_fragment = _merged_source(SCRIPT_NAMES[1])
    scene_fragment = _merged_source(SCRIPT_NAMES[2])
    flee_package = _merged_source(SCRIPT_NAMES[3])
    room_package = _merged_source(SCRIPT_NAMES[4])

    for stage in (10, 15, 20, 40, 60, 65, 70, 80, 100):
        assert f"HandleControllerStage({stage})" in quest_fragment
    assert (
        "(Self as Quest) as W05_Community_BB_Quest_Script" in quest_fragment
    )
    assert "Self as W05_Community_BB_Quest_Script" not in quest_fragment
    assert "Function HandleControllerStage(Int aiStage)" in quest_fragment
    assert "controller.HandleQuestStage(aiStage)" in quest_fragment
    assert "playerRef.SetValue(W05_Community_BB_Completed, 1.0)" in quest_fragment
    assert "lockedDoorRef.Lock(True)" in quest_fragment
    assert "porchDoorRef.Lock(True)" in quest_fragment
    assert "owningQuest.SetStage(40)" in scene_fragment
    assert "controller.CannibalFinishedFleeing(akActor)" in flee_package
    assert "controller.CannibalReachedRoom(akActor)" in room_package
    for merged in (flee_package, room_package):
        assert "Cannibal01Alias as ReferenceAlias" in merged
        assert "Cannibal02Alias as ReferenceAlias" in merged
        assert "Cannibal03Alias as ReferenceAlias" in merged


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_bed_and_breakfast_repairs_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(GENERATED_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_bed_and_breakfast_rows_are_patched_from_one_contract():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"].lower(): row for row in csv.DictReader(status_file)}

    for script_name in SCRIPT_NAMES:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "patched"
        assert row["evidence"] == CONTRACT
