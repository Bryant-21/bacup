from __future__ import annotations

import csv
import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "topicinfos"
)
LIVE_BINDING_LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "topicinfo-fragment-gap-ledger-2026-08-11.csv"
)

PATCH_MEMBERS = {
    "tif_v94_3_personal_003ee27b": (
        "fragment_end",
        "V94_3_Pump_RobotBetaHasRespawnedValue",
    ),
    "tif_v94_3_personal_003ee27e": (
        "fragment_end",
        "V94_3_Pump_RobotAlphaHasRespawnedValue",
    ),
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


@pytest.mark.parametrize("base_name,contract", PATCH_MEMBERS.items())
def test_topicinfo_deep_tranche4_augments_merges_and_compiles_full_source(
    base_name: str, contract: tuple[str, str]
):
    member_name, respawn_value = contract
    source_path = next(
        path
        for path in GENERATED_TOPICINFO_ROOT.glob("*.psc")
        if path.stem.casefold() == base_name.casefold()
    )
    generated_source = source_path.read_text(encoding="utf-8-sig")

    script_name = f"Fragments:TopicInfos:{base_name}"
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, generated_source)
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented
    assert (
        augmented.casefold().count(
            "actorvalue property v94_3_pump_robothasplayedspawnlinevalue auto mandatory"
        )
        == 1
    )
    assert respawn_value in augmented

    patch = _script_patch_source(script_name)
    assert patch is not None
    assert [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ] == [("function", member_name)]

    merged = _merge_script_method_patches(augmented, patch)
    assert merged.casefold().count(f"function {member_name}(") == 1

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_topicinfo_deep_tranche4_manifest_matches_live_vmad_ledger():
    with LIVE_BINDING_LEDGER.open(encoding="utf-8", newline="") as stream:
        tranche_rows = {
            row["script"]: row
            for row in csv.DictReader(stream)
            if row["disposition"] == "patched-deterministic-tranche4"
        }

    assert tranche_rows.keys() == PATCH_MEMBERS.keys()
    for script_name, (member_name, _respawn_value) in PATCH_MEMBERS.items():
        row = tranche_rows[script_name]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == member_name
        assert row["patch_members"].casefold() == member_name


@pytest.mark.parametrize("base_name,contract", PATCH_MEMBERS.items())
def test_topicinfo_deep_tranche4_sets_speaker_state_once_line_plays(
    base_name: str, contract: tuple[str, str]
):
    _member_name, respawn_value = contract
    patch = _script_patch_source(f"Fragments:TopicInfos:{base_name}")
    assert patch is not None
    assert "Actor robot = akSpeakerRef as Actor" in patch
    assert "robot.SetValue(V94_3_Pump_RobotHasPlayedSpawnLineValue, 1.0)" in patch
    assert f"robot.SetValue({respawn_value}, 1.0)" in patch
    assert "Game.GetPlayer()" not in patch


def test_topicinfo_deep_tranche4_patch_count():
    assert len(PATCH_MEMBERS) == 2
