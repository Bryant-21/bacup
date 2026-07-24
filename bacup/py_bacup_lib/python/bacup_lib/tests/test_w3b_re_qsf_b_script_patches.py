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

# Section A -- Camp/CampAF discovery-tableau family, B1 shape (9 rows): stage 10
# does the clutter-marker toggle AND scene-start bundled (no separate native
# trigger); stage 1000 is an evidenced-empty terminal (contracts/w3b-re-qsf-b.md
# Section A).
CAMP_B1_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_Camp_JP32_MobCamp__0058CEBE": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP33_MobCamp__0058CEBF": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF01_00562BA5": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF04_005913EF": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF05_0059192E": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF06_00591931": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF07_00591935": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF08_00591937": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_CampAF09_0059193A": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
}
CAMP_B1_MEMBERS = {"fragment_stage_0010_item_00", "fragment_stage_1000_item_00"}

# Camp_JP34_ReturningCamper -- B1-shaped stage 10, plus an extra native-triggered
# (out-of-shard trigger source) stage 15 scene-start.
CAMP_JP34_SCRIPT = "Fragments:Quests:QF_W05_RE_Camp_JP34_Returnin_0058D3B4"
CAMP_JP34_CALLS = (
    "Alias_ClutterMarkerEnable.GetReference().Enable()",
    "Alias_ClutterMarkerDisable.GetReference().Disable()",
    "StartDialogue.Start()",
    "ChatScene.Start()",
)
CAMP_JP34_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_0015_item_00",
    "fragment_stage_1000_item_00",
}

# B2/B3 shape (3 rows): stage 10 does the clutter-marker toggle ONLY; a separate
# native-`RangeCheckStage`-triggered stage does the scene-start (and, for CampAF02,
# the adjudicated "Kill Patient 2" DamageValue call).
CAMP_B2B3_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_CampAF02_0056385E": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "W05_RE_CampAF02_DoctorAnims.Start()",
        "Alias_Patient02.GetActorReference()",
        "patientActor.DamageValue(Health, patientActor.GetValue(Health))",
    ),
    # NOTE: the live VMAD fragment ScriptName for this row is
    # "qf_w05_re_camptemplate01_..." (matching the generated skeleton filename),
    # NOT the quest's current eid W05_RE_CampAF03 -- fragment script names are
    # derived from the record's original identifier at fragment-creation time and
    # do not follow later eid renames (contract's disclosed naming-vs-content
    # mismatch, Section A).
    "Fragments:Quests:QF_W05_RE_CampTemplate01_00562281": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    # Same naming-vs-content mismatch: live ScriptName is "..._jp07_camp_junkdeal_...",
    # not the quest's live Name "Blood Eagle Camp".
    "Fragments:Quests:QF_W05_RE_JP07_Camp_JunkDeal_00569CC7": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
}
CAMP_B2B3_MEMBERS: dict[str, set[str]] = {
    "Fragments:Quests:QF_W05_RE_CampAF02_0056385E": {
        "fragment_stage_0010_item_00", "fragment_stage_0015_item_00", "fragment_stage_1000_item_00",
    },
    "Fragments:Quests:QF_W05_RE_CampTemplate01_00562281": {
        "fragment_stage_0010_item_00", "fragment_stage_0012_item_00", "fragment_stage_1000_item_00",
    },
    "Fragments:Quests:QF_W05_RE_JP07_Camp_JunkDeal_00569CC7": {
        "fragment_stage_0010_item_00", "fragment_stage_0015_item_00", "fragment_stage_1000_item_00",
    },
}

# Section B -- Mining family (4 rows): stage 10 enables the claim marker; stages
# 20/40/50/1000 are native-triggered, evidenced-empty (no REAssaultQuestScript
# sibling on this shape -- contracts/w3b-re-qsf-b.md Section B).
MINING_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_MiningJM01_005600E7": ("Alias_EnableMarker.GetReference().Enable()",),
    "Fragments:Quests:QF_W05_RE_MiningJM02_00562B57": ("Alias_EnableMarker.GetReference().Enable()",),
    "Fragments:Quests:QF_W05_RE_MiningJM03_005644DA": ("Alias_EnableMarker.GetReference().Enable()",),
    "Fragments:Quests:QF_W05_RE_MiningJM04_00564A28": ("Alias_EnableMarker.GetReference().Enable()",),
}
MINING_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_0020_item_00",
    "fragment_stage_0040_item_00",
    "fragment_stage_0050_item_00",
    "fragment_stage_1000_item_00",
}

