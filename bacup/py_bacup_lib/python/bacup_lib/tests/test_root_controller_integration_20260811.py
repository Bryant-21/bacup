from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
STATUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"


def test_random_stage_children_remain_declaration_only_nondefects():
    with STATUS.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"]: row for row in csv.DictReader(status_file)}

    for suffix in "ABC":
        script_name = f"DefaultSetRandomStages{suffix}"
        source = (SOURCE_ROOT / f"{script_name}.psc").read_text(encoding="utf-8")
        assert source.strip().endswith("Extends quests:_default:setrandomstages")
        assert "Function " not in source
        assert "Event " not in source
        assert _script_patch_source(script_name) is None
        assert rows[script_name]["terminal_state"] == "non-defect"
        assert rows[script_name]["evidence"] == "contracts/root-controller-integration.md"


def test_wayward_state_swap_single_player_substitute_compiles():
    script_name = "W05_WaywardStateSwapRefScript"
    skeleton = (SOURCE_ROOT / f"{script_name}.psc").read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None

    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.count("Function CheckPlayerWaywardValue(") == 1
    assert merged.count("Event OnCellLoad()") == 1
    assert "targetPlayer.GetValue(W05_MQ_004P_ChangeWaywardStates)" in merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_nuke_root_status_matches_the_bounded_local_chain():
    with STATUS.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"]: row for row in csv.DictReader(status_file)}

    for script_name in (
        "Nuke_CodesScript",
        "Nuke_Codes_CodeHuntAliasScript",
        "Nuke_CodesOfficerScript",
    ):
        assert rows[script_name]["terminal_state"] == "patched"
        assert rows[script_name]["evidence"] == "contracts/root-controller-integration.md"

    printers = rows["Nuke_CodesSolutionPrintersScript"]
    assert printers["terminal_state"] == "unsupported-online"
    assert printers["evidence"] == (
        "contracts/evidence-blocked-closure-2026-09-01.md"
    )
    assert "weekly cipher authority" in printers["notes"]


def test_every_root_status_row_has_a_terminal_disposition():
    with STATUS.open(encoding="utf-8", newline="") as status_file:
        rows = list(csv.DictReader(status_file))

    root_rows = [
        row
        for row in rows
        if not row["relative_path"].replace("\\", "/").lower().startswith(
            ("fragments/", "quests/")
        )
    ]
    assert root_rows
    assert all(row["terminal_state"] for row in root_rows)
