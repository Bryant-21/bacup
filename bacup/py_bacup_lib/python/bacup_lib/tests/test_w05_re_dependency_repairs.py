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
SCRIPT_NAME = "DefaultMoveAliasWithSpawnMap"


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
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_default_move_alias_patch_is_a_single_quest_init_member():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "Scriptname " not in patch
    members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert members == {"onquestinit"}


def test_default_move_alias_patch_preserves_evidenced_annulus_and_stage_order():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "Utility.RandomFloat(minDistance, maxDistance)" in patch
    assert "Math.Cos(spawnAngle) * spawnDistance" in patch
    assert "Math.Sin(spawnAngle) * spawnDistance" in patch
    assert "SetStage(StageToSetOnSpawn)" in patch
    assert patch.index("kActor.MoveTo(") < patch.index("SetStage(StageToSetOnSpawn)")


def test_default_move_alias_patch_guards_none_and_zero_distance():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "kCenter == None || kActor == None" in patch
    assert "If maxDistance == 0.0" in patch
    assert "kActor.MoveTo(kCenter, 0.0, 0.0, 0.0, False)" in patch
    assert "If StageToSetOnSpawn >= 0" in patch


def test_default_move_alias_patch_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{SCRIPT_NAME}:\n{diagnostics}"
    assert result.pex_bytes is not None
