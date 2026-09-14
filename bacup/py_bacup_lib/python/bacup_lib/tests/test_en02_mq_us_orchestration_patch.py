from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
QF_SCRIPT = "Fragments:Quests:QF_EN02_MQ_Us_000293A3"
QUEST_SCRIPT = "EN02_MQ_QuestScript"

# Stage fragments declared by the EN02_MQ_Us (000293A3) QUST VMAD fragment table.
# Anything outside this set is pruned silently at build time.
DECLARED_FRAGMENT_STAGES = {
    1, 2, 3, 4, 5, 7, 8, 10, 11, 12, 15, 20, 25, 30, 35, 40, 45, 47, 50, 70,
    75, 80, 81, 82, 83, 90, 110, 120, 140, 160, 170, 175, 180, 185, 186, 187,
    188, 190, 200, 230, 240, 245, 247, 250, 260, 265, 267, 270, 280, 290, 300,
    301, 302, 315, 317, 318, 320, 330, 340, 350, 358, 359, 360, 395, 397, 400,
    405, 407, 410, 998,
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


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_qf_patch_only_targets_declared_fragment_stages():
    patch = _script_patch_source(QF_SCRIPT)
    assert patch is not None
    stages = {
        int(name[len("fragment_stage_") : -len("_item_00")])
        for name in _member_names(patch)
        if name.startswith("fragment_stage_")
    }
    assert stages <= DECLARED_FRAGMENT_STAGES, sorted(
        stages - DECLARED_FRAGMENT_STAGES
    )
    # Stage 310 has no fragment entry; its scene must not be stranded there.
    assert "fragment_stage_0310_item_00" not in _member_names(patch)
    assert "EN02_MQ_Us_0310_OverrideComplete.Start()" in _member_body(
        patch, "fragment_stage_0315_item_00"
    )


def test_qf_patch_declares_nothing_and_merges_idempotently():
    patch = _script_patch_source(QF_SCRIPT)
    assert patch is not None
    lowered = [line.strip().lower() for line in patch.splitlines()]
    assert not any(line.startswith(("scriptname ", "extends ", "state ")) for line in lowered)
    assert not any(" property " in f" {line} " for line in lowered)

    merged = _merged(QF_SCRIPT)
    names = _member_names(merged)
    assert all(names.count(name) == 1 for name in names)
    assert merged.lower().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_modus_intro_and_welcome_scenes_start_from_their_trigger_stages():
    patch = _script_patch_source(QF_SCRIPT)
    assert patch is not None
    # Stage 1 is a DEBUG stage; the intro belongs to the two stages the QUST
    # notes name ("either this or stage 10 will trigger MODUS' intro scene").
    assert "fragment_stage_0001_item_00" not in _member_names(patch)
    for stage in ("0008", "0010"):
        body = _member_body(patch, f"fragment_stage_{stage}_item_00")
        assert "If !IsStageDone(9)" in body
        assert body.count("EN02_MQ_Us_Intro.Start()") == 1
    assert (
        _member_body(patch, "fragment_stage_0025_item_00").count(
            "EN02_MQ_Us_Welcome.Start()"
        )
        == 1
    )


def test_script_filled_player_marker_aliases_are_filled_before_their_gates():
    patch = _script_patch_source(QF_SCRIPT)
    assert patch is not None
    # Each of these aliases has no fill rule (script-filled keyword marker) and
    # gates an activator/trigger whose DefaultAlias script advances the quest.
    expected = {
        "0020": "Alias_PlayerCanAccessInitialElevators",
        "0040": "Alias_PlayerCanActivateCamera",
        "0050": "Alias_PlayerCanMeetModus",
        "0090": "Alias_PlayerProceedToLounge",
        "0120": "Alias_PlayerCollectedFood",
        "0200": "Alias_PlayerCanUploadTape",
        "0240": "Alias_PlayerCanCollectInstructions",
        "0270": "PlayerCanDepositInstructions",
        "0300": "Alias_PlayerCanActivateRadarArray",
    }
    for stage, alias in expected.items():
        body = _member_body(patch, f"fragment_stage_{stage}_item_00")
        assert f"EN02_MarkPlayer({alias})" in body, stage

    # The radar console alias hosts the DefaultAliasOnActivate that sets 315,
    # and has no fill rule, so stage 300 must populate it.
    stage_300 = _member_body(patch, "fragment_stage_0300_item_00")
    assert "Alias_RadarConsoleActive.ForceRefTo(consoleRef)" in stage_300
    assert "Alias_RadarConsole.GetRef()" in stage_300


def test_laser_grid_refresh_stages_are_chained_from_their_source_stages():
    patch = _script_patch_source(QF_SCRIPT)
    assert patch is not None
    assert "SetStage(55)" in _member_body(patch, "fragment_stage_0050_item_00")
    assert "SetStage(95)" in _member_body(patch, "fragment_stage_0090_item_00")


def test_tape_upload_and_wave_cleanup_reach_the_instruction_objective():
    patch = _script_patch_source(QF_SCRIPT)
    assert patch is not None
    stage_230 = _member_body(patch, "fragment_stage_0230_item_00")
    assert "Alias_MODUSHolotape.GetRef()" in stage_230
    assert "playerRef.RemoveItem(tapeBase, 1, True, None)" in stage_230
    assert stage_230.index("Utility.Wait(1.0)") < stage_230.index("SetStage(240)")
    assert "!IsStageDone(240)" in stage_230

    stage_240 = _member_body(patch, "fragment_stage_0240_item_00")
    assert "Alias_ActiveAlarmKlaxon.ForceRefTo(klaxonRef)" in stage_240
    assert "Alias_ActiveAlarmKlaxon.GetRef() == None" in stage_240

    for stage, other in (("0245", 247), ("0247", 245)):
        body = _member_body(patch, f"fragment_stage_{stage}_item_00")
        assert f"IsStageDone({other})" in body
        assert body.count("SetStage(250)") == 1
        assert "!IsStageDone(250)" in body


def test_orbital_drop_triggers_on_340_not_the_shadowed_property_default():
    merged = _merged(QUEST_SCRIPT)
    # iTriggerDropStage's decompiled default is 350, which made the elif chain
    # swallow the stage-350 failsafe branch and stranded the quest before 360.
    assert "Int Property iTriggerDropStage = 350 Auto" in merged
    on_stage_set = _member_body(merged, "onstageset")
    assert "ElseIf auiStageID == 340\n        BeginOrbitalDrop()" in on_stage_set
    assert "auiStageID == iTriggerDropStage" not in on_stage_set
    assert on_stage_set.index("auiStageID == 340") < on_stage_set.index(
        "auiStageID == 350"
    )
