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
GENERATED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CONTRACT = "contracts/w05-community-quest-fragment-closure.md"
DEFENSE_QF = "Fragments:Quests:QF_W05_Community_RaiderFishC_005A4F49"
NON_DEFECT_QFS = (
    "Fragments:Quests:QF_W05_Community_CottageBunk_00548805",
    "Fragments:Quests:QF_W05_Community_RaiderFishC_0055B16E",
    "Fragments:Quests:QF_W05_Community_RaiderFishC_01000879",
)


def _merged_defense_source() -> str:
    relative = Path(*DEFENSE_QF.split(":"))
    skeleton = (GENERATED_ROOT / relative).with_suffix(".psc").read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source(DEFENSE_QF)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_raider_fish_defense_starts_each_local_wave_and_resolves_the_objective():
    merged = _merged_defense_source()

    assert merged.count("StartLocalEncounterWave(") == 3
    for wave_index in range(3):
        assert f"StartLocalEncounterWave({wave_index})" in merged
    assert "pipeRef.SetDestroyed(False)" in merged
    assert "SetObjectiveDisplayed(100, True, True)" in merged
    assert "SetObjectiveCompleted(100, True)" in merged
    assert "SetObjectiveFailed(100, True)" in merged
    assert merged.count("Stop()") == 2


def test_raider_fish_defense_merged_fragment_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_defense_source(),
        imports=[str(base_source), str(GENERATED_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{DEFENSE_QF}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_community_quest_dispositions_match_the_live_carriers():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"].lower(): row for row in csv.DictReader(status_file)}

    defense = rows[DEFENSE_QF.lower()]
    assert defense["terminal_state"] == "patched"
    assert defense["evidence"] == CONTRACT

    for script_name in NON_DEFECT_QFS:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "non-defect"
        assert row["evidence"] == CONTRACT
        assert _script_patch_source(script_name) is None

    bed_and_breakfast = rows[
        "fragments:quests:qf_w05_community_bb_quest_00548a55"
    ]
    controller = rows["w05_community_bb_quest_script"]
    successor_contract = "contracts/w05-community-bed-and-breakfast.md"
    assert bed_and_breakfast["terminal_state"] == "patched"
    assert controller["terminal_state"] == "patched"
    assert bed_and_breakfast["evidence"] == successor_contract
    assert controller["evidence"] == successor_contract
