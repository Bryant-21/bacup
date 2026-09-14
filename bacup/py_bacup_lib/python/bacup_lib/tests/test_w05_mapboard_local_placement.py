from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
TODO = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"

BOARD = "W05_RE_MapBoardActivatorScript"
MASTER = "W05_RE_MapMasterDummyScript"
SEGMENT = "W05_RE_MapSegmentDummyScript"
QUEST = "Fragments:Quests:QF_W05_MQ_000P_005698E4"

TOPOLOGY = (
    (1, "W05_Map1_ActorValue", 1100, 1150, "pW05_Topo01", "pW05_Message_Topo01"),
    (2, "W05_Map2_ActorValue", 1200, 1250, "pW05_Topo02", "pW05_Message_Topo02"),
    (3, "W05_Map3_ActorValue", 1300, 1350, "pW05_Topo03", "pW05_Message_Topo03"),
    (4, "W05_Map4_ActorValue", 1400, 1450, "pW05_Topo04", "pW05_Message_Topo04"),
    (5, "W05_Map5_ActorValue", 1500, 1550, "pW05_Topo05", "pW05_Message_Topo05"),
    (6, "W05_Map6_ActorValue", 1600, 1650, "pW05_Topo06", "pW05_Message_Topo06"),
)


def _merged_source(script_name: str) -> str:
    pex_path = DEPLOYED_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), pex_path
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(
        decompile_pex(pex_path, fo4_api_compat=True), patch
    )


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def test_board_uses_the_exact_six_ready_av_and_placed_stage_mappings():
    merged = _merged_source(BOARD)
    on_activate = _member_body(merged, "onactivate")
    place = _member_body(merged, "placereadyfragment")

    assert (
        "activatingPlayer == None || activatingPlayer != Game.GetPlayer()"
        in on_activate
    )
    assert "ThisPlayersQuest = W05_MQ_000P" in on_activate
    assert "GetLinkedRef(W05_RE_EntranceTrigger_Keyword)" in on_activate
    assert "GetLinkedRef(W05_RE_MapMasterDummyMarker_Keyword)" in on_activate
    for segment_index, actor_value, _acquired, placed, _item, _message in TOPOLOGY:
        assert (
            f"PlaceReadyFragment(activatingPlayer, {actor_value}, {placed}, "
            f"{segment_index})"
        ) in on_activate

    assert "playerRef.GetValue(mapValue) != 1.0" in place
    assert "!ThisPlayersQuest.IsStageDone(placedStage)" in place
    assert place.index("ThisPlayersQuest.SetStage(placedStage)") < place.index(
        "playerRef.SetValue(mapValue, 2.0)"
    )
    assert "MapMasterDummyMarker.RevealMapSegment(segmentIndex)" in place
    assert "!ThisPlayersQuest.IsStageDone(1999)" in on_activate
    assert "ThisPlayersQuest.SetStage(1999)" in on_activate
    for _segment_index, actor_value, _acquired, _placed, _item, _message in TOPOLOGY:
        assert f"{actor_value} != None" in on_activate
        assert f"activatingPlayer.GetValue({actor_value}) >= 2.0" in on_activate


def test_topographic_quest_fragments_arm_place_and_finish_without_objectives():
    merged = _merged_source(QUEST)
    expected = []
    for _segment_index, actor_value, acquired, placed, item, message in TOPOLOGY:
        acquired_name = f"fragment_stage_{acquired:04d}_item_00"
        placed_name = f"fragment_stage_{placed:04d}_item_00"
        expected.extend((acquired_name, placed_name))

        acquired_body = _member_body(merged, acquired_name)
        assert "Alias_Player.GetActorReference()" in acquired_body
        assert "If playerRef == None" in acquired_body
        assert f"GetValue({actor_value}) < 1.0" in acquired_body
        assert f"SetValue({actor_value}, 1.0)" in acquired_body
        assert f"{message}.Show()" in acquired_body
        assert acquired_body.index(f"GetValue({actor_value}) < 1.0") < (
            acquired_body.index(f"{message}.Show()")
        )

        placed_body = _member_body(merged, placed_name)
        assert "If playerRef == None" in placed_body
        assert f"GetValue({actor_value}) < 2.0" in placed_body
        assert f"SetValue({actor_value}, 2.0)" in placed_body
        assert f"GetItemCount({item}) > 0" in placed_body
        assert f"RemoveItem({item}, 1, True)" in placed_body
        assert placed_body.index(f"GetValue({actor_value}) < 2.0") < (
            placed_body.index(f"RemoveItem({item}, 1, True)")
        ) < placed_body.index(f"SetValue({actor_value}, 2.0)")

        assert "SetObjective" not in acquired_body + placed_body
        assert "CompleteQuest" not in acquired_body + placed_body

    completion = _member_body(merged, "fragment_stage_1999_item_00")
    assert "playerRef == None || pW05_MQ00_CodeAV == None" in completion
    assert "mapCode < 100000 || mapCode > 999999" in completion
    assert "Utility.RandomInt(100000, 999999)" in completion
    assert "SetObjective" not in completion
    assert "CompleteQuest" not in completion

    expected.extend(
        (
            "fragment_stage_1999_item_00",
            "fragment_stage_2100_item_00",
            "fragment_stage_2200_item_00",
            "fragment_stage_2300_item_00",
        )
    )
    assert _member_names(merged) == expected


