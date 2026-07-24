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

# Section A -- Assault family. Every active row hands off to the native FO4 base
# REAssaultQuestScript sibling VMAD script (contracts/w3b-re-qsf-a.md Section A).
ASSAULT_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_AssaultAF01_0055DE89": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultBB02_0056F035": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW03_00569D82": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW04_0056A0C1": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW05_0056A0C0": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW06_0056A0BF": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW07_0056A0BE": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW08_0056A0BD": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW09_0056AC47": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW10_0056AC46": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW11_0056AC45": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW13_0056AC44": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW14_0056AC43": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW15_0056AC42": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
    "Fragments:Quests:QF_W05_RE_AssaultZW16_0056AC41": (
        "reAssault.InitAssault()", "reAssault.StartAssault()",
        "reAssault.CompleteAssault()", "reAssault.CleanupAssault()",
    ),
}

# Rows carrying a second stage-1000 log entry (Fragment_Stage_1000_Item_01), per the
# contract's Section A table -- everything except BB02/ZW13/ZW14/ZW16.
ASSAULT_TWO_TERMINAL_ITEMS = {
    "Fragments:Quests:QF_W05_RE_AssaultAF01_0055DE89",
    "Fragments:Quests:QF_W05_RE_AssaultZW03_00569D82",
    "Fragments:Quests:QF_W05_RE_AssaultZW04_0056A0C1",
    "Fragments:Quests:QF_W05_RE_AssaultZW05_0056A0C0",
    "Fragments:Quests:QF_W05_RE_AssaultZW06_0056A0BF",
    "Fragments:Quests:QF_W05_RE_AssaultZW07_0056A0BE",
    "Fragments:Quests:QF_W05_RE_AssaultZW08_0056A0BD",
    "Fragments:Quests:QF_W05_RE_AssaultZW09_0056AC47",
    "Fragments:Quests:QF_W05_RE_AssaultZW10_0056AC46",
    "Fragments:Quests:QF_W05_RE_AssaultZW11_0056AC45",
    "Fragments:Quests:QF_W05_RE_AssaultZW15_0056AC42",
}

ASSAULT_BASE_MEMBERS = {
    "fragment_stage_0020_item_00",
    "fragment_stage_0040_item_00",
    "fragment_stage_0050_item_00",
    "fragment_stage_1000_item_00",
}

# Section B -- Camp/Cryptid/Eavesdrop family (contracts/w3b-re-qsf-a.md Section B).
# B1: MobCamp/static tableau -- stage 10 does ClutterMarker toggle + scene-start
# (no separate trigger stage exists); stage 1000 is an evidenced-empty terminal.
B1_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_Camp_JP11_Campers_00586246": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP17_Bullies_0058624B": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP19_MobCamp__0058CEA2": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP20_MobCamp__0058CEA3": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP21_MobCamp__0058CEA6": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP22_MobCamp__0058CEA7": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP23_MobCamp__0058CEA8": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP24_DeadCult_0058CEA9": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP25_DeadSett_0058CEB0": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP26_DeadMuta_0058CEB5": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP27_MobCamp__0058CEB6": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP28_MobCamp__0058CEB8": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP29_MobCamp__0058CEBB": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP30_MobCamp__0058CEBC": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP31_MobCamp__0058CEBD": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
}
B1_MEMBERS = {"fragment_stage_0010_item_00", "fragment_stage_1000_item_00"}

# B2: Cryptid Stories -- stage 10 ClutterMarker toggle only; stage 20 scene-start;
# JP08 additionally carries the adjudicated "inferred under disclosure" stage 30.
B2_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_Camp_JP08_CryptidS_00571ACC": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP09_CryptidS_00571ACB": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP10_CryptidS_00571ACA": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()",
    ),
}
B2_MEMBERS = {"fragment_stage_0010_item_00", "fragment_stage_0020_item_00", "fragment_stage_1000_item_00"}
JP08_EXTRA_MEMBER = "fragment_stage_0030_item_00"

# B3: Eavesdrop scenes -- stage 10 ClutterMarker toggle only; a distinct trigger
# stage (11, or 15 for SuperMutantsAndFloater) does the scene-start.
B3_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:Quests:QF_W05_RE_Camp_JP11_RaiderCa_00586245": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP12_DateNigh_00586247": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP13_TwoScien_00586244": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP14_SoldierA_00586248": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP15_TwoSoldi_00586249": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "SceneRef.Start()",
    ),
    "Fragments:Quests:QF_W05_RE_Camp_JP16_SuperMut_0058624A": (
        "Alias_ClutterMarkerEnable.GetReference().Enable()",
        "Alias_ClutterMarkerDisable.GetReference().Disable()",
        "StartDialogue.Start()", "ChatScene.Start()",
    ),
}
# 4 of the 6 fire their scene-start at stage 11; SuperMutantsAndFloater fires at 15.
B3_TRIGGER_STAGE = {
    "Fragments:Quests:QF_W05_RE_Camp_JP11_RaiderCa_00586245": 11,
    "Fragments:Quests:QF_W05_RE_Camp_JP12_DateNigh_00586247": 11,
    "Fragments:Quests:QF_W05_RE_Camp_JP13_TwoScien_00586244": 11,
    "Fragments:Quests:QF_W05_RE_Camp_JP14_SoldierA_00586248": 11,
    "Fragments:Quests:QF_W05_RE_Camp_JP15_TwoSoldi_00586249": 11,
    "Fragments:Quests:QF_W05_RE_Camp_JP16_SuperMut_0058624A": 15,
}

