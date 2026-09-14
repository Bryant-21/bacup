from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _fo76_to_fo4_script_type,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

CORE_PATCHES = {
    "EN07_NukeMasterScript": {
        "onquestinit",
        "preparelocalsiloentry",
        "handlelocallaunchcard",
        "handlelocalcodeaccepted",
        "resolvelocalblasttarget",
        "beginlocallaunch",
        "detonatelocalblastfallback",
        "completelocallaunch",
        "resetlocalsilo",
        "ontimer",
    },
    "EN07_TargetingComputerAliasScript": {
        "islocallaunchprepcomplete",
        "onaliasinit",
        "onactivate",
        "setlocalblasttarget",
    },
    "EN07_ExternalKeypadAliasScript": {
        "islocallaunchprepcomplete",
        "haslocalcodepiece",
        "haslocalsiloaccess",
        "onaliasinit",
        "onactivate",
    },
    "MSiloPersonalQuestScript": {
        "preparesiloaliases",
        "ensuresilostarted",
    },
    "MSiloStartupQuestScript": {
        "startsiloquests",
        "startpreparedsilo",
    },
    "EN07_AccessPanelAliasScript": {
        "haslocalsiloaccess",
        "onaliasinit",
        "onactivate",
    },
    "Nuke_CodesOfficerScript": {
        "onaliasinit",
        "ondeath",
        "onitemremoved",
        "ontimer",
        "restoreofficer",
    },
    "EN07_FleeSiloScript": {
        "beginlocallaunch",
        "handlestage",
        "ontimer",
        "finishlocallaunch",
        "resetlocalsilo",
    },
    "EN07_FleeBlastQuestScript": {
        "beginlocalblast",
        "handlestage",
        "beginlocalcountdown",
        "detonatelocalblast",
        "ontimer",
    },
    "EN07_NukeBlastMarkerRefScript": {"clientupdatemaphazards"},
    "EN07_LaunchCardReceptacleScript": {"resetlocalcard"},
}

