from __future__ import annotations

import csv
import os
import re
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc
from tools.stub_evidence.live_vmad_probe import probe


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
PATCH_README = PATCH_ROOT / "README.md"
STATUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
PLUGIN = Path(
    os.environ.get(
        "B21_TEST_SEVENTYSIX_ESM",
        REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm",
    )
)

ROOT_REPAIRS = (
    "AliasSendStoryEventOnActivate.psc",
    "CharGenSetStageOnTriggerEnter.psc",
    "CharGenRoomDoorAliasScript.psc",
    "DefaultAliasOnTriggerLeaveA.psc",
    "DefaultChallengeMessageOnActivateAlias.psc",
    "DefaultChallengeMessageOnActivateColl.psc",
    "DefaultChallengeMessageOnActivateRef.psc",
    "DefaultCollAliasOnActivateGiveItems.psc",
    "DefaultCollectionAliasOnActivateGive.psc",
    "DefaultExplosionOnTriggerEnter.psc",
    "DefaultTriggerThrottledEventScript.psc",
    "E09B_AwardPointsOnActivate.psc",
    "E09B_DisableOnActivate.psc",
    "E09C_AliasTriggerScript.psc",
    "E09C_PlayerMoveTriggerScript.psc",
    "EN06_SpeakerTrackingTriggerScript.psc",
    "ENz09_CollectionOnActivate.psc",
    "GenericEWSmoduleTrigger.psc",
    "LC177_ActivateShutterControlsOnLoad.psc",
    "LC184TerminalEnterMisc.psc",
    "lc080_2stateactivatortriggerscript.psc",
    "MTR05_RefColSendStoryEventOnActivate.psc",
    "MTR08_ClaimTokenTerminalScript.psc",
    "MTNM03_TerminalScript.psc",
    "MTR01_DropTriggerAliasScript.psc",
    "MTR10KeypadAliasScript.psc",
    "MTRZ01_TriggerQuestScript.psc",
    "MTNM04_AddUndesirableTriggerScript.psc",
    "MTNM04_AliasTriggerScript.psc",
    "MTNS06ActivityTriggerScript.psc",
    "OverseerTerminalScript.psc",
    "Moon_Herd_AddItemOnAliasActivate.psc",
    "Moon_Herd_CollAliasSceneTriggerScript.psc",
    "NewPlayerExperience/LoadoutSelectTriggerScript.psc",
    "ReclamationDay_QTTriggersScript.psc",
    "RefColAddToUniqueQuestOnActivate.psc",
    "RS02_Beat_KickoutTriggerScript.psc",
    "SF05_Trigger.psc",
    "SSE_AddMolotovsOnActivate_Script.psc",
    "SetAVBoolOnActivateScript.psc",
    "TEMP_EN05_StartOnTriggerEnterScript.psc",
    "TEST_Trigger_PlayerMoveToRef.psc",
    "TestEMSStartQuestInstanceOnActivate.psc",
    "TestActivateChairScript.psc",
    "TestJayQuestTriggerScript.psc",
    "TestKurtButtonScript.psc",
    "testKurtActivateScript.psc",
    "TestJayTriggerEnterSpawnActor.psc",
    "ToggleEnableOnActivateRef.psc",
    "UD002TerminalAlias.psc",
    "V94_3_AtriumEnemyKillTriggerScript.psc",
    "Vatul79VentilationSetStageScript.psc",
    "Vault79GoldBotSetStageScript.psc",
    "Vault79ReactorSecurityActivateScript.psc",
    "Vault79SentryBotSetStageScript.psc",
    "Vault79SecurityTerminalScript.psc",
    "VaultSystemExploitCheckTriggerScript.psc",
    "W05_MQR_202P_DummyActivateMarker.psc",
    "W05_MQR_203P_ArenaDoorCloseLock.psc",
    "WorkshopTerminalActorValueScript.psc",
    "WorkshopRandomSwitchInputScript.psc",
    "Creatures/ScorchbeastSummonAlliesEffectScript.psc",
    "Creatures/AssaultronStealthScript.psc",
    "Expeditions/ObjModUtility/GoActiveOnTriggerEnter.psc",
    "Expeditions/XPD_Pitt02/CollectionAliasOnTriggerEnter.psc",
    "raids/EncounterStartOnActivate.psc",
    "p76_dlc01/DLC01_BabylonTerminal.psc",
    "raids/RD01/Enc01/DamagingFloorTriggerScript.psc",
    "raids/RD01/Enc01/MeleeComponentDoorTriggerScript.psc",
    "raids/RD01/Enc01/SafeRoomTriggerVolume.psc",
    "raids/RD01/Enc02/StalkerTeleportTriggerScript.psc",
    "sendstoryeventontriggerenter.psc",
    "specialloadoutsmenuactivatorscript.psc",
    "COMP_CommentTriggerScript.psc",
    "E09B_ButtonScript.psc",
    "MoMParlorEntryTriggerScript.psc",
    "MTR05_SayOnActivate.psc",
    "MTR10ShutdownButtonScript.psc",
    "SFL02_Track_TriggerAliasScript.psc",
    "TopOfTheWorldFloor3ButtonScript.psc",
    "VaultDefaultDestMultiStateActivator.psc",
    "EN06_PresEquipmentTerminalScript.psc",
    "MoMCryptosTerminalScript.psc",
    "RS03_Balance_TerminalScript.psc",
)