# Section C -- Object family (4 rows, high complexity).
OBJECT_JP01_SCRIPT = "Fragments:Quests:QF_W05_RE_Object_JP01_0056D1D8"
OBJECT_JP01_CALLS = (
    "SandboxScene.Start()",
    "DialogueScene.Start()",
    "CaptiveSitScene.Start()",
    "captive01.RemoveItem(HeadSacRef, 1, true)",
    "captive02.RemoveItem(HeadSacRef, 1, true)",
    "captive03.RemoveItem(HeadSacRef, 1, true)",
)
OBJECT_JP01_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0019_item_00", "fragment_stage_0020_item_00",
    "fragment_stage_0030_item_00", "fragment_stage_0040_item_00", "fragment_stage_0050_item_00",
    "fragment_stage_1000_item_00",
}

OBJECTAF01_SCRIPT = "Fragments:Quests:QF_W05_RE_ObjectAF01_0056A1D1"
OBJECTAF01_CALLS = (
    "W05_RE_ObjectAF01_Sandbox.Start()",
    "W05_RE_ObjectAF01_Attack.Start()",
    "W05_RE_ObjectAF01_Protectron_Destruct.Cast(protRef, protRef)",
    "W05_RE_ObjectAF01_Explosion.Start()",
    "protRef.RemoveKeyword(W05_RE_ObjectAF01_BrokenProtectronKeyword)",
)
OBJECTAF01_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0023_item_00", "fragment_stage_0025_item_00",
    "fragment_stage_0030_item_00", "fragment_stage_1000_item_00",
}

OBJECTBB01_SCRIPT = "Fragments:Quests:QF_W05_RE_ObjectBB01_00568E56"
OBJECTBB01_CALLS = ("W05_RE_ObjectBB01_Response.Start()",)
OBJECTBB01_MEMBERS = {"fragment_stage_0100_item_00", "fragment_stage_1000_item_00"}

OBJECTBB02_SCRIPT = "Fragments:Quests:QF_W05_RE_ObjectBB02_0056F038"
OBJECTBB02_CALLS = (
    "W05_RE_ObjectBB02_ChechStash.Start()",
    "W05_RE_ObjectBB02_Charge.Start()",
    "W05_RE_ObjectBB02_GetHappy.Start()",
    "W05_RE_ObjectBB02_GetMad.Start()",
)
OBJECTBB02_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0020_item_00", "fragment_stage_0100_item_00",
    "fragment_stage_0110_item_00", "fragment_stage_0200_item_00", "fragment_stage_0210_item_00",
    "fragment_stage_0250_item_00", "fragment_stage_0260_item_00", "fragment_stage_0270_item_00",
    "fragment_stage_0400_item_00", "fragment_stage_1000_item_00",
}

# Section D -- Scene_JP04 rescue/bait pair, TravelersJM family, and the
# SceneAF04/SceneZW01/SceneZW02 rows.
SCENE_JP04_A_SCRIPT = "Fragments:Quests:QF_W05_RE_Scene_JP04_A_005637BC"
SCENE_JP04_A_CALLS = (
    "RunSceneRef.Start()",
    "RealSceneProp.Start()",
    "settlerRef.RemoveItem(HeadSacRef, 1, true)",
)
SCENE_JP04_A_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0011_item_00", "fragment_stage_0019_item_00",
    "fragment_stage_0020_item_00", "fragment_stage_1000_item_00",
}

SCENE_JP04_B_SCRIPT = "Fragments:Quests:QF_W05_RE_Scene_JP04_B_005637BA"
SCENE_JP04_B_CALLS = (
    "SceneRef.Start()",
    "baitRef.RemoveItem(HeadSacRef, 1, true)",
)
SCENE_JP04_B_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0011_item_00", "fragment_stage_0013_item_00",
    "fragment_stage_0015_item_00", "fragment_stage_1000_item_00",
}

