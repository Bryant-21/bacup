from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
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
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def test_workshop_and_hunter_hunted_remain_unpatched_without_online_services():
    for script_name in (
        "GQ_WorkshopClearScript",
        "Fragments:Quests:QF_GQ_WorkshopClear_001789F9",
        "HunterHuntedQuestScript",
        "Fragments:Quests:QF_HunterHuntedQuest_003B25E3",
    ):
        assert _script_patch_source(script_name) is None


def test_flee_blast_patch_merges_local_countdown_once_and_measures_from_marker():
    patch = _script_patch_source("EN07_FleeBlastQuestScript")
    assert patch is not None
    assert "player.GetDistance(blastMarker)" not in patch
    assert "blastMarker.GetDistance(player)" in patch

    skeleton = """\
Scriptname EN07_FleeBlastQuestScript Extends DefaultQuestEMSInterfaceParentScript

referencealias Property NukeBlastMarker Auto mandatory
referencealias Property LaunchingPlayer Auto mandatory

Function DetonateLocalBlast()
EndFunction
"""
    merged = _merge_script_method_patches(skeleton, patch)
    members = _member_names(merged)

    for member in (
        "beginlocalblast",
        "handlestage",
        "beginlocalcountdown",
        "detonatelocalblast",
        "ontimer",
    ):
        assert members.count(member) == 1
    assert "Float playerDistance = blastMarker.GetDistance(player)" in merged


def test_flee_blast_patch_compiles_with_actual_generated_source():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    source_path = GENERATED_SOURCE_ROOT / "EN07_FleeBlastQuestScript.psc"
    assert source_path.is_file(), source_path
    patch = _script_patch_source("EN07_FleeBlastQuestScript")
    assert patch is not None
    merged = _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )

    result = compile_psc(
        merged,
        imports=[str(GENERATED_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="EN07_FleeBlastQuestScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
