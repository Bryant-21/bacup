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

PATCH_CASES = {
    "W05_003P_ApplyPerkOnEnterRefScript": ("Game.GetPlayer()",),
    "W05_InstSwapEnableState": ("EvaluateSwapCriteria",),
}

EXPECTED_MEMBERS = {
    "W05_003P_ApplyPerkOnEnterRefScript": {"ontriggerenter"},
    "W05_InstSwapEnableState": {"evaluateswapcriteria", "oninit", "onload"},
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


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_calls"), PATCH_CASES.items())
def test_wastelanders_patch_restores_confirmed_behavior(
    script_name: str, expected_calls: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert _member_names(patch) == EXPECTED_MEMBERS[script_name]
    for call in expected_calls:
        assert call in patch


def test_inst_swap_enable_state_swap_quest_reachable_when_swap_av_unbound():
    """Revision 4: a sweep found SwapQuest genuinely populated (no SwapAV bound)
    on 32/152 live carriers, silently dropped by the pre-revision-4 body. The
    adjudicated spec requires SwapAV to take precedence (docstring: "ignored if
    a SwapAV is set") with SwapQuest reachable only via the ElseIf when SwapAV
    is unbound — so both branches must exist in that exact None-guard order."""
    patch = _script_patch_source("W05_InstSwapEnableState")
    assert patch is not None
    assert "EnableStates[i].SwapQuest.IsCompleted()" in patch
    swap_av_idx = patch.index("If EnableStates[i].SwapAV")
    swap_quest_idx = patch.index("ElseIf EnableStates[i].SwapQuest")
    is_completed_idx = patch.index("EnableStates[i].SwapQuest.IsCompleted()")
    assert swap_av_idx < swap_quest_idx < is_completed_idx


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_wastelanders_patch_merge_native_compiles_for_fo4(script_name: str):
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


def test_wastelanders_patch_count_matches_adjudicated_batch():
    assert len(PATCH_CASES) == 2
