from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Fragments:Quests:QF_AC_SQ_PointerQuest_00748E5D"
BOUND_STAGES = (100, 300, 500, 9000)


def _patch(script_name: str = SCRIPT_NAME) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch()
    )


def test_pointer_patch_matches_live_binding_and_merges_idempotently() -> None:
    patch = _patch()
    members = [name for name, _start, _end in _members(patch)]
    fragments = {name for name in members if name.startswith("fragment_stage_")}
    expected_fragments = {
        f"fragment_stage_{stage:04d}_item_00" for stage in BOUND_STAGES
    }

    assert fragments == expected_fragments
    assert Counter(members) == Counter(set(members))
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)

    merged = _merged()
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in members:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_pointer_sends_only_bound_story_events_and_never_starts_quests_directly() -> (
    None
):
    patch = _patch()
    sq03 = _member_body(patch, "trystartsq03")
    sq05 = _member_body(patch, "trystartsq05")
    producer = _member_body(patch, "sendpointerstoryevent")

    assert "0x00729CBB" in sq03
    assert "0x00729CC2" in sq03
    assert "AC_MQ02_Stage.IsCompleted()" in sq03
    assert "SendPointerStoryEvent(startKeyword, 2)" in sq03
    assert "0x006FCF87" in sq05
    assert "0x006FCFC0" in sq05
    assert "SendPointerStoryEvent(startKeyword, 3)" in sq05
    assert "flyerAlias.GetReference()" in producer
    assert "Alias_Player.GetReference()" in producer
    assert "SendStoryEventAndWait(eventLocation, flyerRef, playerRef)" in producer
    assert ".Start()" not in patch
    assert ".Reset()" not in patch


def test_pointer_retries_failed_events_and_completes_only_after_both_targets_start() -> (
    None
):
    patch = _patch()
    timer = _member_body(patch, "ontimer")
    completion = _member_body(patch, "trycompletepointer")
    terminal = _member_body(patch, "fragment_stage_9000_item_00")

    assert "IsStageDone(300)" in timer
    assert "TryStartSQ03()" in timer
    assert "IsStageDone(500)" in timer
    assert "TryStartSQ05()" in timer
    assert "RestartPointerTimer()" in timer
    assert "!IsStageDone(300) || !IsStageDone(500)" in completion
    assert "Bool sq03Ready = TryStartSQ03()" in completion
    assert "Bool sq05Ready = TryStartSQ05()" in completion
    assert "sq03Ready && sq05Ready" in completion
    assert "SetStage(9000)" in completion
    assert "CancelTimer(1)" in terminal
    assert "SetObjectiveCompleted(10)" in terminal


def test_pointer_patch_is_reward_neutral() -> None:
    patch = _patch().casefold()
    assert "additem(" not in patch
    assert "addspell(" not in patch
    assert "questreward" not in patch
    assert "rewardcaps" not in patch
    assert "completequest(" not in patch


def test_sq03_activator_route_is_already_functional() -> None:
    activation = _patch("DefaultRefOnActivateSendEvent")
    producer = _patch("DefaultRefSendStoryEvent")

    assert "SendConfiguredStoryEvent" in activation
    assert "MyStoryManagerKeyword.SendStoryEvent" in producer


def test_pointer_full_production_merge_native_compiles_for_fo4() -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
