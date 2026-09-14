from __future__ import annotations

from bacup_lib.tests.test_refuge_daily_quest_script_patches import (
    HUB_QUEST,
    SOURCE_ROOT,
    _member_body,
    _merged_production_source,
)
from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


def test_orlando_scene_checkpoints_advance_the_local_quest_route():
    hub = _script_patch_source(HUB_QUEST)
    assert hub is not None

    stage_event = _member_body(hub, "onstageset")
    assert "auiStageID == 301 && !IsStageDone(300)" in stage_event
    assert "SetStage(300)" in stage_event
    assert "auiStageID == 304 && !IsStageDone(310)" in stage_event
    assert "SetStage(310)" in stage_event
    assert stage_event.index("SetStage(310)") < stage_event.index("StartTimer(1.0, 1)")


def test_orlando_progression_patch_merges_once_and_compiles_for_fo4():
    merged = _merged_production_source(HUB_QUEST)
    member_names = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert member_names.count("onstageset") == 1

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(HUB_QUEST, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
