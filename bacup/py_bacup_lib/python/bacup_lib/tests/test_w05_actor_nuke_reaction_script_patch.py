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
)
from creation_lib.pex import decompile_pex, parse_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "W05_ActorNukeReactionScript"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)
GENERATED_PEX = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / f"{SCRIPT_NAME}.pex"
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


def _merged_source() -> str:
    skeleton = decompile_pex(
        GENERATED_PEX,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    skeleton = _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, skeleton)
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_w05_nuke_reaction_patch_closes_local_stage_and_outfit_lifecycle():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    members = {
        name.lower()
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert {
        "oninit",
        "onload",
        "onunload",
        "registerforlocalnukeevents",
        "refreshlocalnukereaction",
        "getlocalnukeblastmarker",
        "handlelocalnukeincoming",
        "handlelocalnukeallclear",
        "ontimer",
    } <= members
    assert "Outfit Property BaseOutfit Auto" not in patch
    assert "Bool bReactingToNuke" not in patch
    assert "Bool bHazmatOutfitApplied" not in patch
    assert "Event Quest.OnStageSet" in patch
    assert 'RegisterForRemoteEvent(EN07_MQ_FleeBlast, "OnStageSet")' in patch
    assert "EN07_MQ_FleeBlast.GetAlias(0)" in patch
    assert "blastMarker.GetDistance(Self) > reactionRadius" in patch
    assert "Self == Game.GetPlayer()" in patch
    assert "GetLinkedRef(W05_NPCNukeFleeTargetKeyword)" in patch
    assert "SetValue(W05_NPCNukeFleeValue, 1.0)" in patch
    assert "SetValue(W05_NPCNukeFleeValue, 0.0)" in patch
    assert "NukeDialogueQuest.IsRunning()" in patch
    assert "NukeDialogueQuest.Start()" not in patch
    assert "SetOutfit(hazmatOutfit)" in patch
    assert "SetOutfit(BaseOutfit)" in patch
    assert "bHazmatOutfitApplied = True" in patch
    assert "If bHazmatOutfitApplied && BaseOutfit != None" in patch
    assert "EN07_MQ_FleeBlast.GetCurrentStageID() == 10" in patch


def test_w05_nuke_reaction_patch_merges_with_live_generated_skeleton():
    merged = _merged_source()

    assert merged.startswith(f"Scriptname {SCRIPT_NAME} Extends Actor")
    assert merged.count("Outfit Property BaseOutfit Auto") == 1
    assert merged.count("Bool bReactingToNuke") == 1
    assert merged.count("Bool bHazmatOutfitApplied") == 1
    assert "iIncomingNukeStage" not in merged
    assert "Event Quest.OnStageSet" in merged
    assert "Function HandleLocalNukeIncoming" in merged
    assert "Event OnTimer" in merged


def test_w05_nuke_reaction_patch_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(GENERATED_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None

    pex_path = tmp_path / f"{SCRIPT_NAME}.pex"
    pex_path.write_bytes(result.pex_bytes)
    pex = parse_pex(pex_path)
    variables = {str(variable.name).lower() for variable in pex.objects[0].variables}
    assert "breactingtonuke" in variables
    assert "bhazmatoutfitapplied" in variables
    functions = {
        str(function.name).lower()
        for state in pex.objects[0].states
        for function in state.functions
    }
    assert "::remote_quest_onstageset" in functions
