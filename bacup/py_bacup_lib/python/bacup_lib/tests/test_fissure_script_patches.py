from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
SOURCE_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAMES = ["SQ_FissureSpawnTriggerScript", "SQ_SmallFissureSpawner"]


def _fo4_base_source() -> Path | None:
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    candidates = [Path(configured)] if configured else []
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
    pex_path = DEPLOYED_SCRIPT_ROOT / f"{script_name}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    return _merge_script_method_patches(skeleton, patch)


def test_fissure_trigger_bypasses_missing_story_manager_with_verified_actor():
    patch = _script_patch_source("SQ_FissureSpawnTriggerScript")

    assert patch is not None
    assert 'Game.GetFormFromFile(0x000974BC, "SeventySix.esm")' in patch
    assert "spawnedScorchbeast.StartCombat(akPlayer, True)" in patch
    assert "SendStoryEvent" not in patch
    assert "cooldownSeconds = 300.0" in patch
    assert "cooldownSeconds = 30.0" in patch


def test_fissure_trigger_restores_trigger_and_cooldown_events():
    patch = _script_patch_source("SQ_FissureSpawnTriggerScript")

    assert patch is not None
    assert "Event OnTriggerEnter(ObjectReference akActionRef)" in patch
    assert "Event OnActivate(ObjectReference akActionRef)" in patch
    assert "State coolingdown" in patch
    assert "Event OnTimer(Int aiTimerID)" in patch
    assert 'GoToState("")' in patch


def test_small_fissure_quest_bridges_filled_aliases_to_trigger():
    patch = _script_patch_source("SQ_SmallFissureSpawner")

    assert patch is not None
    assert "Event OnQuestInit()" in patch
    assert "Trigger.GetReference()" in patch
    assert "PlayerAlias.GetActorReference()" in patch
    assert "triggerRef.Activate(playerRef)" in patch


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_fissure_patch_merges_into_deployed_skeleton(script_name: str):
    merged = _merged_source(script_name)

    assert merged.lower().count("scriptname ") == 1
    assert "Debug.Trace" not in merged


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_fissure_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_SCRIPT_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
