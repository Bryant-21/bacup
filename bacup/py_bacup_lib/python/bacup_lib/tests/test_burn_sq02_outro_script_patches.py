from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


P1_SCRIPT = "Fragments:Quests:QF_BURN_SQ02_Outro_00815C9A"
P2_SCRIPT = "Fragments:Quests:QF_BURN_SQ02_OutroP2_00838DB1"

P1_STAGE_MEMBERS = tuple(
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (
        10,
        20,
        30,
        40,
        50,
        100,
        110,
        150,
        175,
        200,
        300,
        400,
        405,
        500,
        600,
        680,
        700,
        800,
        810,
        815,
        850,
        851,
        900,
        1000,
        1050,
        1100,
        1110,
        1120,
        1130,
        1200,
        1300,
        1303,
        1305,
        1310,
        1320,
        1350,
        1351,
        1400,
        1500,
        1505,
        1507,
        1510,
        1520,
        1525,
        1530,
        1600,
        1700,
        1800,
        1810,
        1815,
        1820,
        1823,
        1826,
        1829,
        1830,
        1833,
        1835,
        1850,
        1890,
        1895,
        1900,
        1910,
        1920,
        1950,
        1951,
        2000,
        2010,
        9000,
        9999,
    )
)
P2_STAGE_MEMBERS = tuple(
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (
        5,
        50,
        100,
        200,
        215,
        225,
        250,
        300,
        350,
        360,
        370,
        400,
        500,
        505,
        506,
        507,
        510,
        525,
        550,
        560,
        600,
        9000,
        9999,
    )
)
P1_FLAGLESS_CHECKPOINT_MEMBERS = tuple(
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (
        10,
        20,
        30,
        40,
        50,
        110,
        405,
        815,
        1050,
        1110,
        1120,
        1130,
        1505,
        1507,
        1520,
        1525,
        1530,
        1810,
    )
)

REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"


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


def _patch_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _skeleton(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return source_path.read_text(encoding="utf-8")


def _merged_source(script_name: str) -> str:
    return _merge_script_method_patches(
        _skeleton(script_name), _patch_source(script_name)
    )


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


@pytest.mark.parametrize(
    ("script_name", "stage_members", "property_count"),
    (
        (P1_SCRIPT, P1_STAGE_MEMBERS, 45),
        (P2_SCRIPT, P2_STAGE_MEMBERS, 34),
    ),
)
def test_outro_patches_supply_each_locked_stage_member_once_and_merge_idempotently(
    script_name: str, stage_members: tuple[str, ...], property_count: int
):
    patch = _patch_source(script_name)
    patch_members = _member_names(patch)
    patch_stage_members = [
        name for name in patch_members if name.startswith("fragment_stage_")
    ]
    merged = _merged_source(script_name)

    assert len(P1_STAGE_MEMBERS) == 69
    assert len(P2_STAGE_MEMBERS) == 23
    assert set(patch_stage_members) == set(stage_members)
    assert len(patch_stage_members) == len(stage_members)
    assert all(patch_stage_members.count(name) == 1 for name in stage_members)
    assert _skeleton(script_name).lower().count(" property ") == property_count
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in patch.splitlines())
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert _merge_script_method_patches(merged, patch) == merged


def test_insurrection_reconstructs_objective_inventory_and_actor_progression():
    patch = _patch_source(P1_SCRIPT)

    patrol = _member_body(patch, "fragment_stage_0600_item_00")
    supplies = _member_body(patch, "fragment_stage_0700_item_00")
    owen = _member_body(patch, "fragment_stage_1100_item_00")
    loan_fight = _member_body(patch, "fragment_stage_1820_item_00")
    key = _member_body(patch, "fragment_stage_1830_item_00")

    assert "SetObjectiveDisplayed(60)" in patrol
    assert "EnableAlias(Enemy_PatrolCaptain)" in patrol
    assert "AddItemOnce(supplyContainer, Item_SuppliesBackpack)" in supplies
    assert "AddItemOnce(Corpse_Owen.GetReference(), Item_Ring)" in owen
    assert loan_fight.count("MakeAliasHostile(") == 4
    assert "AddItemOnce(Container_LoanShark.GetReference(), Item_LoanKey)" in key


def test_insurrection_flagless_checkpoint_fragments_are_explicit_no_ops():
    patch = _patch_source(P1_SCRIPT)

    assert len(P1_FLAGLESS_CHECKPOINT_MEMBERS) == 18
    for member_name in P1_FLAGLESS_CHECKPOINT_MEMBERS:
        body = _member_body(patch, member_name).splitlines()
        assert body[1:-1] == ["\tReturn"]


