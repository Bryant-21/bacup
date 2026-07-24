from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_TERMINAL_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / "fragments" / "terminals"
)

PATCH_CASES = {
    "TERM_W05_MQ_004P_Crane_Regis_00424851": {1},
}


def _script_name(base_name: str) -> str:
    return f"Fragments:Terminals:{base_name}"


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


def _merged_production_source(base_name: str) -> str:
    pex_path = DEPLOYED_TERMINAL_ROOT / f"{base_name}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("base_name", "fragment_ids"), PATCH_CASES.items())
def test_crane_terminal_patch_supplies_every_vmad_fragment(
    base_name: str, fragment_ids: set[int]
):
    patch = _script_patch_source(_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    expected = {f"fragment_terminal_{fragment_id:02d}" for fragment_id in fragment_ids}
    assert expected <= members


def test_crane_terminal_registers_before_advancing_quest_and_disabling_security():
    patch = _script_patch_source(_script_name("TERM_W05_MQ_004P_Crane_Regis_00424851"))
    assert patch is not None

    set_value_index = patch.find(
        "SetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy, 1.0)"
    )
    set_stage_index = patch.find("W05_MQ_004P_Crane.SetCurrentStageID(820)")
    stop_bunker_index = patch.find("W05_MQ_004P_Crane_BunkerQuest.Stop()")
    add_topic_index = patch.find("W05_MQ_004P_Crane_PipBoyRegistered.Add()")

    assert set_value_index != -1
    assert set_stage_index != -1
    assert stop_bunker_index != -1
    assert add_topic_index != -1
    assert set_value_index < set_stage_index < stop_bunker_index < add_topic_index


def test_crane_terminal_guards_against_double_registration():
    patch = _script_patch_source(_script_name("TERM_W05_MQ_004P_Crane_Regis_00424851"))
    assert patch is not None

    guard_index = patch.find(
        "GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) != 0.0"
    )
    set_value_index = patch.find(
        "SetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy, 1.0)"
    )

    assert guard_index != -1
    assert set_value_index != -1
    assert guard_index < set_value_index


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_crane_terminal_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Terminals/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