def test_visual_reveal_follows_only_the_authored_master_and_segment_links():
    master = _merged_source(MASTER)
    segment = _merged_source(SEGMENT)
    reveal = _member_body(master, "revealmapsegment")
    segment_reveal = _member_body(segment, "revealmapsegment")
    on_load = _member_body(master, "onload")
    digit_expressions = (
        "MapNumbers / 100000",
        "(MapNumbers / 10000) % 10",
        "(MapNumbers / 1000) % 10",
        "(MapNumbers / 100) % 10",
        "(MapNumbers / 10) % 10",
        "MapNumbers % 10",
    )

    for segment_index, digit_expression in enumerate(digit_expressions, start=1):
        assert f"segmentIndex == {segment_index}" in reveal
        assert f"GetLinkedRef(W05_RE_MapSegment{segment_index}_Keyword)" in reveal
        assert digit_expression in reveal
        assert digit_expression in on_load
    assert "segmentScript.RevealMapSegment(numberToShow)" in reveal
    assert "GetLinkedRef(W05_RE_MapSegment_Keyword)" in segment_reveal
    assert segment_reveal.index("MapSegment.Enable(False)") < segment_reveal.index(
        "ShowNumber(numberToShow)"
    )
    assert "PlaceAtMe" not in segment_reveal
    assert "PlayerToCheck == None || W05_MQ00_CodeAV == None" in on_load
    for member in (on_load, reveal):
        assert "MapNumbers < 100000 || MapNumbers > 999999" in member
        assert "Utility.RandomInt(100000, 999999)" in member
        assert "PlayerToCheck.SetValue(W05_MQ00_CodeAV, MapNumbers)" in member


def test_mapboard_todo_narrowing_closes_only_the_thirteen_restored_stages():
    todo_text = TODO.read_text(encoding="utf-8")
    quest_row = next(
        line
        for line in todo_text.splitlines()
        if "script=Fragments:Quests:QF_W05_MQ_000P_005698E4|" in line
    )
    remaining = {
        "0100:00",
        "0150:00",
        "0200:00",
        "0250:00",
        "0300:00",
        "0350:00",
        "0400:00",
        "0450:00",
        "0500:00",
        "0550:00",
        "0999:00",
    }
    closed = {
        *(f"{stage:04d}:00" for stage in range(1100, 1700, 50)),
        "1999:00",
    }
    assert f"members={','.join(sorted(remaining))}|" in quest_row
    assert not any(member in quest_row for member in closed)
    assert "W05-AUDIT-TODO|script=W05_RE_MapBoardActivatorScript|" not in todo_text


def test_mapboard_merge_is_idempotent_for_all_four_scripts():
    for script_name in (BOARD, MASTER, SEGMENT, QUEST):
        pex_path = DEPLOYED_ROOT / _script_relative_path(script_name, ".pex")
        skeleton = decompile_pex(pex_path, fo4_api_compat=True)
        patch = _script_patch_source(script_name)
        assert patch is not None
        merged = _merge_script_method_patches(skeleton, patch)
        assert _merge_script_method_patches(merged, patch) == merged


def test_mapboard_repair_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    import_root = tmp_path / "imports"
    import_root.mkdir()
    merged_sources = {
        script_name: _merged_source(script_name)
        for script_name in (BOARD, MASTER, SEGMENT, QUEST)
    }
    for script_name in (BOARD, MASTER, SEGMENT):
        source_path = import_root / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(merged_sources[script_name], encoding="utf-8")

    for script_name, source in merged_sources.items():
        result = compile_psc(
            source,
            imports=[str(import_root), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}: {diagnostics}"
        assert result.pex_bytes is not None
