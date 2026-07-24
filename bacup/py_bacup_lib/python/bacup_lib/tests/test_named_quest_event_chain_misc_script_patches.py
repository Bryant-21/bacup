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
    "E08A_BeehiveContainerScript": {"onactivate"},
    "E09B_Wheel_ColaRewardScript": {"oneffectstart"},
    "LC006_SecurityMarkerScript": {"onactivate"},
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


@pytest.mark.parametrize("script_name", sorted(PATCH_CASES))
def test_patch_supplies_expected_members(script_name: str):
    expected_members = PATCH_CASES[script_name]
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected_members <= _member_names(patch)

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


@pytest.mark.parametrize("script_name", sorted(PATCH_CASES))
def test_patch_merged_source_has_single_handler_per_member(script_name: str):
    merged = _merged_source(script_name)

    for member in PATCH_CASES[script_name]:
        assert merged.lower().count(f"event {member}(") == 1


def test_beehive_spawn_gated_before_placement():
    merged = _merged_source("E08A_BeehiveContainerScript")

    enabled_index = merged.find("enabled = True")
    guard_index = merged.find("MaxActorCount <= 0 || MaxSpawnCount <= 0")
    wait_index = merged.find("Utility.Wait(SpawnTime)")
    place_index = merged.find("PlaceActorAtMe(BeeSwarm)")

    assert -1 not in (enabled_index, guard_index, wait_index, place_index)
    assert enabled_index < guard_index < wait_index < place_index


def test_wheel_cola_picks_before_casting():
    merged = _merged_source("E09B_Wheel_ColaRewardScript")

    pick_index = merged.find("Utility.RandomInt(0, Effects.Length - 1)")
    cast_index = merged.find("chosen.Cast(akTarget, akTarget)")

    assert pick_index != -1
    assert cast_index != -1
    assert pick_index < cast_index


def test_security_marker_casts_quest_before_reading_doors():
    merged = _merged_source("LC006_SecurityMarkerScript")

    cast_index = merged.find("as LC006_PoseidonPlantQuestScript")
    doors_index = merged.find("questScript.SecurityDoors")
    setopen_index = merged.find("SetOpen(False)")

    assert -1 not in (cast_index, doors_index, setopen_index)
    assert cast_index < doors_index < setopen_index


def test_security_marker_clamps_each_endpoint_before_minmax():
    """Contract's specified order: clamp StartDoor/EndDoor independently into
    [0, doors.Length - 1] first, THEN take min/max of the clamped pair. Guards
    against regressing to the reviewed-out ordering (swap-then-partial-clamp),
    which silently closes zero doors instead of the boundary door when both
    endpoints are out of range on the same side."""
    merged = _merged_source("LC006_SecurityMarkerScript")

    clamp_start_index = merged.find("clampedStart")
    clamp_end_index = merged.find("clampedEnd")
    swap_index = merged.find("swapTemp")

    assert -1 not in (clamp_start_index, clamp_end_index, swap_index)
    assert clamp_start_index < swap_index
    assert clamp_end_index < swap_index


def _clamp_door_range(start_door: int, end_door: int, door_count: int) -> tuple[int, int]:
    """Python mirror of LC006_SecurityMarkerScript.psc's OnActivate clamp
    algorithm, kept in lockstep with the patch: clamp each endpoint
    independently into [0, door_count - 1], then take min/max."""

    def _clamp(value: int) -> int:
        if value < 0:
            return 0
        if value >= door_count:
            return door_count - 1
        return value

    low = _clamp(start_door)
    high = _clamp(end_door)
    if low > high:
        low, high = high, low
    return low, high


@pytest.mark.parametrize(
    ("start_door", "end_door", "door_count", "expected"),
    [
        # Reviewer repro: both endpoints beyond a 17-door array clamp to the
        # same boundary door instead of producing an empty range.
        (20, 25, 17, (16, 16)),
        # Mirrored failure mode: both endpoints below zero.
        (-5, -1, 17, (0, 0)),
        # Ordinary in-range case is unaffected by the reorder.
        (2, 5, 17, (2, 5)),
        # Reversed in-range endpoints still normalize via min/max.
        (5, 2, 17, (2, 5)),
    ],
)
def test_security_marker_door_range_clamp_edges(
    start_door: int, end_door: int, door_count: int, expected: tuple[int, int]
):
    assert _clamp_door_range(start_door, end_door, door_count) == expected


@pytest.mark.parametrize("script_name", sorted(PATCH_CASES))
def test_patch_set_compiles_for_fo4(script_name: str):
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