FRAGMENT_PATCHES = {
    "Fragments:Quests:QF_EN07_MQ_FleeSilo_002D0F68": {
        10,
        20,
        30,
        35,
        40,
        45,
        200,
        300,
    },
    "Fragments:Quests:qf_en07_mq_fleeblast_002d0f69": {10, 100},
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


def _member_names(patch: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }


@pytest.mark.parametrize(("script_name", "expected"), CORE_PATCHES.items())
def test_en07_core_patch_supplies_local_launch_behavior(
    script_name: str, expected: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected <= _member_names(patch)


@pytest.mark.parametrize(("script_name", "stages"), FRAGMENT_PATCHES.items())
def test_en07_fragment_patch_supplies_every_vmad_stage(
    script_name: str, stages: set[int]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    expected = {f"fragment_stage_{stage:04d}_item_00" for stage in stages}
    assert expected <= _member_names(patch)


def test_en07_launch_chain_preserves_card_code_target_and_cooldown_gates():
    card_patch = _script_patch_source("EN07_LaunchCardReceptacleScript")
    keypad_patch = _script_patch_source("EN07_ExternalKeypadAliasScript")
    target_patch = _script_patch_source("EN07_TargetingComputerAliasScript")
    master_patch = _script_patch_source("EN07_NukeMasterScript")

    assert card_patch is not None
    assert keypad_patch is not None
    assert target_patch is not None
    assert master_patch is not None
    assert "HandleLocalLaunchCard(Self)" in card_patch
    assert "personalQuest.IsStageDone(530)" in card_patch
    assert "SetValue(CodeEnteredIndexValue, 1.0)" in keypad_patch
    assert "HasLocalCodePiece(player)" in keypad_patch
    assert "0x004DE233" in keypad_patch
    assert "0x004DE23B" in keypad_patch
    assert "If LinkedAccessPanel != None" in keypad_patch
    assert "BeginLocalLaunch(iSiloID, iLaunchID" in target_patch
    assert "EN07_FleeQuestStartKeyword.SendStoryEventAndWait" in target_patch
    assert "ResolveLocalBlastTarget" in master_patch
    assert "0x003A8CCF" in master_patch
    assert "launchData.KeypadActive.ForceRefTo(exteriorKeypad)" in master_patch
    assert "EN07_SiloResetCooldown.GetValue()" in master_patch


def test_en07_flee_silo_never_direct_starts_event_scoped_quest():
    flee_patch = _script_patch_source("EN07_FleeSiloScript")
    blast_patch = _script_patch_source("EN07_FleeBlastQuestScript")
    master_patch = _script_patch_source("EN07_NukeMasterScript")
    assert flee_patch is not None
    assert blast_patch is not None
    assert master_patch is not None
    assert "fleeQuest.Start()" not in flee_patch
    assert "fleeQuest.Start()" not in blast_patch
    send_index = master_patch.index("EN07_FleeBlastQuestStartKeyword.SendStoryEventAndWait")
    assert master_patch.index("blastAlias.ForceRefTo(blastMarker)") < send_index
    assert master_patch.index("triggerLocationAlias.ForceLocationTo") < send_index


def test_tales_targeting_override_has_no_unimplemented_fullscreen_map_bridge():
    targeting = (
        REPO_ROOT
        / "mods"
        / "B21_TalesFromAppalachia"
        / "Scripts"
        / "Source"
        / "User"
        / "EN07_TargetingComputerAliasScript.psc"
    ).read_text(encoding="utf-8")
    assert "B21_FullScreenMap" not in targeting
    assert "SpawnNukedVariant" not in targeting
    assert "EN07_FleeQuestStartKeyword.SendStoryEventAndWait" in targeting

    bridge = (
        REPO_ROOT
        / "mods"
        / "B21_TalesFromAppalachia"
        / "Scripts"
        / "Source"
        / "User"
        / "B21"
        / "B21_TFA_NukeQuestBridge.psc"
    ).read_text(encoding="utf-8")
    assert "NukeCodes.Start()" not in bridge
    assert "NukeCodesStartKeyword.SendStoryEvent" in bridge


@pytest.mark.parametrize(
    "relative_path",
    [
        Path("EN07_TargetingComputerAliasScript.psc"),
        Path("EN07_ExternalKeypadAliasScript.psc"),
        Path("B21") / "B21_TFA_NukeQuestBridge.psc",
    ],
)
def test_tales_nuke_overrides_compile_for_fo4(relative_path: Path, tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    tales_source = (
        REPO_ROOT
        / "mods"
        / "B21_TalesFromAppalachia"
        / "Scripts"
        / "Source"
        / "User"
    )
    for dependency in (
        "EN07_NukeMasterScript",
        "MSiloPersonalQuestScript",
        "MSiloStartupQuestScript",
    ):
        dependency_path = tmp_path / _script_relative_path(dependency, ".psc")
        dependency_path.parent.mkdir(parents=True, exist_ok=True)
        dependency_path.write_text(_merged_source(dependency), encoding="utf-8")
    source = (tales_source / relative_path).read_text(encoding="utf-8")
    result = compile_psc(
        source,
        imports=[
            str(tales_source),
            str(tmp_path),
            str(GENERATED_SOURCE_ROOT),
            str(base_source),
        ],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{relative_path}:\n{diagnostics}"


def test_en07_region_timers_do_not_share_mutable_silo_state():
    master = _script_patch_source("EN07_NukeMasterScript")
    assert master is not None
    assert "StartTimer(180.0, 7001 + aiSiloID)" in master
    assert "StartTimer(cooldownSeconds, 7011 + aiSiloID)" in master
    assert "DetonateLocalBlastFallback(aiTimerID - 7001)" in master
    assert "ResetLocalSilo(siloID, siloID + 3)" in master
    assert "iDebugNukeRegionIndex" not in master
    fallback_ids = {7001 + silo_id for silo_id in range(3)}
    cooldown_ids = {7011 + silo_id for silo_id in range(3)}
    assert fallback_ids.isdisjoint(cooldown_ids)


def _generated_pex_path(script_name: str) -> Path:
    path = GENERATED_SCRIPT_ROOT / _script_relative_path(script_name, ".pex")
    assert path.is_file(), path
    return path


def _merged_source(script_name: str) -> str:
    skeleton = decompile_pex(
        _generated_pex_path(script_name),
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_en07_patch_set_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    script_names = [*CORE_PATCHES, *FRAGMENT_PATCHES]
    merged_sources: dict[str, str] = {}
    for script_name in script_names:
        source = _merged_source(script_name)
        merged_sources[script_name] = source
        source_path = tmp_path / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(source, encoding="utf-8")

    for script_name, source in merged_sources.items():
        result = compile_psc(
            source,
            imports=[str(tmp_path), str(GENERATED_SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}:\n{diagnostics}"
        assert result.pex_bytes is not None
