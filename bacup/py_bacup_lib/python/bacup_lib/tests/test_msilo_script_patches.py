from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _fo76_to_fo4_script_type,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

CORE_PATCHES = {
    "MSiloBreadcrumbTriggerScript": {"ontriggerenter"},
    "MSiloHandScannerLoadScript": {"onload", "onactivate", "ensuresiloquests"},
    "MSiloIDCardActivatorScript": {"onload", "resolveresidential"},
    "MSiloLaserGridScript": {"requestcollisionupdate"},
    "MSiloOperationsPanelActivatorScript": {"onload", "resolveoperations"},
    "MSiloPersonalQuestScript": {
        "onquestinit",
        "preparesiloaliases",
        "ensuresilostarted",
        "beginsilo",
        "handlestage",
    },
    "MSiloQuestScript_Control": {
        "onquestinit",
        "startlaunchprep",
        "completelaunchprep",
        "replacelaunchchief",
    },
    "MSiloQuestScript_Main": {"onquestinit", "initialize", "selectlocation"},
    "MSiloQuestScript_Operations": {
        "onquestinit",
        "handlepaneldestroyed",
        "setlasergridsopen",
    },
    "MSiloQuestScript_Reactor": {
        "onquestinit",
        "shutdownreactor",
        "restartreactor",
        "overridesecuritylockdown",
    },
    "MSiloQuestScript_Residential": {
        "onquestinit",
        "handleidcardactivation",
        "fabricateandauthorizeid",
    },
    "MSiloQuestScript_Storage": {
        "onquestinit",
        "handlepanelactivation",
        "opensecuritydoor",
    },
    "MSiloStartupQuestScript": {
        "onquestinit",
        "ontimer",
        "startsiloquests",
        "startpreparedsilo",
        "handlelocation",
    },
    "MSiloStoragePanelActivatorScript": {"onload", "resolvestorage"},
    "MSiloTerminalTextReplacementScript": {"onmenuitemrun", "refreshterminal"},
}

TERMINAL_PATCHES = {
    "TERM_MSilo_Control_LaunchCon_003E4837": {1},
    "TERM_MSilo_Control_LaunchCon_010020BE": {1},
    "TERM_MSilo_Control_RobotFabr_003E482D": {1, 2, 3, 4, 5},
    "TERM_MSilo_Control_RobotFabr_010020B1": {1, 2, 3, 4, 5},
    "TERM_MSilo_Reactor_ReactorCo_003DE7D2": {1, 2, 4},
    "TERM_MSilo_Reactor_ReactorSe_003DE7D4": {1},
    "TERM_MSilo_Reactor_SecurityC_00530BED": {1},
    "TERM_MSilo_Residential_IDCar_003DE7A4": {1, 2},
    "TERM_MSilo_Residential_IDCar_003DE7AB": {3},
    "TERM_MSilo_Storage_Facilitie_0051AFF7": {2},
    "TERM_MSilo_Storage_Facilitie_0051B04A": {1},
    "TERM_MSilo_Storage_Facilitie_01001DE3": {1},
}

