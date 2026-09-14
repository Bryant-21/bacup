from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_MEMBERS = {
    "LookoutTowerSurveyTriggerScript": {"onactivate"},
    "LookoutTowerQuestScript": {"onquestinit"},
}


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


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / f"{script_name}.psc"
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def test_patches_are_member_only():
    for script_name, expected_members in PATCH_MEMBERS.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert set(_member_names(patch)) == expected_members
        assert not any(
            line.strip().lower().startswith(("scriptname ", "state "))
            for line in patch.splitlines()
        )
        assert not any(
            " property " in f" {line.strip().lower()} " for line in patch.splitlines()
        )


def test_activation_waits_for_success_before_marking_tower_surveyed():
    patch = _script_patch_source("LookoutTowerSurveyTriggerScript")
    assert patch is not None
    activate = _member_body(patch, "onactivate")

    assert "akActionRef != Game.GetPlayer()" in activate
    assert "akActionRef.GetValue(ThisTowerValue) > 0.0" in activate
    assert "SendStoryEventAndWait(None, Self, akActionRef)" in activate
    assert "If !questStarted" in activate
    assert activate.index("SendStoryEventAndWait") < activate.index(
        "akActionRef.SetValue(ThisTowerValue, 1.0)"
    )
    assert "SendStoryEvent(" not in activate


def test_quest_reveals_only_hidden_markers_reports_count_and_stops():
    patch = _script_patch_source("LookoutTowerQuestScript")
    assert patch is not None
    quest_init = _member_body(patch, "onquestinit")

    for expected in [
        "MapMarkersToReveal.GetCount()",
        "MapMarkersToReveal.GetAt(markerIndex)",
        "!marker.IsMapMarkerVisible()",
        "marker.AddToMap()",
        "revealedCount += 1",
        "LookoutTowerSurveyMessage.Show(revealedCount)",
        "Stop()",
    ]:
        assert expected in quest_init
    visibility_checks = [
        index
        for index in range(len(quest_init))
        if quest_init.startswith("marker.IsMapMarkerVisible()", index)
    ]
    assert len(visibility_checks) == 2
    assert (
        quest_init.index("marker.AddToMap()")
        < visibility_checks[1]
        < quest_init.index("revealedCount += 1")
    )
    assert "Game.GetFormFromFile" not in quest_init


def test_production_merges_are_unique_and_idempotent():
    for script_name, expected_members in PATCH_MEMBERS.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        merged = _merged_production_source(script_name)
        names = _member_names(merged)

        for member_name in expected_members:
            assert names.count(member_name) == 1
            assert _member_body(merged, member_name) == _member_body(patch, member_name)
        assert _merge_script_method_patches(merged, patch) == merged


def test_full_production_merges_native_compile_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for script_name in PATCH_MEMBERS:
        result = compile_psc(
            _merged_production_source(script_name),
            imports=[str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=f"{script_name}.psc",
        )

        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, diagnostics
        assert result.pex_bytes is not None
