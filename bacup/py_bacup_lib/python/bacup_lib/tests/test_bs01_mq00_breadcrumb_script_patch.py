from __future__ import annotations

from pathlib import Path

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
SCRIPT_NAME = "Fragments:Quests:QF_BS01_MQ00_Breadcrumb_005EAD3C"
LEVEL_CONNECTOR_SCRIPT_NAME = (
    "Fragments:Quests:QF_BS01_MQ00_Breadcrumb_OnIn_005EECB8"
)
BOUND_FRAGMENT_MEMBERS = {
    "fragment_stage_0100_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
}
PATCH_MEMBERS = BOUND_FRAGMENT_MEMBERS | {
    "trystartradio",
    "trystarttrust",
    "ontimer",
}
SKELETON_DECLARATIONS = (
    "location Property LocMountainsObservatoryLocation Auto mandatory",
    "location Property LocMountainsObservatoryIntLocation Auto mandatory",
    "keyword Property BS01_Radio_IntroBroadcast_QuestStartKeyword Auto mandatory",
    "referencealias Property Alias_Player Auto mandatory",
    "quest Property BS01_MQ01_Trust Auto mandatory",
    "quest Property BS01_MQ00_Radio Auto mandatory",
    "locationalias Property Alias_Loc_ATLASInterior Auto mandatory",
    "locationalias Property Alias_Loc_FortAtlas Auto mandatory",
    "keyword Property BS01_MQ01_Trust_QuestStartKeyword Auto mandatory",
    "keyword Property BS01_MQ01_Trust_QuestActiveKeyword Auto mandatory",
)


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _merged_level_connector_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(
        LEVEL_CONNECTOR_SCRIPT_NAME, ".psc"
    )
    patch = _script_patch_source(LEVEL_CONNECTOR_SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_patch_is_member_only_and_contains_all_bound_fragments_once():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert set(_member_names(patch)) == PATCH_MEMBERS
    for member_name in BOUND_FRAGMENT_MEMBERS:
        assert _member_names(patch).count(member_name) == 1
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_production_merge_preserves_declarations_without_duplicate_members():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_production_source()
    member_names = _member_names(merged)

    for declaration in SKELETON_DECLARATIONS:
        assert merged.count(declaration) == 1
    for member_name in PATCH_MEMBERS:
        assert member_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert merged.lower().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_stage_sequence_preserves_alias_radio_and_objective_effects():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    stage_200 = _member_body(patch, "fragment_stage_0200_item_00")

    assert (
        "Alias_Loc_FortAtlas.ForceLocationTo(LocMountainsObservatoryLocation)"
        in stage_100
    )
    assert (
        "Alias_Loc_ATLASInterior."
        "ForceLocationTo(LocMountainsObservatoryIntLocation)" in stage_100
    )
    assert stage_100.count("SetObjectiveDisplayed(100)") == 1
    assert stage_100.count("TryStartRadio()") == 1

    radio_start = _member_body(patch, "trystartradio")
    radio_send = (
        "accepted = BS01_Radio_IntroBroadcast_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert radio_start.count(radio_send) == 1
    assert radio_start.count("BS01_MQ00_Radio.IsRunning()") == 2
    assert radio_start.count("BS01_MQ00_Radio.IsCompleted()") == 2
    assert radio_start.count("StartTimer(5.0, 100)") == 1
    assert radio_start.index(radio_send) < radio_start.index("StartTimer(5.0, 100)")
    assert radio_start.count("GetStage() <= 100") == 2

    assert stage_200.count("SetObjectiveCompleted(100)") == 1
    assert stage_200.count("SetObjectiveDisplayed(200)") == 1


def test_completed_stage_hands_off_to_inactive_forging_trust_through_story_manager():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    stage_300 = _member_body(patch, "fragment_stage_0300_item_00")
    trust_start = _member_body(patch, "trystarttrust")
    trust_send = (
        "accepted = BS01_MQ01_Trust_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )

    assert stage_300.count("SetObjectiveCompleted(200)") == 1
    assert stage_300.count("TryStartTrust()") == 1
    assert "Stop()" not in stage_300
    assert "BS01_MQ00_Radio.SetStage(9000)" not in stage_300

    assert trust_start.count(trust_send) == 1
    assert trust_start.count("BS01_MQ01_Trust.IsRunning()") == 2
    assert trust_start.count("BS01_MQ01_Trust.IsCompleted()") == 2
    assert trust_start.count("StartTimer(5.0, 300)") == 1
    assert trust_start.index(trust_send) < trust_start.index("If accepted")
    assert trust_start.index("If accepted") < trust_start.index(
        "BS01_MQ00_Radio.SetStage(9000)"
    )
    assert trust_start.index("BS01_MQ00_Radio.SetStage(9000)") < trust_start.index(
        "Stop()"
    )
    assert trust_start.index("Stop()") < trust_start.index("StartTimer(5.0, 300)")


def test_rejected_story_events_rearm_only_their_dedicated_retry_timer():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    timer = _member_body(patch, "ontimer")

    assert timer.count("aiTimerID == 100") == 1
    assert timer.count("aiTimerID == 300") == 1
    assert timer.count("TryStartRadio()") == 1
    assert timer.count("TryStartTrust()") == 1
    assert timer.index("aiTimerID == 100") < timer.index("TryStartRadio()")
    assert timer.index("aiTimerID == 300") < timer.index("TryStartTrust()")
    assert "GetStage() <= 100" in timer
    assert "GetStage() >= 300" in timer


def test_handoffs_use_waiting_story_events_without_direct_quest_start():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert patch.count("SendStoryEventAndWait") == 2
    assert ".SendStoryEvent(" not in patch
    assert ".Start()" not in patch


def test_full_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_level_connector_waits_for_level_20_then_sends_story_event_once():
    patch = _script_patch_source(LEVEL_CONNECTOR_SCRIPT_NAME)
    assert patch is not None
    assert set(_member_names(patch)) == {
        "fragment_stage_0100_item_00",
        "trystartbreadcrumb",
        "ontimer",
    }
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    attempt = _member_body(patch, "trystartbreadcrumb")
    send = (
        "BS01_MQ00_Breadcrumb_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert attempt.index("playerRef.GetLevel() < 20") < attempt.index(
        "StartTimer(30.0, 100)"
    )
    timer_start = attempt.index("StartTimer(30.0, 100)")
    assert timer_start < attempt.index("Return", timer_start)
    assert attempt.count(send) == 1
    assert attempt.index(send) < attempt.rindex("Stop()")
    assert ".Start()" not in attempt

    timer = _member_body(patch, "ontimer")
    assert timer.count("aiTimerID == 100") == 1
    assert timer.count("TryStartBreadcrumb()") == 1


def test_level_connector_merge_preserves_properties_and_native_compiles():
    merged = _merged_level_connector_source()
    assert merged.count(
        "keyword Property BS01_MQ00_Breadcrumb_QuestStartKeyword Auto mandatory"
    ) == 1
    assert merged.count("referencealias Property Alias_Player Auto mandatory") == 1
    for member_name in (
        "fragment_stage_0100_item_00",
        "trystartbreadcrumb",
        "ontimer",
    ):
        assert _member_names(merged).count(member_name) == 1

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(LEVEL_CONNECTOR_SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
