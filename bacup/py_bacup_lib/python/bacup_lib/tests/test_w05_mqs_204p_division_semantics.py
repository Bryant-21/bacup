from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.esp import Plugin
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SOURCE_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
TARGET_PLUGIN = REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm"
QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQS_204P_0040C458"
FAKE_WALL = "W05_MQS_204P_FakeWallScript"
PLAYER = "W05_MQS_204P_PlayerScript"
MOTHERLODE = "W05_MQS_204P_MotherlodeScript"
QUEST_CONTROLLER = "W05_MQS_204P_QuestScript"
INSTANCE_HELPER = "DefaultSetStageOnInstanceLoadQuest"
PATCHED_SCRIPTS = (QUEST_FRAGMENT, FAKE_WALL, PLAYER, MOTHERLODE)
OPEN_STAGE_ALIASES = {
    615: "Alias_FakeBarrier01",
    625: "Alias_FakeBarrier02",
    635: "Alias_FakeBarrier03",
    645: "Alias_FakeBarrier04",
    655: "Alias_FakeBarrier05",
}
FAKE_WALL_STAGES = {86: 610, 87: 620, 88: 630, 89: 640, 90: 650}
VISIBLE_BARRIER_REFS = {
    0x591925: (0x40E051, 0x40E050),
    0x591926: (0x40E052, 0x40E06C),
    0x591927: (0x40E053, 0x40E06D),
    0x591928: (0x40E054, 0x40E06E),
    0x591929: (0x40E055, 0x40E06F),
}


def _generated_source(script_name: str) -> str:
    path = GENERATED_ROOT / f"{script_name.replace(':', '/')}.psc"
    assert path.is_file(), f"generated source unavailable: {path}"
    return path.read_text(encoding="utf-8")


def _merged_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_generated_source(script_name), patch)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _vmad(record: dict) -> dict:
    return next(
        field["VirtualMachineAdapter"]
        for field in record["fields"]
        if "VirtualMachineAdapter" in field
    )


def _property_map(script: dict) -> dict[str, object]:
    return {
        item["propertyName"]: item["Value"] for item in script.get("Properties", [])
    }


def _english_string(localized: dict) -> str:
    return next(
        value["String"]
        for value in localized["Values"]
        if value["Language"] == "English"
    )


def test_source_client_pex_exposes_declarations_but_no_recoverable_members():
    for script_name in (*PATCHED_SCRIPTS, QUEST_CONTROLLER):
        path = SOURCE_PEX_ROOT / f"{script_name.replace(':', '/')}.pex"
        assert path.is_file(), f"FO76 source PEX unavailable: {path}"
        source = decompile_pex(path, fo4_api_compat=True)
        assert _member_names(source) == []