ALL_PATCH_CASES: dict[str, tuple[str, ...]] = {
    **ASSAULT_PATCH_CASES,
    **B1_PATCH_CASES,
    **B2_PATCH_CASES,
    **B3_PATCH_CASES,
}


def _expected_members(script_name: str) -> set[str]:
    if script_name in ASSAULT_PATCH_CASES:
        members = set(ASSAULT_BASE_MEMBERS)
        if script_name in ASSAULT_TWO_TERMINAL_ITEMS:
            members.add("fragment_stage_1000_item_01")
        return members
    if script_name in B1_PATCH_CASES:
        return set(B1_MEMBERS)
    if script_name in B2_PATCH_CASES:
        members = set(B2_MEMBERS)
        if script_name == "Fragments:Quests:QF_W05_RE_Camp_JP08_CryptidS_00571ACC":
            members.add(JP08_EXTRA_MEMBER)
        return members
    if script_name in B3_PATCH_CASES:
        trigger_stage = B3_TRIGGER_STAGE[script_name]
        return {
            "fragment_stage_0010_item_00",
            f"fragment_stage_{trigger_stage:04d}_item_00",
            "fragment_stage_1000_item_00",
        }
    raise AssertionError(f"unhandled script_name: {script_name}")


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


@pytest.mark.parametrize(("script_name", "expected_calls"), ALL_PATCH_CASES.items())
def test_qsf_a_patch_restores_confirmed_behavior(
    script_name: str, expected_calls: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert _member_names(patch) == _expected_members(script_name)
    for call in expected_calls:
        assert call in patch


@pytest.mark.parametrize("script_name", ASSAULT_PATCH_CASES)
def test_qsf_a_assault_cast_is_none_guarded(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    action_lines = [
        line for line in patch.splitlines() if "reAssault." in line and "= Self as" not in line
    ]
    assert action_lines
    # Each function body declares + guards its own local. Stage 20 carries two
    # action lines (InitAssault then StartAssault) under a single guard per the
    # adjudicated amendment; every other guarded function is one action per guard.
    # Base functions: 20 (2 actions), 40 (1), 50 (1), 1000/Item_00 (1) = 4 guards,
    # 5 actions; rows with a second stage-1000 Item_01 add one more guard+action.
    expected_functions = 5 if script_name in ASSAULT_TWO_TERMINAL_ITEMS else 4
    guard_count = patch.count("reAssault != None")
    assert guard_count == expected_functions
    assert len(action_lines) == expected_functions + 1

    init_index = patch.index("reAssault.InitAssault()")
    start_index = patch.index("reAssault.StartAssault()")
    assert init_index < start_index


@pytest.mark.parametrize("script_name", list(B1_PATCH_CASES) + list(B2_PATCH_CASES) + list(B3_PATCH_CASES))
def test_qsf_a_camp_actions_are_none_guarded(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    action_lines = [
        line for line in patch.splitlines()
        if ".Enable()" in line or ".Disable()" in line or ".Start()" in line
    ]
    assert action_lines
    guard_count = patch.count("!= None")
    assert guard_count >= len(action_lines)


@pytest.mark.parametrize("script_name", ALL_PATCH_CASES)
def test_qsf_a_patch_merge_native_compiles_for_fo4(script_name: str):
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


def test_qsf_a_deleted_donotuse_row_has_no_patch_file():
    # QF_W05_RE_AssaultBB01_0056F036 -- eid DELETED_W05_RE_AssaultBB01_DONotUse,
    # contract disposition non-defect (intentional/cut content), not patched.
    assert _script_patch_source("Fragments:Quests:QF_W05_RE_AssaultBB01_0056F036") is None


def test_qsf_a_patch_count_matches_approved_contract():
    assert len(ASSAULT_PATCH_CASES) == 15
    assert len(B1_PATCH_CASES) == 15
    assert len(B2_PATCH_CASES) == 3
    assert len(B3_PATCH_CASES) == 6
    assert len(ALL_PATCH_CASES) == 39
