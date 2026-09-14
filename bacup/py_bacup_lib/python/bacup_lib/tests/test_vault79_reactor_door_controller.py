from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Vault79ReactorDoorOpenScript"


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


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    merged = _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def test_reactor_door_patch_merges_one_activation_handler():
    merged = _merged_source()
    assert merged.lower().count("scriptname ") == 1
    assert merged.count("Event OnActivate(ObjectReference akActionRef)") == 1


def test_reactor_door_patch_runs_the_proven_release_graph_once():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    blocked = patch.index("If IsActivationBlocked()")
    lock = patch.index("BlockActivation(True, True)")
    banging = patch.index("sBangingOnDoor.Play(myDoorDummy)")
    ghoul = patch.index("sGhoulNoise.Play(myGhoulSoundMarker)")
    klaxon = patch.index("myKlaxonDummy.Activate(Self)")
    sound_chain = patch.index("myKlaxonDummy.GetLinkedRefChain(LinkCustom01)")
    sound_enable = patch.index("klaxonSounds[soundIndex].EnableNoWait()")
    door = patch.index("myDoorDummy.Activate(Self)")
    horde = patch.index("myActorEnableMarker.Enable()")
    boss_choice = patch.index("playerRef.GetValue(myActorValue) < 1.0")
    legendary = patch.index("myBossEnableLegendaryMarker.Enable()", boss_choice)
    ordinary_branch = patch.index("\n\tElse", legendary)
    ordinary = patch.index("myBossEnableMarker.Enable()", ordinary_branch)

    assert (
        blocked
        < lock
        < banging
        < ghoul
        < klaxon
        < sound_chain
        < sound_enable
        < door
        < horde
        < boss_choice
        < legendary
        < ordinary_branch
        < ordinary
    )
    second_ghoul_guard = patch.index("If myGhoulSound2Marker != None")
    second_ghoul = patch.index("sGhoulNoise.Play(myGhoulSound2Marker)")
    assert second_ghoul_guard < second_ghoul
    assert "sGhoulNoise.Play(myGhoulSound2Marker)" in patch
    assert "myBossEnableLegendaryMarker.Enable()" in patch
    assert "myBossEnableMarker.Enable()" in patch
    assert "BlockActivation(False)" not in patch
    assert "SetStage(" not in patch
    assert ".Start()" not in patch
    assert "Utility.Wait(" not in patch


def test_reactor_door_merged_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
