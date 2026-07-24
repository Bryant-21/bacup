from __future__ import annotations

import csv
import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
CONTRACT = DOCS / "contracts" / "w05-control-script-audit.md"
TODO = DOCS / "TODO.md"
STATUS = DOCS / "status.csv"
GENERATED = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

REPORTED_SCRIPTS = {
    "W05_002P_DeathclawIsleTriggerScript",
    "W05_002P_IntroSceneTriggerScript",
    "W05_002P_RadicalHostilityTrigger",
    "W05_003P_EnterAnyTriggerRefColl",
    "W05_003P_HiddenDoorTriggerScript",
    "W05_003P_MusicOverrideTriggerScript",
    "W05_004P_Crane_DispenserTriggerScript",
    "W05_DnD_MainDoor_Script",
    "W05_MQ_001P_Wayward_LaceyIselaTrigger",
    "W05_MQ_002P_RadioTerminalScript",
    "W05_MQ_002P_StartSceneOnTriggerEnter",
    "W05_MQ_004P_Crane_DoorTriggerScript",
    "W05_MQ_004p_UpstairDoorAliasScript",
    "W05_MQ_101P_A_RepairSubTerminalScript",
    "W05_MQ_101P_A_RepairTerminalScript",
    "W05_MQA_206P_SSTalkTriggerBoxScript",
    "W05_MQR_201P_IntercomTriggerScript",
    "W05_MQR_201P_LouRoomTriggerScript",
    "W05_MQR_202P_DummyActivateMarker",
    "W05_MQR_203P_ArenaDoorCloseLock",
    "W05_MQR_203P_DoorPortalRefScript",
    "W05_MQR_203P_DoorPortalScript",
    "W05_MQR_205P_RaRaCowerTriggerScript",
    "W05_MQR_205P_SecurityTriggerScript",
    "W05_MQR_PlayerVault79KeypadObjective",
    "W05_MQR_Vault79KeypadAliasScript",
    "W05_MQS_204P_DisableOnTriggerEnter",
    "W05_OverseerCAMP_TutTriggerScript",
    "W05_QT_TriggerScript",
    "W05_RE_BlacklightActivatorScript",
    "W05_RE_ClueBoardActivatorScript",
    "W05_RE_FakeKeypadActivatorScript",
    "W05_RE_GraftonPawnShopLoadDoorScript",
    "W05_RE_MapBoardActivatorScript",
    "W05_RE_SceneZW01_TriggerScript",
    "W05_RE_V79KeypadActivatorScript",
    "W05_Vault79ElevatorDoorTriggerScript",
    "W05_Vault79EntranceDoorTriggerScript",
    "W05_Vaut79EntranceKeypadScript",
    "W05_Wayward_IntTriggerRCScript",
    "WL005_BombActivateFurnitureScript",
    "WL005_ExplodingDoorSequence01Script",
    "WL005_ExplodingDoorSequence02Script",
    "WL005_MiniQuakeOnTriggerEnterScript",
    "WL005_PlaySoundOnTriggerEnter",
    "WL006_PlayButtonSound",
    "WL006_SwapButtonScript",
    "WL036KeypadScript",
}

OMITTED_MEMBER_BEARING_SCRIPTS = {
    "W05_RE_ObjectAF01_SelfDestruct_Script",
    "W05_WaywardStateSwapRefScript",
    "WL005_DeathBoxMachineScript",
    "WL005_DeathTurretsMachineScript",
    "WL005_FallingDustLoopScript",
    "WL005_FloorCollapseSequenceScript",
    "WL006_SentryBotRevealScript",
}

REMAINING_DECLARATION_SCRIPTS = {
    "WL005_LousRadioScript",
    "WL005_PreventFallDamageScript",
    "WL006_ManageActivationLight",
}

MARKER_PATCHES = {
    "W05_MQR_201P_IntercomTriggerScript",
    "W05_MQR_205P_RaRaCowerTriggerScript",
    "W05_Vault79ElevatorDoorTriggerScript",
}

AUDIT_TODO_SCRIPTS = {
    "W05_003P_MusicOverrideTriggerScript",
    "W05_MQ_004p_UpstairDoorAliasScript",
    "W05_QT_TriggerScript",
    "W05_RE_FakeKeypadActivatorScript",
    "W05_RE_MapBoardActivatorScript",
    "W05_Vault79ElevatorDoorTriggerScript",
    "W05_Vaut79EntranceKeypadScript",
    "W05_WaywardStateSwapRefScript",
}


