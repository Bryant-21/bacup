"""Ally radiant completion, its reward guard, and the ally outro start.

Converted ally radiants have an unbroken accept -> objective -> return chain and
no completion. These checks pin the stage-900 producer, the guard that keeps it
from completing a quest that pays nothing, the second-run properties that make a
repeatable radiant repeatable, and the ally outro start.

The native Papyrus compiler accepts unknown identifiers and member calls, so:

* stock ``PapyrusCompiler.exe``, which rejects both, is run too;
* symbol existence is checked through the Papyrus LSP ``ScriptDB`` with negative
  controls in both directions;
* the guard's one build-time premise, FO4's native ``QuestCompletionXP`` on all
  three generic radiants, is asserted against the live converted plugin.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.esp import Plugin
from creation_lib.papyrus_lsp import ScriptDB
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
TARGET_PLUGIN = REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wastelanders-radiant-completion-2026-09-09.md"
)

QUEST_SCRIPT = "COMP_RQ_Script"
ALLY_SCRIPT = "CompanionScript"
RETURN_SCRIPT = "COMP_PlayerReturnToQuestGiverInfo"
MERGE_SET = (QUEST_SCRIPT, ALLY_SCRIPT, RETURN_SCRIPT)

# The three generic radiants every proven title override resolves onto.
GENERIC_RADIANTS = {0x54F1A4: "COMP_RQ_Fetch", 0x56FB76: "COMP_RQ_Kill", 0x5727AD: "COMP_RQ_Rescue"}
COMPLETION_STAGE = 900
FAIL_STAGE = 800
COMPLETION_XP_GLOBAL = "5918EA"


def _fo4_root() -> Path | None:
    candidates: list[str] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(configured)
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(value)
                break
    for candidate in candidates:
        root = Path(candidate)
        if (root / "Data" / "Scripts" / "Source" / "Base").is_dir():
            return root
    return None


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    patch = _script_patch_source(script_name)
    assert patch is not None, script_name
    return _merge_script_method_patches(skeleton, patch)


def _write_merge_set(root: Path) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    for name in MERGE_SET:
        dest = root / _script_relative_path(name, ".psc")
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(_merged(name), encoding="utf-8")
    return root


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, header: str) -> str:
    assert header in source, header
    tail = source.split(header, 1)[1]
    return tail.split("EndFunction", 1)[0].split("EndEvent", 1)[0]


# --------------------------------------------------------------------------
# The producer and its guard
# --------------------------------------------------------------------------


def test_completion_producer_merges_once_and_is_idempotent() -> None:
    for name in (QUEST_SCRIPT, ALLY_SCRIPT):
        patch = _script_patch_source(name)
        assert patch is not None
        assert "scriptname " not in patch.lower()

        merged = _merged(name)
        assert merged.lower().count("scriptname ") == 1
        names = _member_names(merged)
        assert len(names) == len(set(names)), sorted(names)
        assert _merge_script_method_patches(merged, patch) == merged


def test_stage_300_is_the_only_producer_of_stage_900() -> None:
    merged = _merged(QUEST_SCRIPT)
    handler = _member_body(merged, "Event OnStageSet(Int auiStageID, Int auiItemID)")

    # The call sits in the return branch and nowhere else.
    return_branch = handler.split("ElseIf auiStageID == QuestStageReturnToQuestGiver", 1)[1]
    return_branch = return_branch.split("ElseIf", 1)[0]
    assert "TryCompleteRadiantQuest()" in return_branch
    assert handler.count("TryCompleteRadiantQuest()") == 1

    # No other route sets the completion stage.
    assert (
        merged.count("SetStage(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)") == 1
    )
    body = _member_body(merged, "Bool Function TryCompleteRadiantQuest()")
    assert "SetStage(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)" in body

    # The unused completion mode stays unwritten: no shipping row selects it and
    # its enum value is indistinguishable from the actor value's default.
    assert "COMP_QuestSetting" not in body
    assert "CompletesUponFirstObjectiveCompleted" not in merged


def test_completion_producer_guards_cover_the_gameplay_matrix() -> None:
    body = _member_body(_merged(QUEST_SCRIPT), "Bool Function TryCompleteRadiantQuest()")

    # running, work done, player returned
    assert "If !IsRunning()" in body
    assert "!IsStageDone(SecondObjective)" in body
    assert "!IsStageDone(QuestStageReturnToQuestGiver)" in body
    # exactly once: repeat activation, duplicate callback, reload
    assert "IsStageDone(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)" in body
    # cancellation and shutdown cannot be walked back into a completion
    assert "IsStageDone(QuestStageFailQuest)" in body
    assert "IsStageDone(QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV)" in body
    # the reward gate is evaluated before the transition, not after it
    guard_index = body.index("RadiantCompletionPaysOut()")
    assert guard_index < body.index("SetStage(QuestCompletionStage")

    # It is a stage producer and nothing else.
    for forbidden in ("AddItem", "RewardPlayerXP", "AddPerk", "Start()", "Stop()", "SetObjective"):
        assert forbidden not in body


def test_the_reward_guard_is_explicit_and_has_both_carriers() -> None:
    merged = _merged(QUEST_SCRIPT)
    gate = _member_body(merged, "Bool Function RadiantCompletionPaysOut()")
    assert "CompletionRewardScriptCoversStage(QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV)" in gate
    assert "NativeCompletionXPIsCarried()" in gate

    # Carrier 1 is a real runtime probe of the substitute reward script, so RW-01
    # attaching a stage-900 row is picked up with no further Papyrus change.
    carrier1 = _member_body(
        merged, "Bool Function CompletionRewardScriptCoversStage(Int auiStageID)"
    )
    assert "thisQuest as B21:QuestRewards" in carrier1
    assert "rewards.HasRewardForStage(auiStageID)" in carrier1

    # Carrier 2 is the one build-time premise, in one named place.
    carrier2 = _member_body(merged, "Bool Function NativeCompletionXPIsCarried()")
    assert carrier2.strip() == "Return True"


def test_completion_bookkeeping_qualifies_the_ally_actor_values() -> None:
    body = _member_body(
        _merged(QUEST_SCRIPT), "Function ClearRadiantQuestState(Bool completed)"
    )

    # RD-03: COMP_QuestCount is declared on CompanionScript, not here. The
    # unqualified read compiled and would not have resolved at runtime.
    assert "CompanionRef.GetValue(CompanionRef.COMP_QuestCount)" in body
    assert "CompanionRef.SetValue(CompanionRef.COMP_QuestCount, questCount)" in body
    assert "GetValue(COMP_QuestCount)" not in body.replace(
        "GetValue(CompanionRef.COMP_QuestCount)", ""
    )

    # Second run: the count advances and the cooldown is stamped only on success.
    assert "questCount += 1.0" in body
    assert "PlayerRef.SetValue(CompanionRef.PlayerQuestCountAV, questCount)" in body
    assert (
        "CompanionRef.SetValue(CompanionRef.COMP_DailyQuest_NextAllowedDay, "
        "Utility.GetCurrentGameTime() + coolDown)" in body
    )
    # A failed run must not pay the counter or the cooldown, and must not start
    # the ally outro.
    assert body.count("If completed") == 2
    assert "CompanionRef.TryStartOutroQuest()" in body


def test_second_run_is_reachable_through_the_delivered_selection() -> None:
    ally = _merged(ALLY_SCRIPT)
    selection = _member_body(ally, "Int Function FindRadiantQuestDataIndex(Float questCount)")

    # The next run is selected from the incremented player count and refused
    # until the cooldown the completed run stamped has elapsed.
    assert "questCount == currentDatum.QuestCount" in selection
    assert (
        "currentDatum.IgnoreCoolDown || GetValue(COMP_DailyQuest_NextAllowedDay) "
        "<= Utility.GetCurrentGameTime()" in selection
    )
    assert "FindRadiantQuestDataIndex(player.GetValue(PlayerQuestCountAV))" in ally

    # A stale pointer from the finished run cannot re-drive the finished quest.
    for consumer in ("Function SetRQAcceptanceStage()",):
        assert "_COMP_RQ_Ins.IsRunning()" in _member_body(ally, consumer)
    assert "!radiantQuest.IsRunning()" in _merged(RETURN_SCRIPT)


# --------------------------------------------------------------------------
# The ally outro start
# --------------------------------------------------------------------------


def test_outro_start_uses_the_story_event_route_and_cannot_double_start() -> None:
    body = _member_body(_merged(ALLY_SCRIPT), "Function TryStartOutroQuest()")

    assert "!OutroQuest || !OutroQuestStartKeyword || OutroQuestPlayerQuestCount < 0" in body
    assert "OutroQuest.IsRunning() || OutroQuest.IsStarting() || OutroQuest.IsCompleted()" in body
    assert (
        "player.GetValue(PlayerQuestCountAV) < (OutroQuestPlayerQuestCount as Float)" in body
    )
    assert (
        "OutroQuestStartKeyword.SendStoryEvent(GetCurrentLocation(), Self, None, 0, 0)" in body
    )
    # Both outro quests are event-scoped, so Quest.Start() is not a legal route.
    assert ".Start()" not in body


# --------------------------------------------------------------------------
# Symbol existence, independent of any compiler
# --------------------------------------------------------------------------


def test_completion_symbols_resolve_through_the_papyrus_lsp(tmp_path: Path) -> None:
    fo4_root = _fo4_root()
    source_dirs = [str(_write_merge_set(tmp_path / "merged")), str(SOURCE_ROOT)]
    if fo4_root is not None:
        source_dirs.append(str(fo4_root / "Data" / "Scripts" / "Source" / "Base"))

    db = ScriptDB(str(tmp_path / "lsp.db"), source_dirs=source_dirs)
    try:
        for member in (
            "TryCompleteRadiantQuest",
            "RadiantCompletionPaysOut",
            "CompletionRewardScriptCoversStage",
            "NativeCompletionXPIsCarried",
        ):
            assert db.has_function(QUEST_SCRIPT, member), member
            assert db.get_function_return_type(QUEST_SCRIPT, member) == "Bool"
        assert db.has_function(ALLY_SCRIPT, "TryStartOutroQuest")

        # The substitute reward script the guard probes really exists and really
        # exposes the member the guard calls.
        assert db.script_exists("B21:QuestRewards")
        assert db.has_function("B21:QuestRewards", "HasRewardForStage")

        for prop in (
            "QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV",
            "QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV",
            "QuestStageReturnToQuestGiver",
            "QuestStageFailQuest",
            "SecondObjective",
        ):
            assert db.has_property(QUEST_SCRIPT, prop), prop
        for prop in (
            "COMP_QuestCount",
            "OutroQuest",
            "OutroQuestStartKeyword",
            "OutroQuestPlayerQuestCount",
            "PlayerQuestCountAV",
        ):
            assert db.has_property(ALLY_SCRIPT, prop), prop

        # Negative controls in both directions: the unqualified read RD-03 fixed
        # really was unresolvable, and a made-up name really is rejected.
        assert not db.has_property(QUEST_SCRIPT, "COMP_QuestCount")
        assert not db.has_function(QUEST_SCRIPT, "TryCompleteRadiantQuestZ")
    finally:
        db.close()


# --------------------------------------------------------------------------
# Compilation, both compilers
# --------------------------------------------------------------------------


@pytest.mark.parametrize("script_name", MERGE_SET)
def test_merged_sources_compile_with_the_native_compiler(
    script_name: str, tmp_path: Path
) -> None:
    fo4_root = _fo4_root()
    if fo4_root is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    base = fo4_root / "Data" / "Scripts" / "Source" / "Base"
    merged_root = _write_merge_set(tmp_path / "merged")

    result = compile_psc(
        _merged(script_name),
        imports=[str(merged_root), str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    assert result.ok, "\n".join(str(item) for item in result.diagnostics)
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", MERGE_SET)
def test_merged_sources_compile_with_stock_papyruscompiler(
    script_name: str, tmp_path: Path
) -> None:
    """The native compiler accepts unknown identifiers and unknown member calls,
    so the stock compiler is the check with teeth."""
    fo4_root = _fo4_root()
    compiler = None if fo4_root is None else fo4_root / "Papyrus Compiler" / "PapyrusCompiler.exe"
    if compiler is None or not compiler.is_file():
        pytest.skip("stock PapyrusCompiler.exe unavailable")
    base = fo4_root / "Data" / "Scripts" / "Source" / "Base"

    user = _write_merge_set(tmp_path / "Source" / "User")
    out = tmp_path / "out"
    out.mkdir(parents=True, exist_ok=True)
    relative = _script_relative_path(script_name, ".psc")

    process = subprocess.run(
        [
            str(compiler),
            str(Path(relative).with_suffix("")),
            f"-import={user};{SOURCE_ROOT};{base}",
            f"-output={out}",
            f"-flags={base / 'Institute_Papyrus_Flags.flg'}",
        ],
        cwd=str(user),
        capture_output=True,
        text=True,
        check=False,
    )
    log = ((process.stdout or "") + (process.stderr or "")).strip()
    assert "Compilation succeeded" in log and ", 0 failed." in log, log


# --------------------------------------------------------------------------
# The build-time premise the guard rests on
# --------------------------------------------------------------------------


def test_all_three_generic_radiants_still_carry_native_completion_xp() -> None:
    """Carrier 2 of the reward guard, asserted against the live plugin.

    ``NativeCompletionXPIsCarried()`` returns True because Fallout 4 pays this
    field itself and the GMRW repair excludes it from B21:QuestRewards for that
    reason. If a future conversion drops it, this fails before anyone ships a
    radiant that completes for nothing.
    """
    if not TARGET_PLUGIN.is_file():
        pytest.skip(f"converted plugin unavailable: {TARGET_PLUGIN}")

    plugin = Plugin.load(TARGET_PLUGIN, game="fo4", lazy_index=True)
    try:
        for form_id, editor_id in GENERIC_RADIANTS.items():
            record = plugin.read_authoring_record(form_id)
            assert record is not None, editor_id
            assert record.get("eid") == editor_id

            xp = [f["QuestCompletionXP"] for f in record["fields"] if "QuestCompletionXP" in f]
            assert len(xp) == 1, editor_id
            assert xp[0]["reference"]["object_id"] == COMPLETION_XP_GLOBAL

            # And 900 really is the completing stage, 800 really is the failing
            # one, so the producer targets the stage the record flags.
            flags = _stage_flags(record)
            assert flags[COMPLETION_STAGE] == ["CompleteQuest"], editor_id
            assert flags[FAIL_STAGE] == ["FailQuest"], editor_id
            assert [
                stage for stage, value in flags.items() if "CompleteQuest" in value
            ] == [COMPLETION_STAGE], editor_id
    finally:
        plugin.close()


def _stage_flags(record: dict) -> dict[int, list[str]]:
    flags: dict[int, list[str]] = {}
    stage: int | None = None
    for field in record["fields"]:
        if "INDX" in field:
            stage = field["INDX"].get("StageIndex", 0)
        elif "StageFlags" in field and stage is not None:
            flags[stage] = field["StageFlags"]
    return flags


def test_contract_records_the_evidence_and_the_remaining_gaps() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for token in (
        "5918EA COMP_RQ_Reward_XP",
        "600.0",
        "01CC2A MQ102",
        "completion_xp_via_xnam",
        "71 of 71 rows, zero exceptions",
        "requested 569, found 569, missing 0",
        "573198 Astronaut_Outtro_Start",
        "5A061F Beckett_Outtro_Start",
    ):
        assert token in contract, token

    # The gaps stay visibly open rather than being absorbed into the result.
    for blocker in (
        "deliberately not authored",
        "has no writer",
        "still unpaid",
        "stays hollow",
        "zero callers",
        "zero live VMAD bindings",
    ):
        assert blocker in contract, blocker
