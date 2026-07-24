from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SOURCE_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
SELF_DESTRUCT = "W05_RE_ObjectAF01_SelfDestruct_Script"
UNPATCHED_CLIENT_CONTROLLERS = {
    "W05_WaywardStateSwapRefScript": [
        "clientenablearray",
        "updateclientenablestate",
        "checkplayerwaywardvalue",
        "oncellload",
        "ontimer",
    ],
    "WL005_DeathBoxMachineScript": [
        "startdoublevision",
        "clientstopspikedropmachinesound",
        "clientplayspikedropmachinesound",
        "clientplayrockrumblesound",
        "clientplaylightswitchclicksound",
        "enddoublevision",
    ],
    "WL005_DeathTurretsMachineScript": [
        "clientplaycounddownbeepsound",
        "clientplaypoweroffsound",
        "clientplaypoweronsound",
    ],
}


def _member_names(source: str) -> list[str]:
    return [
        name.lower()
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _generated_source(script_name: str) -> str:
    path = GENERATED_ROOT / f"{script_name}.psc"
    assert path.is_file(), f"generated source unavailable: {path}"
    return path.read_text(encoding="utf-8")


def _source_pex(script_name: str) -> str:
    path = SOURCE_PEX_ROOT / f"{script_name}.pex"
    assert path.is_file(), f"FO76 source PEX unavailable: {path}"
    return decompile_pex(path, fo4_api_compat=True)


def _merged_self_destruct() -> str:
    patch = _script_patch_source(SELF_DESTRUCT)
    assert patch is not None
    return _merge_script_method_patches(_generated_source(SELF_DESTRUCT), patch)


def test_source_pex_confirms_the_three_unpatched_controllers_are_client_surfaces():
    for script_name, expected_members in UNPATCHED_CLIENT_CONTROLLERS.items():
        assert _script_patch_source(script_name) is None
        assert _member_names(_source_pex(script_name)) == expected_members
        assert _member_names(_generated_source(script_name)) == expected_members


def test_self_destruct_patch_restores_local_countdown_and_terminal_path():
    patch = _script_patch_source(SELF_DESTRUCT)
    assert patch is not None
    assert "Scriptname " not in patch

    merged = _merged_self_destruct()
    assert merged.count("Function ResetSelfDestruct(") == 1
    assert merged.count("Function StartSelfDestruct(") == 1
    assert merged.count("Function ExplodeSelfDestruct(") == 1
    assert merged.count("Event OnEffectFinish(") == 1
    assert merged.count("Event OnTimer(") == 1
    assert 'StartTimer(SelfDestructingTime, selfDestructTimerID)' in merged
    assert 'GoToState("selfdestructed")' in merged
    assert "selfRef.PlaceAtMe(SelfDestructExplosion)" in merged


@pytest.mark.parametrize("script_name", list(UNPATCHED_CLIENT_CONTROLLERS))
def test_unpatched_client_controllers_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _generated_source(script_name),
        imports=[str(GENERATED_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_self_destruct_patch_merges_and_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_self_destruct(),
        imports=[str(GENERATED_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SELF_DESTRUCT}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