def test_insurrection_preserves_recruit_and_loan_shark_choice_state():
    patch = _patch_source(P1_SCRIPT)

    assert "SetPlayerValue(AV_LeonardChoice, 1.0)" in _member_body(
        patch, "fragment_stage_1310_item_00"
    )
    assert "SetPlayerValue(AV_LeonardChoice, 2.0)" in _member_body(
        patch, "fragment_stage_1320_item_00"
    )
    assert "SetPlayerValue(AV_MagpieChoice, 1.0)" in _member_body(
        patch, "fragment_stage_1910_item_00"
    )
    assert "SetPlayerValue(AV_MagpieChoice, 2.0)" in _member_body(
        patch, "fragment_stage_1920_item_00"
    )
    assert "SetPlayerValue(AV_LoanShark, 1.0)" in _member_body(
        patch, "fragment_stage_1890_item_00"
    )
    assert "SetPlayerValue(AV_LoanShark, 2.0)" in _member_body(
        patch, "fragment_stage_1895_item_00"
    )
    assert "SetPlayerValue(AV_LoanShark, 3.0)" in _member_body(
        patch, "fragment_stage_1900_item_00"
    )


def test_insurrection_handoff_uses_story_manager_and_leaves_rewards_to_b21():
    patch = _patch_source(P1_SCRIPT)
    completion = _member_body(patch, "fragment_stage_9000_item_00")

    assert completion.index("CompleteAllObjectives()") < completion.index(
        "SendStoryEventAndWait"
    ) < completion.index("AdvanceIfPending(9999)")
    assert "BURN_SQ02_OutroP2_QuestStartKeyword.SendStoryEventAndWait" in completion
    assert "BURN_SQ02_OutroP2.Start(" not in patch
    assert "RewardCaps" not in patch
    assert "RewardItems" not in patch


def test_rust_settles_runs_all_five_local_waves_then_bypasses_only_server_handler():
    patch = _patch_source(P2_SCRIPT)

    assert patch.count(
        "(Self as Quest) as DefaultQuestEncounterWaveScript"
    ) == 2
    assert "Self as DefaultQuestEncounterWaveScript" not in patch
    assert "StartArenaWave(0)" in _member_body(
        patch, "fragment_stage_0225_item_00"
    )
    for stage, wave_index in ((250, 1), (300, 2), (350, 3), (360, 4)):
        assert f"StartArenaWave({wave_index})" in _member_body(
            patch, f"fragment_stage_{stage:04d}_item_00"
        )
    assert "AdvanceIfPending(400)" in _member_body(
        patch, "fragment_stage_0370_item_00"
    )
    assert "AdvanceIfPending(500)" in _member_body(
        patch, "fragment_stage_0400_item_00"
    )
    assert "DefaultKillObjective" not in patch


def test_rust_settles_preserves_both_eugene_outcomes_and_converges():
    patch = _patch_source(P2_SCRIPT)
    rust_king_kill = _member_body(patch, "fragment_stage_0505_item_00")
    player_choice = _member_body(patch, "fragment_stage_0506_item_00")
    player_kill = _member_body(patch, "fragment_stage_0507_item_00")

    assert "eugene.Kill(rustKing)" in rust_king_kill
    assert "AdvanceIfPending(510)" in rust_king_kill
    assert "SetObjectiveDisplayed(60)" in player_choice
    assert "MakeEugeneHostile()" in player_choice
    assert "SetObjectiveCompleted(60)" in player_kill
    assert "SetObjectiveDisplayed(70)" in player_kill
    assert "AdvanceIfPending(510)" in player_kill


def test_rust_settles_knockout_teleport_and_shutdown_are_ordered_and_reward_free():
    patch = _patch_source(P2_SCRIPT)
    knockout = _member_body(patch, "fragment_stage_0525_item_00")
    fade = _member_body(patch, "fragment_stage_0550_item_00")
    teleport = _member_body(patch, "fragment_stage_0560_item_00")
    cleanup = _member_body(patch, "fragment_stage_0600_item_00")
    completion = _member_body(patch, "fragment_stage_9000_item_00")

    assert knockout.index("knockoutFurniture.Activate(playerRef)") < knockout.index(
        "AdvanceIfPending(550)"
    )
    assert fade.index("FadeToBlackSpell.Cast(playerRef, playerRef)") < fade.index(
        "AdvanceIfPending(560)"
    )
    assert teleport.index("playerRef.MoveTo(teleportMarker)") < teleport.index(
        "AdvanceIfPending(600)"
    )
    assert cleanup.index("playerRef.RemoveItem(Key_Pipe") < cleanup.index(
        "AdvanceIfPending(9000)"
    )
    assert completion.index("CompleteAllObjectives()") < completion.index(
        "AdvanceIfPending(9999)"
    )
    assert "RewardCaps" not in patch
    assert "RewardItems" not in patch


@pytest.mark.parametrize("script_name", (P1_SCRIPT, P2_SCRIPT))
def test_outro_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("FO4 Papyrus compile dependencies unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
