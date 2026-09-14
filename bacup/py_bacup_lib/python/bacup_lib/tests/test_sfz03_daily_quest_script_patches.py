from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import (
    _fo4_base_source,
)
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)
CRYPTID_ALIAS = "SFZ03_Queen_CryptidAliasScript"
PLAYER_ALIAS = "SFZ03_Queen_PlayerScript"
QUEST_SCRIPT = "SFZ03_Queen_QuestScript"
PERK_SCRIPT = "sfz03_queen_perkscript"
QUEST_FRAGMENT = "Fragments:Quests:QF_SFZ03_Queen_000451FC"

PATCH_MEMBERS = {
    QUEST_SCRIPT: {
        "onquestinit",
        "onquestshutdown",
        "registerruntimeevents",
        "unregisterruntimeevents",
        "objectreference.ontriggerenter",
        "objectreference.onactivate",
        "resethuntstate",
        "inspectcryptidcollection",
        "handlecryptidencounter",
        "handlecryptiddeath",
        "harvestcryptidsample",
        "setcreaturestagefromactor",
        "setcreaturestage",
        "castcryptidknowledge",
    },
    CRYPTID_ALIAS: {
        "oncombatstatechanged",
        "onhit",
        "ondeath",
        "onactivate",
    },
    PLAYER_ALIAS: {"onkill"},
    PERK_SCRIPT: {"onentryrun"},
    QUEST_FRAGMENT: {
        "fragment_stage_0000_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0120_item_00",
        "fragment_stage_0130_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0151_item_00",
        "fragment_stage_0155_item_00",
        "fragment_stage_0160_item_00",
        "fragment_stage_0161_item_00",
        "fragment_stage_0165_item_00",
        "fragment_stage_0170_item_00",
        "fragment_stage_0171_item_00",
        "fragment_stage_0175_item_00",
        "fragment_stage_0180_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0230_item_00",
        "fragment_stage_0240_item_00",
        "fragment_stage_0250_item_00",
        "fragment_stage_0260_item_00",
        "fragment_stage_0270_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0410_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_1000_item_00",
    },
}


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


def _remote_event_arguments(source: str, method_name: str) -> set[str]:
    prefix = f"{method_name}("
    return {
        stripped[len(prefix) : -1]
        for line in source.splitlines()
        if (stripped := line.strip()).startswith(prefix)
        and stripped.endswith(")")
    }


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_sfz03_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


def test_sfz03_patches_merge_once_and_native_compile(tmp_path: Path):
    merged_sources = {
        script_name: _merged_production_source(script_name)
        for script_name in PATCH_MEMBERS
    }
    for script_name, merged in merged_sources.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        names = _member_names(merged)
        for member in PATCH_MEMBERS[script_name]:
            assert names.count(member) == 1
        assert _merge_script_method_patches(merged, patch) == merged

        temporary_source = tmp_path / _script_relative_path(
            script_name, ".psc"
        )
        temporary_source.parent.mkdir(parents=True, exist_ok=True)
        temporary_source.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    for script_name, merged in merged_sources.items():
        result = compile_psc(
            merged,
            imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )

        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}\n{diagnostics}"
        assert result.pex_bytes is not None


def test_sfz03_runtime_events_restore_local_terminal_and_analyzer_edges():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    registration = _member_body(patch, "registerruntimeevents")
    shutdown = _member_body(patch, "onquestshutdown")
    cleanup = _member_body(patch, "unregisterruntimeevents")
    trigger = _member_body(patch, "objectreference.ontriggerenter")
    analyzer = _member_body(patch, "objectreference.onactivate")

    assert "GetAlias(50)" in registration
    assert "GetAlias(51)" in registration
    assert "GetAlias(52)" in registration
    assert "GetAlias(10)" in registration
    assert 'RegisterForRemoteEvent(siteOneTrigger, "OnTriggerEnter")' in registration
    assert 'RegisterForRemoteEvent(analyzerRef, "OnActivate")' in registration
    assert "UnregisterRuntimeEvents()" in shutdown
    assert "ResetHuntState()" in shutdown
    assert _remote_event_arguments(
        registration, "RegisterForRemoteEvent"
    ) == _remote_event_arguments(cleanup, "UnregisterForRemoteEvent")
    assert _remote_event_arguments(
        cleanup, "UnregisterForRemoteEvent"
    ) == {
        'siteOneTrigger, "OnTriggerEnter"',
        'siteTwoTrigger, "OnTriggerEnter"',
        'siteThreeTrigger, "OnTriggerEnter"',
        'analyzerRef, "OnActivate"',
    }
    assert "akActionRef != Game.GetPlayer()" in trigger
    assert "SetStage(110)" in trigger
    assert "SetStage(120)" in trigger
    assert "SetStage(130)" in trigger
    assert "akActionRef != playerRef" in analyzer
    assert "playerRef.RemoveItem(sampleForm, 1, True)" in analyzer
    assert "SetStage(400)" in analyzer


