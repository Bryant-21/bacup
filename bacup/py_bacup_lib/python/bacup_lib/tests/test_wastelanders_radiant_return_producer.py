"""The stage-300 return producer for ally radiant quests.

`COMP_PlayerReturnToQuestGiverInfo` is bound on 122 live INFO records and is the
only producer of `COMP_RQ_Script.QuestStageReturnToQuestGiver`. These checks pin
the member and the guards that keep it from becoming an automatic stage advance.

The native Papyrus compiler accepts unknown unqualified identifiers and member
calls, so a passing `compile_psc` doesn't prove a symbol resolves; the Papyrus LSP
`ScriptDB` checks that separately.
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.papyrus_lsp import ScriptDB
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wastelanders-radiant-return-producer-2026-09-09.md"
)

RETURN_SCRIPT = "COMP_PlayerReturnToQuestGiverInfo"
DEPENDENCIES = ("CompanionScript", "COMP_RQ_Script")


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


def _merged(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None, script_name
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _body(source: str, header: str) -> str:
    assert header in source
    return source.split(header, 1)[1].split("EndFunction", 1)[0].split("EndEvent", 1)[0]


def test_return_producer_patch_merges_once_and_is_idempotent() -> None:
    patch = _script_patch_source(RETURN_SCRIPT)
    assert patch is not None
    assert "scriptname " not in patch.lower()
    assert _member_names(patch) == ["onend"]

    merged = _merged(RETURN_SCRIPT)
    assert merged.lower().count("scriptname ") == 1
    assert _member_names(merged).count("onend") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_return_producer_uses_the_delivered_speaker_resolution() -> None:
    merged = _merged(RETURN_SCRIPT)
    accept = _merged("COMP_PlayerAcceptsQuestInfoScript")

    # Return carriers are mostly ally responses; accept carriers are
    # PlayerAddress lines. Both must resolve the companion the same way.
    for shared in (
        "Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)",
        "If akSpeakerRef == Game.GetPlayer()",
        "(akSpeakerRef as Actor).GetDialogueTarget()",
        "companionActor = akSpeakerRef as Actor",
        "companionActor as CompanionScript",
    ):
        assert shared in merged
        assert shared in accept


def test_return_producer_sets_stage_300_once_and_only_stage_300() -> None:
    merged = _merged(RETURN_SCRIPT)
    body = _body(merged, "Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)")

    assert "companion.GetCurrentRadiantQuest()" in body
    assert "!radiantQuest || !radiantQuest.IsRunning()" in body
    # The objective stage must actually have happened before a "return" counts.
    assert "!radiantQuest.IsStageDone(radiantQuest.SecondObjective)" in body
    # Repeated dialogue, Random-flagged carriers and shared INFOs are idempotent.
    assert (
        "!radiantQuest.IsStageDone(radiantQuest.QuestStageReturnToQuestGiver)" in body
    )
    assert (
        body.count("radiantQuest.SetStage(radiantQuest.QuestStageReturnToQuestGiver)")
        == 1
    )

    # It is a stage producer, not a substitute for the missing request producer
    # and not a substitute for the blocked 300 -> 900 completion authority.
    for forbidden in (
        ".Start()",
        "StartDailyQuest",
        "StartRadiantQuestByIndex",
        "CompleteQuest",
        "QuestCompletionStage",
        "QuestShutDownStage",
        "SetObjective",
    ):
        assert forbidden not in body


def test_companion_accessor_is_an_extraction_not_a_behavior_change() -> None:
    companion = _merged("CompanionScript")
    accessor = _body(companion, "COMP_RQ_Script Function GetCurrentRadiantQuest()")
    acceptance = _body(companion, "Function SetRQAcceptanceStage()")

    # The resolution rule moved wholesale into the accessor.
    assert "Math.Floor(player.GetValue(PlayerRadiantQuestDataIndex))" in accessor
    assert "dataIndex >= 0 && dataIndex < RadiantQuestData.Length" in accessor
    assert "specificAliases.QuestTarget as COMP_RQ_Script" in accessor
    assert accessor.rstrip().endswith("Return _COMP_RQ_Ins")
    assert "Math.Floor" not in acceptance

    # The delivered acceptance route is unchanged and still targets stage 100.
    assert "_COMP_RQ_Ins = GetCurrentRadiantQuest()" in acceptance
    assert "_COMP_RQ_Ins.SetStage(_COMP_RQ_Ins.QuestStageAcceptQuest)" in acceptance
    assert "QuestStageReturnToQuestGiver" not in acceptance


def test_return_producer_symbols_resolve_through_the_papyrus_lsp(
    tmp_path: Path,
) -> None:
    merged_root = tmp_path / "merged"
    merged_root.mkdir()
    for name in (RETURN_SCRIPT, *DEPENDENCIES):
        (merged_root / _script_relative_path(name, ".psc")).write_text(
            _merged(name), encoding="utf-8"
        )

    db = ScriptDB(str(tmp_path / "lsp.db"), source_dirs=[str(merged_root)])
    try:
        assert db.has_event(RETURN_SCRIPT, "OnEnd")
        assert db.has_function("CompanionScript", "GetCurrentRadiantQuest")
        assert (
            db.get_function_return_type("CompanionScript", "GetCurrentRadiantQuest")
            == "COMP_RQ_Script"
        )
        assert db.has_property("COMP_RQ_Script", "SecondObjective")
        assert db.has_property("COMP_RQ_Script", "QuestStageReturnToQuestGiver")
        # Negative control: the accessor name is not simply being accepted.
        assert not db.has_function("CompanionScript", "GetCurrentRadiantQuestZ")
    finally:
        db.close()


@pytest.mark.parametrize("script_name", [RETURN_SCRIPT, *DEPENDENCIES])
def test_return_producer_full_merged_sources_compile_for_fo4(
    script_name: str, tmp_path: Path
) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged_root = tmp_path / "merged"
    merged_root.mkdir(exist_ok=True)
    for dependency in DEPENDENCIES:
        (merged_root / _script_relative_path(dependency, ".psc")).write_text(
            _merged(dependency), encoding="utf-8"
        )

    result = compile_psc(
        _merged(script_name),
        imports=[str(merged_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_contract_records_the_corrected_binding_census_and_the_open_blockers() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for token in (
        "requested 297, found 297, missing 0",
        "122 live INFO bindings",
        "175 live INFO bindings",
        "48 distinct `INFOGroup` roots",
        "player returned to quest giver, dialogue info",
    ):
        assert token in contract

    # The blocked edges must stay visibly blocked; no substitute is invented.
    for blocker in (
        "no client-side caller exists",
        "`GMRW` does not exist in FO4",
        "deliberately not implemented here",
        "no live binding for the parent",
    ):
        assert blocker in contract
