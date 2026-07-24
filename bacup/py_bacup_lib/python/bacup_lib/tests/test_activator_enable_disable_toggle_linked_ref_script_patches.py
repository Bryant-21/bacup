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

# DotMatrixPrinterScript, POI287OnReadAddToMap, EnableDisableOnActivationScript,
# ACDuctEnterFXScript, and UnlockOnLoadScript are deterministic guarded one-shot
# handlers (no named states/timers) — a full compile of the merged patch is
# sufficient coverage for them; see repair-papyrus-stubs SKILL.md's
# dedicated-test-file criteria. MoMSecretDoorTriggerScript,
# UD004_NukashineAutoCloseDoorScript, and FSDoorScript are genuinely stateful
# (explicit timer IDs / reentry locks) and keep their detailed regression tests
# below.
PATCH_CASES = {
    "DotMatrixPrinterScript": {"onactivate"},
    "MoMSecretDoorTriggerScript": {"onactivate", "ontimer", "opensecretdoor"},
    "POI287OnReadAddToMap": {"onread"},
    "UD004_NukashineAutoCloseDoorScript": {"onopen", "ontimer"},
    "EnableDisableOnActivationScript": {"onactivate"},
    "ACDuctEnterFXScript": {"onactivate"},
    "UnlockOnLoadScript": {"onload"},
    "FSDoorScript": {"onopen", "ontimer"},
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


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_patch_supplies_expected_members(script_name: str, expected_members: set[str]):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected_members <= _member_names(patch)

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


def test_mom_secret_door_merged_source_has_single_handler_per_member():
    merged = _merged_source("MoMSecretDoorTriggerScript")

    assert merged.lower().count("event onactivate(") == 1
    assert merged.lower().count("event ontimer(") == 1
    assert merged.lower().count("function opensecretdoor(") == 1


def test_mom_secret_door_guards_before_opening():
    merged = _merged_source("MoMSecretDoorTriggerScript")

    lock_guard = merged.find("lock_SecretDoor")
    veil_guard = merged.find("WornHasKeyword(MoMVeilItemKeyword)")
    open_call = merged.find("linkedDoor.SetOpen(True)")

    assert lock_guard != -1
    assert veil_guard != -1
    assert open_call != -1
    assert lock_guard < veil_guard < open_call


def test_ud004_merged_source_has_single_handler_per_member():
    merged = _merged_source("UD004_NukashineAutoCloseDoorScript")

    assert merged.lower().count("event onopen(") == 1
    assert merged.lower().count("event ontimer(") == 1


def test_ud004_cancels_timer_before_restarting_it():
    merged = _merged_source("UD004_NukashineAutoCloseDoorScript")

    cancel_index = merged.find("CancelTimer(AutoCloseTimerID)")
    start_index = merged.find("StartTimer(delay, AutoCloseTimerID)")

    assert cancel_index != -1
    assert start_index != -1
    assert cancel_index < start_index


def test_ud004_rechecks_open_state_before_closing():
    merged = _merged_source("UD004_NukashineAutoCloseDoorScript")

    assert "GetOpenState()" in merged
    assert "SetOpen(False)" in merged


def test_fs_door_merged_source_has_single_handler_per_member():
    merged = _merged_source("FSDoorScript")

    assert merged.lower().count("event onopen(") == 1
    assert merged.lower().count("event ontimer(") == 1


def test_fs_door_uses_door_timer_property_with_nonzero_timer_id():
    merged = _merged_source("FSDoorScript")

    assert "StartTimer(DoorTimer, 1)" in merged
    assert "aiTimerID == 1" in merged
    assert "StartTimer(DoorTimer, 0)" not in merged


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_set_native_compiles_for_fo4(script_name: str):
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
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
