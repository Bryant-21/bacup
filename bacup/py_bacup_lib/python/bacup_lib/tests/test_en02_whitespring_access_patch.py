from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _fo76_to_fo4_script_type,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:Quests:QF_EN02_MQ_Us_000293A3"
OLD_PEX = (
    REPO_ROOT
    / "mods"
    / "SeventySixOld"
    / "data"
    / "Scripts"
    / _script_relative_path(SCRIPT_NAME, ".pex")
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
        OLD_PEX,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_en02_stage_400_grants_enclave_player_membership_idempotently():
    merged = _merged_source()

    assert "Extends Quest" in merged
    assert merged.count("Function Fragment_Stage_0400_Item_00()") == 1
    assert "EnclaveFaction != None" in merged
    assert "!playerRef.IsInFaction(EnclaveFaction)" in merged
    assert "playerRef.AddToFaction(EnclaveFaction)" in merged


def test_en02_stage_400_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