def test_sfz03_cryptid_events_restore_solo_kill_and_harvest():
    quest = _script_patch_source(QUEST_SCRIPT)
    cryptid_alias = _script_patch_source(CRYPTID_ALIAS)
    player_alias = _script_patch_source(PLAYER_ALIAS)
    perk = _script_patch_source(PERK_SCRIPT)
    assert quest is not None
    assert cryptid_alias is not None
    assert player_alias is not None
    assert perk is not None

    encounter = _member_body(quest, "handlecryptidencounter")
    death = _member_body(quest, "handlecryptiddeath")
    harvest = _member_body(quest, "harvestcryptidsample")

    assert "akCryptid.HasKeyword(SFZ03_Queen_CryptidBossKeyword)" in encounter
    assert "SetStage(180)" in encounter
    assert "deathMarkerRef.MoveTo(cryptid)" in death
    assert "SetStage(200)" in death
    assert "akActionRef != playerRef" in harvest
    assert "playerRef.AddItem(sampleForm, 1, True)" in harvest
    assert "SetStage(300)" in harvest
    assert "hunt.HandleCryptidDeath(akVictim, None)" in player_alias
    assert "hunt.HandleCryptidDeath(akSenderRef, Self)" in cryptid_alias
    assert (
        "hunt.HarvestCryptidSample(akSenderRef, akActionRef)"
        in cryptid_alias
    )
    assert "hunt.HarvestCryptidSample(akTarget, akOwner)" in perk


def test_sfz03_fragments_restore_first_repeat_and_analyzer_completion():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    startup = _member_body(patch, "fragment_stage_0010_item_00")
    locations = _member_body(patch, "fragment_stage_0100_item_00")
    sample = _member_body(patch, "fragment_stage_0300_item_00")
    analyzer = _member_body(patch, "fragment_stage_0400_item_00")
    buff = _member_body(patch, "fragment_stage_0500_item_00")
    completion = _member_body(patch, "fragment_stage_1000_item_00")

    assert (
        "playerRef.GetValue(SFZ03_Queen_QuestCompletedValue) > 0.0"
        in startup
    )
    assert "SetStage(100)" in startup
    assert "SetStage(50)" in startup
    assert "SetObjectiveDisplayed(100, True)" in locations
    assert "SetObjectiveDisplayed(110, True)" in locations
    assert "SetObjectiveDisplayed(120, True)" in locations
    assert "If Alias_MapMarker != None" in locations
    assert "SetObjectiveCompleted(200, True)" in sample
    assert "SetObjectiveDisplayed(300, True)" in sample
    assert "SFZ03_Queen_AnalyzerScene.Start()" in analyzer
    assert "hunt.CastCryptidKnowledge()" in buff
    assert "SetStage(1000)" in buff
    assert (
        "playerRef.SetValue(SFZ03_Queen_QuestCompletedValue, 1.0)"
        in completion
    )
    assert "Stop()" in completion


def test_sfz03_does_not_invent_a_replacement_for_missing_wave_spawn_data():
    combined = "\n".join(
        _script_patch_source(script_name) or ""
        for script_name in PATCH_MEMBERS
    )

    assert "PlaceAtMe" not in combined
    assert "SendStoryEvent" not in combined
    assert "DefaultQuestEncounterWaveScript" not in combined