# TravelersJM02-05 -- uniform shape, only Alias_TRIGGER bound, stage 1000 only,
# evidenced-empty terminal (native controller sibling out-of-shard). NOTE: the
# live VMAD fragment ScriptName truncates to "...travelersjm0_<form_id>" on all 4
# rows (matching the generated skeleton filename) -- it does NOT carry the
# specific JM02/JM03/JM04/JM05 suffix despite each row's own eid doing so.
TRAVELERSJM_SCRIPTS = (
    "Fragments:Quests:QF_W05_RE_Scene_TravelersJM0_0056EC33",  # JM02
    "Fragments:Quests:QF_W05_RE_Scene_TravelersJM0_00571CD7",  # JM03
    "Fragments:Quests:QF_W05_RE_Scene_TravelersJM0_00587BCB",  # JM04
    "Fragments:Quests:QF_W05_RE_Scene_TravelersJM0_00587BCA",  # JM05
)
EMPTY_TERMINAL_ONLY_MEMBERS = {"fragment_stage_1000_item_00"}

SCENEAF04_SCRIPT = "Fragments:Quests:QF_W05_RE_SceneAF04_005849E2"

SCENEZW01_SCRIPT = "Fragments:Quests:QF_W05_RE_SceneZW01_0056368E"
SCENEZW01_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0020_item_00", "fragment_stage_1000_item_00",
}

SCENEZW02_SCRIPT = "Fragments:Quests:QF_W05_RE_SceneZW02_005655E4"
SCENEZW02_CALLS = ("MoveScene.Start()", "DeadScene.Start()")
SCENEZW02_MEMBERS = {
    "fragment_stage_0020_item_00", "fragment_stage_0040_item_00", "fragment_stage_1000_item_00",
}

# Section E -- Travel family (7 rows).
TRAVELAF01_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelAF01_00567A72"

TRAVELAF02_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelAF02_0056A1D0"
TRAVELAF02_CALLS = ("W05_RE_TravelAF02_KillActor.Start()",)
TRAVELAF02_MEMBERS = {"fragment_stage_0020_item_00", "fragment_stage_1000_item_00"}

TRAVELBB01_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelBB01_0056F039"
TRAVELBB01_CALLS = (
    "W05_RE_TravelBB01_NotInCombatScene.Start()",
    "W05_RE_TravelBB01_InCombatScene.Start()",
    "W05_RE_TravelBB01_SearchingScene.Start()",
    "DeadScene.Start()",
)
TRAVELBB01_MEMBERS = {
    "fragment_stage_0100_item_00", "fragment_stage_0200_item_00", "fragment_stage_0300_item_00",
    "fragment_stage_0400_item_00", "fragment_stage_1000_item_00",
}

TRAVELBB02_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelBB02_0056F034"
TRAVELBB02_CALLS = ("W05_RE_TravelBB02_ChickenDiedScene.Start()",)
TRAVELBB02_MEMBERS = {"fragment_stage_0100_item_00", "fragment_stage_1000_item_00"}

TRAVELBB03_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelBB03_0056F033"
TRAVELBB03_CALLS = (
    "W05_RE_TravelBB03_ChasingSquirrel.Start()",
    "W05_RE_TravelBB03_SquirrelDiedScene.Start()",
    "W05_RE_TravelBB03_WhoGetsMeatScene.Start()",
)
TRAVELBB03_MEMBERS = {
    "fragment_stage_0010_item_00", "fragment_stage_0100_item_00", "fragment_stage_0110_item_00",
    "fragment_stage_0210_item_00", "fragment_stage_1000_item_00",
}

TRAVELSM01_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelSM01_0059EA8F"
TRAVELSM02_SCRIPT = "Fragments:Quests:QF_W05_RE_TravelSM02_0059EA8E"

# Non-defect rows, adjudication-approved: intentional/cut scaffolding, no patch.
NON_DEFECT_SCRIPTS = (
    "Fragments:Quests:QF_W05_RE_CampTemplate_00568688",
    "Fragments:Quests:QF_W05_RE_JP05_Camp_CryptidS_00569CC5",
    "Fragments:Quests:QF_W05_RE_JP06_Camp_Rambling_00569CC6",
    "Fragments:Quests:QF_W05_RE_SceneJN01_Debug_005A506B",
    "Fragments:Quests:QF_W05_RE_SceneTemplate_00568689",
)