PERSONAL_STAGE_FRAGMENTS = {
    10,
    11,
    19,
    100,
    110,
    120,
    130,
    140,
    150,
    160,
    170,
    180,
    198,
    200,
    210,
    220,
    230,
    240,
    250,
    298,
    300,
    310,
    320,
    398,
    400,
    410,
    419,
    420,
    430,
    440,
    498,
    500,
    510,
    520,
    530,
    598,
    1000,
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


def _member_names(patch: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }


@pytest.mark.parametrize(("script_name", "expected"), CORE_PATCHES.items())
def test_msilo_core_patch_supplies_local_behavior(
    script_name: str, expected: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected <= _member_names(patch)


@pytest.mark.parametrize(("base_name", "fragment_ids"), TERMINAL_PATCHES.items())
def test_msilo_terminal_patch_supplies_vmad_fragments(
    base_name: str, fragment_ids: set[int]
):
    patch = _script_patch_source(f"Fragments:Terminals:{base_name}")

    assert patch is not None
    expected = {f"fragment_terminal_{fragment_id:02d}" for fragment_id in fragment_ids}
    assert expected <= _member_names(patch)


def test_msilo_personal_patch_supplies_every_vmad_stage_fragment():
    patch = _script_patch_source("Fragments:Quests:QF_MSiloPersonal_003E03AA")

    assert patch is not None
    expected = {
        f"fragment_stage_{stage:04d}_item_00" for stage in PERSONAL_STAGE_FRAGMENTS
    }
    assert expected <= _member_names(patch)


def _generated_pex_path(script_name: str) -> Path:
    relative = _script_relative_path(script_name, ".pex")
    path = GENERATED_SCRIPT_ROOT / relative
    assert path.is_file(), path
    return path


def _merged_source(script_name: str) -> str:
    skeleton = decompile_pex(
        _generated_pex_path(script_name),
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_msilo_patch_set_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    script_names = [
        *CORE_PATCHES,
        *(f"Fragments:Terminals:{name}" for name in TERMINAL_PATCHES),
        "Fragments:Quests:QF_MSiloPersonal_003E03AA",
    ]
    merged_sources: dict[str, str] = {}
    for script_name in script_names:
        source = _merged_source(script_name)
        merged_sources[script_name] = source
        source_path = tmp_path / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(source, encoding="utf-8")

    for script_name, source in merged_sources.items():
        result = compile_psc(
            source,
            imports=[str(tmp_path), str(GENERATED_SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}:\n{diagnostics}"
        assert result.pex_bytes is not None


def test_msilo_start_paths_return_explicit_quest_state_after_orchestration():
    personal = _merged_source("MSiloPersonalQuestScript")
    startup = _merged_source("MSiloStartupQuestScript")
    scanner = _merged_source("MSiloHandScannerLoadScript")
    assert "playerAlias.ForceRefTo(player)" in personal
    assert "MSiloLocation.ForceLocationTo(canonicalLocation)" in personal
    assert "Return StartPreparedSilo(akLocation)" not in startup
    assert "Return startup.StartPreparedSilo(akLocation, Self)" not in scanner
    assert "startup.StartPreparedSilo(akLocation, Self)" in scanner
    assert "personalQuest.Reset()" in personal
    expected_result = (
        "Return (managerQuest.IsRunning() || managerQuest.IsCompleted()) && "
        "(personalQuest.IsRunning() || personalQuest.IsCompleted())"
    )
    assert startup.count(expected_result) == 2
    assert expected_result in scanner
    assert "personalQuest.Start()" not in personal


def test_msilo_story_manager_aliases_are_prepared_before_silo_entry():
    personal = _script_patch_source("MSiloPersonalQuestScript")
    master = _script_patch_source("EN07_NukeMasterScript")
    keypad = _script_patch_source("EN07_ExternalKeypadAliasScript")
    assert personal is not None
    assert master is not None
    assert keypad is not None
    assert "startup.StartPreparedSilo(CodeData[siloID].SiloLocation, akEntryRef)" in master
    prepare_index = keypad.index("PrepareLocalSiloEntry(LaunchCardValue, GetReference())")
    assert prepare_index < keypad.index("accessPanel.BlockActivation(False, False)")
    assert "managerQuest.Start()" not in personal


def test_msilo_starts_manager_then_personal_through_exact_story_events():
    startup = _merged_source("MSiloStartupQuestScript")
    assert (
        'Game.GetFormFromFile(0x003E03AB, "SeventySix.esm") as Keyword'
        in startup
    )
    manager_send = (
        "MSiloQuestKeyword.SendStoryEventAndWait(akLocation, player, eventRef, 0, 0)"
    )
    personal_send = (
        "personalQuestKeyword.SendStoryEventAndWait(eventLocation, player, "
        "eventRef, 0, 0)"
    )
    manager_send_index = startup.index(manager_send)
    manager_verified_index = startup.index(
        "If !managerQuest.IsRunning() && !managerQuest.IsCompleted()",
        manager_send_index,
    )
    alias_prepare_index = startup.index(
        "personal.PrepareSiloAliases(akLocation)", manager_verified_index
    )
    personal_send_index = startup.index(personal_send)
    assert (
        manager_send_index
        < manager_verified_index
        < alias_prepare_index
        < personal_send_index
    )
    assert startup.count(".SendStoryEventAndWait(") == 2
    assert "managerQuest.GetAlias(2)" in startup


def test_msilo_stage_helper_cannot_advance_a_failed_story_start():
    personal = _merged_source("MSiloPersonalQuestScript")
    helper_start = personal.index("Function TryToSetStage(Int aiStage)")
    running_guard = personal.index(
        "If personalQuest == None || !personalQuest.IsRunning()", helper_start
    )
    set_stage = personal.index("SetStage(aiStage)", running_guard)
    assert running_guard < set_stage


def test_msilo_interactions_never_direct_start_event_scoped_manager():
    scripts = [
        "MSiloPersonalQuestScript",
        "MSiloStartupQuestScript",
        "MSiloHandScannerLoadScript",
        "MSiloIDCardActivatorScript",
        "MSiloOperationsPanelActivatorScript",
        "MSiloStoragePanelActivatorScript",
        "Fragments:Terminals:TERM_MSilo_Storage_Facilitie_0051AFF7",
        "Fragments:Terminals:TERM_MSilo_Storage_Facilitie_0051B04A",
        "Fragments:Terminals:TERM_MSilo_Storage_Facilitie_01001DE3",
    ]
    for script_name in scripts:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert ".Start()" not in patch


@pytest.mark.parametrize(
    "script_name",
    [
        "MSiloBreadcrumbTriggerScript",
        "MSiloHandScannerLoadScript",
        "MSiloQuestScript_Residential",
        "MSiloQuestScript_Reactor",
        "MSiloQuestScript_Operations",
        "MSiloQuestScript_Storage",
        "MSiloQuestScript_Control",
        "Fragments:Quests:QF_MSiloPersonal_003E03AA",
    ],
)
def test_msilo_callers_do_not_raw_start_unfilled_personal_quest(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "personalQuest.Start()" not in patch
    assert "MSiloPersonal.Start()" not in patch


def test_msilo_stage_route_reaches_reward_checks_and_stops_after_launch():
    personal = _script_patch_source("MSiloPersonalQuestScript")
    master = _script_patch_source("EN07_NukeMasterScript")
    assert personal is not None
    assert master is not None
    for stage in (198, 199, 298, 299, 398, 399, 498, 499, 598, 599):
        assert f"TryToSetStage({stage})" in personal
    assert "personalQuest.SetStage(1000)" in master
    assert "Stop()" in personal