POST_CENSUS_COMPATIBILITY_PATCHES = (
    "CharGenPerkBoardActivatorScript.psc",
    "DefaultQuestTriggerRespawnVIPScript.psc",
    "LC101ControlTerminalScript.psc",
    "Vault79GearDoorExplosivesScript.psc",
    "Vault79KillGhoulsScript.psc",
    "Vault79ReactorDoorOpenScript.psc",
    "W05_RE_MapBoardActivatorScript.psc",
)


def _merged_source(relative_path: str) -> str:
    script_name = relative_path.removesuffix(".psc").replace("/", ":")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("relative_path", ROOT_REPAIRS)
def test_root_repair_merges_idempotently_and_compiles(relative_path: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(relative_path),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=relative_path,
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_remote_collection_repairs_register_before_handling_events():
    for relative_path in (
        "DefaultCollAliasOnActivateGiveItems.psc",
        "DefaultChallengeMessageOnActivateColl.psc",
        "ENz09_CollectionOnActivate.psc",
        "Moon_Herd_AddItemOnAliasActivate.psc",
        "Moon_Herd_CollAliasSceneTriggerScript.psc",
        "ReclamationDay_QTTriggersScript.psc",
        "RefColAddToUniqueQuestOnActivate.psc",
        "SSE_AddMolotovsOnActivate_Script.psc",
        "Expeditions/XPD_Pitt02/CollectionAliasOnTriggerEnter.psc",
        "raids/RD01/Enc01/DamagingFloorTriggerScript.psc",
        "raids/RD01/Enc01/MeleeComponentDoorTriggerScript.psc",
        "raids/RD01/Enc02/StalkerTeleportTriggerScript.psc",
    ):
        merged = _merged_source(relative_path)
        assert "RegisterForRemoteEvent" in merged
        assert "Event ObjectReference." in merged


def test_stateful_explosion_is_one_shot_and_damage_floor_is_reversible():
    explosion = _merged_source("DefaultExplosionOnTriggerEnter.psc")
    assert 'GoToState("donestate")' in explosion
    assert "Utility.RandomFloat(minTimeBetweenExplosions, maxTimeBetweenExposions)" in explosion

    damaging_floor = _merged_source(
        "raids/RD01/Enc01/DamagingFloorTriggerScript.psc"
    )
    assert "enteringActor.AddSpell(SpellToApply, False)" in damaging_floor
    assert "leavingActor.RemoveSpell(SpellToApply)" in damaging_floor


def _has_executable_member(source: str) -> bool:
    in_member = False
    member_has_body = False
    for raw_line in source.splitlines():
        line = re.sub(r";.*", "", raw_line).strip()
        if re.match(r"(?i)^(event|function|\w+(?:\[\])?\s+function)\b", line):
            in_member = True
            member_has_body = False
        elif in_member and re.match(r"(?i)^end(event|function)$", line):
            if member_has_body:
                return True
            in_member = False
        elif in_member and line:
            member_has_body = True
    return False


def test_exhaustive_root_candidate_census_has_a_bounded_disposition():
    candidates: list[str] = []
    row_pattern = re.compile(r"\| `([^`]+\.psc)` \| Unpatched candidate \|")
    for line in PATCH_README.read_text(encoding="utf-8").splitlines():
        match = row_pattern.match(line)
        if match:
            relative_path = match.group(1).replace("\\", "/")
            if not relative_path.lower().startswith(("fragments/", "quests/")):
                candidates.append(relative_path)

    classified_patch_paths = set(ROOT_REPAIRS) | set(POST_CENSUS_COMPATIBILITY_PATCHES)
    candidates = [path for path in candidates if path not in classified_patch_paths]
    candidates.extend(ROOT_REPAIRS)
    candidates.extend(POST_CENSUS_COMPATIBILITY_PATCHES)
    assert len(candidates) == 281
    names = [path.removesuffix(".psc").replace("/", ":") for path in candidates]
    live_counts = probe(PLUGIN, names)
    with STATUS.open(encoding="utf-8", newline="") as status_file:
        status_rows = {row["relative_path"].lower(): row for row in csv.DictReader(status_file)}

    dispositions = {
        "patched": [],
        "non-defect": [],
        "evidence-blocked": [],
        "unsupported-online": [],
    }
    base_parents = {
        "objectreference",
        "referencealias",
        "refcollectionalias",
        "quest",
        "terminal",
        "activemagiceffect",
    }
    root_online_contracts = {"hunterhuntedterminalscript.psc"}
    root_debug_contracts = {
        "test_lvc_vochangeonbuttonpress.psc",
        "test_vharbison_container_on_activate.psc",
    }
    root_nondefect_contracts = {
        "comp_commenttriggerquestscript.psc",
        "defaultaliastriggerentershowtutorial.psc",
        "e09a_multipleepicmutationtrigger.psc",
        "mtr04_getterminalbound.psc",
        "retriggerscript_nonappalachia.psc",
        "restrictedareatriggerscript.psc",
        "thousandwattdoorscript.psc",
        "creatures/honeybeastracescript.psc",
        "creatures/megaslothracescript.psc",
        "creatures/moleminerracescript.psc",
        "creatures/scorchedracescript.psc",
        "creatures/snallygasterracescript.psc",
        "debugstevecterminalscript.psc",
        "ewstestbutton.psc",
        "testkurtterminalscript.psc",
        "testnukecodewordterminalscript.psc",
        "warehouseencountertypesterminalscript.psc",
        "warehouseholdactorcontrolbutton.psc",
        "weapontestinggymbuttonscript.psc",
    }
    for relative_path, script_name in zip(candidates, names, strict=True):
        source = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
        status_row = status_rows.get(relative_path.lower())
        if (PATCH_ROOT / relative_path).is_file():
            disposition = "patched"
        elif live_counts[script_name] == 0 or _has_executable_member(source):
            disposition = "non-defect"
        elif relative_path.lower() in root_debug_contracts | root_nondefect_contracts:
            disposition = "non-defect"
        elif relative_path.lower() in root_online_contracts or (
            status_row and status_row["terminal_state"] == "unsupported-online"
        ):
            disposition = "unsupported-online"
        elif status_row and status_row["terminal_state"] in {
            "evidence-blocked",
            "record-dependency",
        }:
            disposition = "evidence-blocked"
        elif status_row and status_row["terminal_state"]:
            disposition = "non-defect"
        else:
            header = next(
                line.strip()
                for line in source.splitlines()
                if line.strip().lower().startswith("scriptname ")
            )
            parent_match = re.search(r"(?i)\bextends\s+([^\s]+)", header)
            parent = parent_match.group(1).lower() if parent_match else ""
            has_properties = bool(re.search(r"(?im)\bproperty\b", source))
            has_states = bool(re.search(r"(?im)^\s*state\b", source))
            if not has_properties and not has_states and parent not in base_parents:
                disposition = "non-defect"
            else:
                disposition = "evidence-blocked"
        dispositions[disposition].append(relative_path)

    assert {key: len(value) for key, value in dispositions.items()} == {
        "patched": 91,
        "non-defect": 150,
        "evidence-blocked": 32,
        "unsupported-online": 8,
    }