ALL_CALL_CASES: dict[str, tuple[str, ...]] = {
    **CAMP_B1_CASES,
    CAMP_JP34_SCRIPT: CAMP_JP34_CALLS,
    **CAMP_B2B3_CASES,
    **MINING_CASES,
    OBJECT_JP01_SCRIPT: OBJECT_JP01_CALLS,
    OBJECTAF01_SCRIPT: OBJECTAF01_CALLS,
    OBJECTBB01_SCRIPT: OBJECTBB01_CALLS,
    OBJECTBB02_SCRIPT: OBJECTBB02_CALLS,
    SCENE_JP04_A_SCRIPT: SCENE_JP04_A_CALLS,
    SCENE_JP04_B_SCRIPT: SCENE_JP04_B_CALLS,
    **{s: () for s in TRAVELERSJM_SCRIPTS},
    SCENEAF04_SCRIPT: (),
    SCENEZW01_SCRIPT: (),
    SCENEZW02_SCRIPT: SCENEZW02_CALLS,
    TRAVELAF01_SCRIPT: (),
    TRAVELAF02_SCRIPT: TRAVELAF02_CALLS,
    TRAVELBB01_SCRIPT: TRAVELBB01_CALLS,
    TRAVELBB02_SCRIPT: TRAVELBB02_CALLS,
    TRAVELBB03_SCRIPT: TRAVELBB03_CALLS,
    TRAVELSM01_SCRIPT: (),
    TRAVELSM02_SCRIPT: (),
}

ALL_MEMBER_CASES: dict[str, set[str]] = {
    **{s: set(CAMP_B1_MEMBERS) for s in CAMP_B1_CASES},
    CAMP_JP34_SCRIPT: set(CAMP_JP34_MEMBERS),
    **CAMP_B2B3_MEMBERS,
    **{s: set(MINING_MEMBERS) for s in MINING_CASES},
    OBJECT_JP01_SCRIPT: set(OBJECT_JP01_MEMBERS),
    OBJECTAF01_SCRIPT: set(OBJECTAF01_MEMBERS),
    OBJECTBB01_SCRIPT: set(OBJECTBB01_MEMBERS),
    OBJECTBB02_SCRIPT: set(OBJECTBB02_MEMBERS),
    SCENE_JP04_A_SCRIPT: set(SCENE_JP04_A_MEMBERS),
    SCENE_JP04_B_SCRIPT: set(SCENE_JP04_B_MEMBERS),
    **{s: set(EMPTY_TERMINAL_ONLY_MEMBERS) for s in TRAVELERSJM_SCRIPTS},
    SCENEAF04_SCRIPT: set(EMPTY_TERMINAL_ONLY_MEMBERS),
    SCENEZW01_SCRIPT: set(SCENEZW01_MEMBERS),
    SCENEZW02_SCRIPT: set(SCENEZW02_MEMBERS),
    TRAVELAF01_SCRIPT: set(EMPTY_TERMINAL_ONLY_MEMBERS),
    TRAVELAF02_SCRIPT: set(TRAVELAF02_MEMBERS),
    TRAVELBB01_SCRIPT: set(TRAVELBB01_MEMBERS),
    TRAVELBB02_SCRIPT: set(TRAVELBB02_MEMBERS),
    TRAVELBB03_SCRIPT: set(TRAVELBB03_MEMBERS),
    TRAVELSM01_SCRIPT: set(EMPTY_TERMINAL_ONLY_MEMBERS),
    TRAVELSM02_SCRIPT: set(EMPTY_TERMINAL_ONLY_MEMBERS),
}