def _entries(path: Path, prefix: str) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith(prefix):
            continue
        fields: dict[str, str] = {}
        for token in line.split("|")[1:]:
            key, value = token.split("=", 1)
            fields[key] = value
        entries.append(fields)
    return entries


def test_manifest_covers_report_and_omitted_member_bearing_controls_exactly():
    entries = _entries(CONTRACT, "W05-AUDIT|")
    by_script = {entry["script"]: entry for entry in entries}

    assert len(entries) == 58
    assert len(by_script) == len(entries)
    assert set(by_script) == (
        REPORTED_SCRIPTS
        | OMITTED_MEMBER_BEARING_SCRIPTS
        | REMAINING_DECLARATION_SCRIPTS
    )
    assert len(REPORTED_SCRIPTS) == 48
    assert len(OMITTED_MEMBER_BEARING_SCRIPTS) == 7
    assert len(REMAINING_DECLARATION_SCRIPTS) == 3

    generated_by_name = {path.stem.lower() for path in GENERATED.glob("*.psc")}
    assert {name.lower() for name in by_script} <= generated_by_name


def test_manifest_patch_presence_and_minimal_markers_stay_synchronized():
    entries = _entries(CONTRACT, "W05-AUDIT|")
    for entry in entries:
        patch_value = entry["patch"]
        expected_marker = int(entry["marker"])
        if patch_value == "none":
            assert expected_marker == 0
            continue

        patch = REPO_ROOT / patch_value
        assert patch.is_file(), patch
        source = patch.read_text(encoding="utf-8")
        assert source.count("; TODO") == expected_marker
        assert re.search(r"^\s*(?:Function|Event)\s+", source, re.MULTILINE)

    actual_marker_scripts = {
        entry["script"]
        for entry in entries
        if entry["patch"] != "none" and int(entry["marker"]) == 1
    }
    assert actual_marker_scripts == MARKER_PATCHES


def test_deferred_registry_covers_every_new_audit_gap_without_hollow_patches():
    entries = _entries(TODO, "W05-AUDIT-TODO|")
    by_script = {entry["script"]: entry for entry in entries}
    assert set(by_script) == AUDIT_TODO_SCRIPTS

    contract_entries = {
        entry["script"]: entry for entry in _entries(CONTRACT, "W05-AUDIT|")
    }
    for script, entry in by_script.items():
        assert entry["blocker"]
        assert entry["removal"]
        assert entry["contract"] == "contracts/w05-control-script-audit.md"
        assert entry["status"] in {"evidence-blocked", "unsupported-online"}
        if entry["patch"] == "none":
            assert entry["marker"] == "0"
            assert contract_entries[script]["patch"] == "none"
        else:
            patch = REPO_ROOT / entry["patch"]
            assert patch.read_text(encoding="utf-8").count("; TODO") == 1

    todo_text = TODO.read_text(encoding="utf-8")
    for script in MARKER_PATCHES - AUDIT_TODO_SCRIPTS:
        assert f"script={script}|" in todo_text


def test_reclassified_status_rows_point_to_the_audit_contract():
    with STATUS.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"]: row for row in csv.DictReader(status_file)}

    expected = {
        "WL005_BombActivateFurnitureScript": "non-defect",
        "WL005_ExplodingDoorSequence02Script": "non-defect",
        "W05_RE_SceneZW01_TriggerScript": "non-defect",
        "W05_Vaut79EntranceKeypadScript": "evidence-blocked",
        "W05_OverseerCAMP_TutTriggerScript": "patched",
        "W05_MQ_004P_Crane_DoorTriggerScript": "patched",
        "W05_Vault79ElevatorDoorTriggerScript": "patched",
        "W05_RE_ObjectAF01_SelfDestruct_Script": "patched",
        "W05_WaywardStateSwapRefScript": "unsupported-online",
        "WL005_DeathBoxMachineScript": "non-defect",
        "WL005_DeathTurretsMachineScript": "non-defect",
        "WL005_ExplodingDoorSequence01Script": "non-defect",
        "WL005_FallingDustLoopScript": "non-defect",
        "WL005_FloorCollapseSequenceScript": "non-defect",
        "WL005_MiniQuakeOnTriggerEnterScript": "non-defect",
        "WL005_PlaySoundOnTriggerEnter": "non-defect",
        "WL006_PlayButtonSound": "non-defect",
        "WL006_SentryBotRevealScript": "non-defect",
        "WL006_SwapButtonScript": "non-defect",
        "WL036KeypadScript": "non-defect",
    }
    for script, disposition in expected.items():
        assert rows[script]["terminal_state"] == disposition
        assert rows[script]["evidence"] == "contracts/w05-control-script-audit.md"
