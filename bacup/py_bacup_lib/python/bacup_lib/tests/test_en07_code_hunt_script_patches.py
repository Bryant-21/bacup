from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCHED_SCRIPTS = (
    "EN07_CodeHuntQuestScript",
    "Fragments:Quests:QF_EN07_MQ_CodeHunt_002D0F6A",
    "Nuke_CodesScript",
    "Nuke_Codes_CodeHuntAliasScript",
    "Nuke_CodesOfficerScript",
    "Nuke_MasterScript",
    "Nuke_LaunchCardPatrolTerminalScript",
)


def _merged(script_name: str) -> str:
    skeleton = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_code_hunt_terminal_uses_story_manager_after_prefilling_aliases():
    merged = _merged("Nuke_LaunchCardPatrolTerminalScript")
    send_index = merged.index("EN07_CodeHuntQuestStartKeyword.SendStoryEventAndWait")
    assert merged.index("targetAlias.ForceRefTo(akTarget)") < send_index
    assert merged.index("targetLocationAlias.ForceLocationTo") < send_index
    assert "codeHuntQuest.Start()" not in merged
    assert "aiTargetType < 0 || aiTargetType > 1" in merged


def test_code_hunt_resets_completed_instance_before_second_story_event():
    merged = _merged("Nuke_LaunchCardPatrolTerminalScript")
    reset_index = merged.index("codeHuntQuest.Reset()")
    fill_index = merged.index("targetAlias.ForceRefTo(akTarget)")
    send_index = merged.index("EN07_CodeHuntQuestStartKeyword.SendStoryEventAndWait")
    assert reset_index < fill_index < send_index
    assert "codeHuntQuest.IsCompleted()" in merged[:fill_index]
    assert "|| codeHuntQuest.IsCompleted()" not in merged.split("codeHuntQuest.Reset()", 1)[0]


def test_code_hunt_acquisition_completes_main_prerequisites():
    merged = _merged("EN07_CodeHuntQuestScript")
    assert "player.GetItemCount(Nuke_LaunchCard) > 0" in merged
    assert "HasLocalCodePiece(player, iSiloGroupID)" in merged
    assert "introQuest.SetStage(40)" in merged
    assert "introQuest.SetStage(50)" in merged
    assert "SetStage(iSuccessStage)" in merged


def test_nuke_codes_initializes_local_officers_and_terminal_fallback():
    codes = _merged("Nuke_CodesScript")
    terminal = _merged("Nuke_LaunchCardPatrolTerminalScript")
    assert "officerAlias.RestoreOfficer()" in codes
    assert "nukeCodes.PrepareLocalCodeTarget" in terminal
    assert "CreateLocalTarget(akTerminalRef" in terminal
    assert "0x003E25D9" in terminal
    master = _merged("Nuke_MasterScript")
    assert "Nuke_CodesStartQuest.SendStoryEvent(None, player, player)" in master
    assert "nukeCodesQuest.Start()" not in master


def test_nuke_code_officer_registers_its_inventory_filter():
    officer = _merged("Nuke_CodesOfficerScript")
    assert "AddInventoryEventFilter(NukeCodePage)" in officer


def test_code_hunt_fragment_patch_covers_all_bound_stages():
    merged = _merged("Fragments:Quests:QF_EN07_MQ_CodeHunt_002D0F6A")
    for stage in (1, 2, 3, 10, 100, 200, 1000):
        assert f"Fragment_Stage_{stage:04d}_Item_00" in merged


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_code_hunt_patch_compiles(script_name: str, tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    for dependency in PATCHED_SCRIPTS:
        source_path = tmp_path / _script_relative_path(dependency, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(_merged(dependency), encoding="utf-8")

    source = _merged(script_name)
    result = compile_psc(
        source,
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