assert set(ALL_CALL_CASES) == set(ALL_MEMBER_CASES)


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_calls"), ALL_CALL_CASES.items())
def test_qsf_b_patch_restores_confirmed_behavior(
    script_name: str, expected_calls: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert _member_names(patch) == ALL_MEMBER_CASES[script_name]
    for call in expected_calls:
        assert call in patch


@pytest.mark.parametrize(
    "script_name",
    [s for s, calls in ALL_CALL_CASES.items() if any(
        ".Enable()" in c or ".Disable()" in c or ".Start()" in c or ".Cast(" in c
        or ".RemoveItem(" in c or ".RemoveKeyword(" in c or ".DamageValue(" in c
        for c in calls
    )],
)
def test_qsf_b_actions_are_none_guarded(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    action_lines = [
        line for line in patch.splitlines()
        if any(
            token in line
            for token in (
                ".Enable()", ".Disable()", ".Start()", ".Cast(", ".RemoveItem(",
                ".RemoveKeyword(", ".DamageValue(",
            )
        )
        and "If " not in line
    ]
    assert action_lines
    guard_count = patch.count("!= None")
    assert guard_count >= 1


def test_qsf_b_campaf02_kill_patient_assignment_is_reachable():
    # Adjudicated: bound Health actorvalue property is the evidence a
    # DamageValue/GetValue pair (not Actor.Kill()) was the intended call --
    # verify the local Actor assignment precedes its own guard and use.
    patch = _script_patch_source("Fragments:Quests:QF_W05_RE_CampAF02_0056385E")
    assert patch is not None
    assign_index = patch.index("Actor patientActor = Alias_Patient02.GetActorReference()")
    guard_index = patch.index("If patientActor != None && Health != None")
    damage_index = patch.index("patientActor.DamageValue(Health, patientActor.GetValue(Health))")
    assert assign_index < guard_index < damage_index


def test_qsf_b_objectaf01_casts_from_bound_ref_not_self():
    # Adjudicated correction: Spell.Cast(ObjectReference, ObjectReference) cannot
    # take Self (the Quest) as akSource -- verify the corrected reference-typed
    # local is assigned, guarded, and used as both cast arguments.
    patch = _script_patch_source("Fragments:Quests:QF_W05_RE_ObjectAF01_0056A1D1")
    assert patch is not None
    assert "Self as" not in patch
    assign_index = patch.index("ObjectReference protRef = Alias_DisProtectron.GetReference()")
    cast_index = patch.index("W05_RE_ObjectAF01_Protectron_Destruct.Cast(protRef, protRef)")
    assert assign_index < cast_index
    keyword_assign_index = patch.rindex("ObjectReference protRef = Alias_DisProtectron.GetReference()")
    keyword_index = patch.index("protRef.RemoveKeyword(W05_RE_ObjectAF01_BrokenProtectronKeyword)")
    assert keyword_assign_index < keyword_index


@pytest.mark.parametrize(
    "script_name",
    [OBJECT_JP01_SCRIPT, SCENE_JP04_A_SCRIPT, SCENE_JP04_B_SCRIPT],
)
def test_qsf_b_hood_removal_refs_assigned_before_use(script_name: str):
    # Object_JP01 stage 40 / Scene_JP04_A stage 20 / Scene_JP04_B stage 13 all
    # reuse the same "remove HeadSacRef from the bound actor" idiom -- verify
    # each local ObjectReference is assigned and guarded before RemoveItem.
    patch = _script_patch_source(script_name)
    assert patch is not None
    for line in patch.splitlines():
        if ".RemoveItem(HeadSacRef, 1, true)" in line:
            local_name = line.strip().split(".RemoveItem", 1)[0]
            assign_needle = f"ObjectReference {local_name} ="
            assert assign_needle in patch
            assert patch.index(assign_needle) < patch.index(line.strip())


@pytest.mark.parametrize("script_name", ALL_CALL_CASES)
def test_qsf_b_patch_merge_native_compiles_for_fo4(script_name: str):
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


@pytest.mark.parametrize("script_name", NON_DEFECT_SCRIPTS)
def test_qsf_b_non_defect_rows_have_no_patch_file(script_name: str):
    assert _script_patch_source(script_name) is None


def test_qsf_b_patch_count_matches_approved_contract():
    assert len(CAMP_B1_CASES) == 9
    assert len(CAMP_B2B3_CASES) == 3
    assert len(MINING_CASES) == 4
    assert len(TRAVELERSJM_SCRIPTS) == 4
    assert len(ALL_CALL_CASES) == 37
    assert len(NON_DEFECT_SCRIPTS) == 5
    assert len(ALL_CALL_CASES) + len(NON_DEFECT_SCRIPTS) == 42
