from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _fo76_to_fo4_script_type,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)
FO76_CLIENT_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
CAMERA_REPAIRS = {
    "Storm_LightningShakeCameraScript": {"oneffectstart"},
    "LC101ControlTerminalScript": {"playdusteffects"},
    "WL038MotherlodeDrillScript": {"clientshakecameraandcontroller"},
    "Vault79GearDoorExplosivesScript": {"shake"},
    "Vault79MotherlodeVaultWallScrip": {
        "clientcamerashake1",
        "clientcamerashake2",
    },
    "Vault79MotherlodeTunnelScript": {"clientcamerashake"},
}
UNSUPPORTED_PLAYER_API_REPAIRS = {
    "SpecialLoadoutsMenuActivatorScript": {"clientdisplayspecialbuildsmenu"},
    "p76_DLC01:DLC01_AtomicShopAdvertisement": {"onactivate"},
    "CharGenPipBoyPickupScript": {"onload"},
    "CharGenPerkBoardActivatorScript": {"clientdisplayperksmenu"},
    "CampObjectScript": {"clientdisplaycompanionnamemenu"},
}
COMPILE_FAILURE_REPAIRS = (
    "ArcadeBottleBlasterTarget",
    "ArcadeNukaZapperRaceTarget",
    "ArcadeShootingGalleryTarget",
    "ArcadeWhackAMoleTarget",
    "DefaultEffectPlaySound",
    "DLC03HermitCrabSpawnScript",
    "EN07_ApplyVaporizeImod",
    "MTNS04_NightstalkerArriveEffectScript",
    "NukashineFX",
    "p76_DLC01:DLC01_ApplyBabylonVaporizeImod",
    "PlayerTutorialScript",
    "PotionExpireScript",
    "SFS08_Heart_StranglerHeartScript",
    "V94_1_StranglerHeartScript",
    "VaultHandScannerScript",
    "VaultIDCardReaderScript",
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
                candidates.append(Path(line.split("=", 1)[1].strip().strip('"')))
                break
    for root in candidates:
        source_root = root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _merged_source(script_name: str) -> str:
    source_path = GENERATED_SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    source = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(source, patch)


@pytest.mark.parametrize(("script_name", "members"), CAMERA_REPAIRS.items())
def test_camera_repairs_replace_fo76_player_api_and_compile(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    actual = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind in {"function", "event"}
    }
    assert actual == members
    assert "player.Shake" not in patch
    assert "Game.ShakeCamera(" in patch
    assert "Game.ShakeController(" in patch

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(GENERATED_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_msilo_storage_uses_the_personal_quest_bound_actor_value():
    patch = _script_patch_source("MSiloQuestScript_Storage")
    assert patch is not None
    assert patch.count(
        "SetValue(personal.MSilo_Storage_SuccessfulBootValue, 1.0)"
    ) == 2
    assert "SetValue(MSilo_Storage_SuccessfulBootValue" not in patch


@pytest.mark.parametrize(
    ("script_name", "replaced_members"), UNSUPPORTED_PLAYER_API_REPAIRS.items()
)
def test_unavailable_fo76_player_api_is_removed_without_replacing_base_scripts(
    script_name: str, replaced_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind in {"function", "event"}
    }
    assert replaced_members <= members
    merged = _merged_source(script_name)
    assert "player.Show" not in merged
    assert "player.IsNewCharacter" not in merged

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        merged,
        imports=[str(GENERATED_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", COMPILE_FAILURE_REPAIRS)
def test_log_reported_missing_store_script_now_builds_for_fo4(script_name: str):
    source_pex = FO76_CLIENT_PEX_ROOT / _script_relative_path(script_name, ".pex")
    if not source_pex.is_file():
        pytest.skip(f"FO76 client PEX unavailable: {source_pex}")
    source = decompile_pex(
        source_pex,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    source = _augment_fo76_to_fo4_script_skeleton(script_name, source)
    patch = _script_patch_source(script_name)
    if patch is not None:
        source = _merge_script_method_patches(source, patch)

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        source,
        imports=[str(GENERATED_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
