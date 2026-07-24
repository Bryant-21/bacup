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
DEPLOYED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

PATCH_CASES = {
    "Raids:RD01:Enc01:GuardianBotActorScript": {
        "pex": Path("Raids/RD01/Enc01/guardianbotactorscript.pex"),
        "members": {
            ("event", "ondeath"),
            ("event", "ontimer"),
        },
    },
}


def _fo4_base_source() -> Path | None:
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    candidates: list[Path] = []
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


def _merged_production_source(script_name: str) -> str:
    case = PATCH_CASES[script_name]
    pex_path = DEPLOYED_SCRIPT_ROOT / case["pex"]
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_guardian_bot_patch_is_a_method_fragment(script_name: str):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    actual_members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert PATCH_CASES[script_name]["members"] <= actual_members


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_guardian_bot_patch_merges_into_production_skeleton(script_name: str):
    merged = _merged_production_source(script_name)

    assert merged.lower().count("scriptname ") == 1
    for _kind, member_name in PATCH_CASES[script_name]["members"]:
        assert member_name in merged.lower()


def test_guardian_bot_patch_uses_explicit_nonzero_timer_id():
    patch = _script_patch_source("Raids:RD01:Enc01:GuardianBotActorScript")

    assert patch is not None
    assert "StartTimer(fDeathTimerDuration, 1)" in patch
    assert "aiTimerID == 1" in patch
    assert "DeathExplosion != None" in patch
    assert "PlaceAtMe(DeathExplosion)" in patch


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_guardian_bot_production_merge_native_compiles_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    source = _merged_production_source(script_name)
    result = compile_psc(
        source,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
