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
OLD_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySixOld" / "data" / "Scripts"
OLD_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySixOld" / "Scripts" / "Source" / "User"
)

CORE_PATCHES = {
    "EN07_NukeMasterScript": {
        "onquestinit",
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
        "onaliasinit",
        "onactivate",
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
    assert "SetValue(CodeEnteredIndexValue, 1.0)" in keypad_patch
    assert "BeginLocalLaunch(iSiloID, iLaunchID" in target_patch
    assert "ResolveLocalBlastTarget" in master_patch
    assert "0x003A8CCF" in master_patch
    assert "EN07_SiloResetCooldown.GetValue()" in master_patch


def _old_pex_path(script_name: str) -> Path:
    path = OLD_SCRIPT_ROOT / _script_relative_path(script_name, ".pex")
    assert path.is_file(), path
    return path


def _merged_source(script_name: str) -> str:
    skeleton = decompile_pex(
        _old_pex_path(script_name),
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
            imports=[str(tmp_path), str(OLD_SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}:\n{diagnostics}"
        assert result.pex_bytes is not None