def test_live_bindings_prove_wall_categories_barriers_and_instance_stages():
    target = Plugin.load(TARGET_PLUGIN, game="fo4", lazy_index=True)
    try:
        quest = target.read_authoring_record(0x40C458)
        assert quest is not None
        vmad = _vmad(quest)

        helper = next(
            script
            for script in vmad["Scripts"]
            if script["ScriptName"] == INSTANCE_HELPER
        )
        instance_rows = _property_map(helper)["InstancedLocationData"]
        instance_stages = {}
        for row in instance_rows:
            values = {member["memberName"]: member["Value"] for member in row}
            location = values["TargetLocation"]["FormID"]["reference"]["object_id"]
            instance_stages[values["StageToSet"]] = location
        assert instance_stages == {15: "540390", 275: "55308C"}

        alias_scripts = {
            (alias["Object"]["Alias"], script["ScriptName"]): _property_map(script)
            for alias in vmad["Script Fragments"]["Aliases"]
            for script in alias.get("Alias Scripts", [])
        }
        assert {
            alias_id: alias_scripts[(alias_id, FAKE_WALL)]["StageToSet"]
            for alias_id in FAKE_WALL_STAGES
        } == FAKE_WALL_STAGES
        assert alias_scripts[(0, PLAYER)]["StageToSet"] == 800
        assert alias_scripts[(85, MOTHERLODE)]["StageToSet"] == 1000

        message = target.read_authoring_record(0x40E281)
        assert message is not None
        buttons = [
            _english_string(field["ButtonText"])
            for field in message["fields"]
            if "ButtonText" in field
        ]
        assert buttons == [
            "Leave.",
            "Hit it as hard as you can.",
            "Push the wall.",
            "Talk to the Wall.",
        ]

        location = target.read_authoring_record(0x55308C)
        assert location is not None
        special_references = next(
            field["MasterSpecialReferences"]
            for field in location["fields"]
            if "MasterSpecialReferences" in field
        )
        special_map = {
            int(
                row["MasterSpecialReferencesLocRefType"]["reference"]["object_id"],
                16,
            ): int(row["MasterSpecialReferencesRef"]["reference"]["object_id"], 16)
            for row in special_references
            if row["MasterSpecialReferencesLocRefType"]["reference"]["plugin"]
            == "SeventySix.esm"
        }
        for location_ref_type, (reference_id, base_id) in VISIBLE_BARRIER_REFS.items():
            assert special_map[location_ref_type] == reference_id
            reference = target.read_authoring_record(reference_id)
            assert reference is not None
            assert (
                next(
                    field["Base"]["reference"]["object_id"]
                    for field in reference["fields"]
                    if "Base" in field
                )
                == f"{base_id:06X}"
            )
            base = target.read_authoring_record(base_id)
            assert base is not None
            scripts = {
                script["ScriptName"].lower() for script in _vmad(base)["Scripts"]
            }
            assert scripts == {
                "b21twostateactivator76",
                "defaultrefenablelinkedrefonactivate",
            }
    finally:
        target.close()

    helper_source = _generated_source(INSTANCE_HELPER)
    for entrypoint in (
        "Event OnQuestInit()",
        "Event Actor.OnLocationChange(",
        "Event Actor.OnPlayerLoadGame(",
        "Event OnStageSet(",
    ):
        assert entrypoint in helper_source
    assert (
        "CheckPlayerInstanceLocation(playerRef.GetCurrentLocation())" in helper_source
    )
    assert "locationData.TargetLocation == playerLocation" in helper_source
    assert "SetStage(locationData.StageToSet)" in helper_source


def test_patches_preserve_proven_categories_and_open_the_animated_barriers():
    quest_patch = _script_patch_source(QUEST_FRAGMENT)
    assert quest_patch is not None
    for stage, alias_name in OPEN_STAGE_ALIASES.items():
        body = _member_body(quest_patch, f"Fragment_Stage_{stage:04d}_Item_00")
        assert f"{alias_name}.GetReference()" in body
        assert "wallRef.Activate(playerRef)" in body
        assert "Alias_FakeWall" not in body

    fake_wall = _script_patch_source(FAKE_WALL)
    assert fake_wall is not None
    body = _member_body(fake_wall, "OnActivate")
    assert "selectedAction == 1" in body
    assert "MotherlodeAggressiveChoicesMade" in body
    assert "selectedAction == 2" in body
    assert "MotherlodeStandardChoicesMade" in body
    assert "selectedAction == 3" in body
    assert "MotherlodeEmpatheticChoicesMade" in body
    for index, stage in enumerate(range(610, 660, 10), start=1):
        assert f"StageToSet == {stage}" in body
        assert body.count(f"W05_MQS_204P_FakeWallMSG_Empathetic{index:02d}.Show()") == 1

    player = _script_patch_source(PLAYER)
    assert player is not None
    player_body = _member_body(player, "OnSit")
    for counter in ("Aggressive", "Standard", "Empathetic"):
        assert f"Motherlode{counter}ChoicesMade >= 3" in player_body
    assert "W05_MQS_204P_008D_MixedResults" in player_body

    motherlode = _script_patch_source(MOTHERLODE)
    assert motherlode is not None
    motherlode_body = _member_body(motherlode, "OnActivate")
    assert "MotherlodeAggressiveChoicesMade >= 3" in motherlode_body
    assert "MotherlodeStandardChoicesMade >= 3" in motherlode_body
    assert "MotherlodeEmpatheticChoicesMade >= 3" in motherlode_body
    assert "stripped client PEX exposes no mixed-value rule" in motherlode_body
    assert "Use Standard for a 2-2-1 mixed result" in motherlode_body


@pytest.mark.parametrize("script_name", (*PATCHED_SCRIPTS, INSTANCE_HELPER))
def test_full_merged_scripts_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    source = (
        _merged_source(script_name)
        if script_name in PATCHED_SCRIPTS
        else _generated_source(script_name)
    )
    result = compile_psc(
        source,
        imports=[str(GENERATED_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
