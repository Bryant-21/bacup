from __future__ import annotations

from pathlib import Path

import pytest

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

# W05_RE_* activator/trigger scripts patched with a single guarded
# OnActivate/OnTriggerEnter handler (member-fill only, no state carried across
# calls). Merged from the former w05_re_boards / w05_re_grafton_scene_v79 /
# w05_v79_wayward shards, which each patched a disjoint script subset of this
# same activator/trigger family.
PATCHED_SCRIPTS = {
    "W05_RE_BlacklightActivatorScript": {"onactivate"},
    "W05_RE_ClueBoardActivatorScript": {"onactivate", "enablecluemarker"},
    "W05_RE_V79KeypadActivatorScript": {"onactivate"},
    "W05_Vault79ElevatorDoorTriggerScript": {"ontriggerenter"},
    "W05_Wayward_IntTriggerRCScript": {"ontriggerenter"},
}

# Confirmed to carry no patch (bodyless skeletons / evidence-blocked).
UNPATCHED_SCRIPTS = (
    "W05_RE_FakeKeypadActivatorScript",
    "W05_RE_MapBoardActivatorScript",
    "W05_RE_GraftonPawnShopLoadDoorScript",
    "W05_RE_SceneZW01_TriggerScript",
    "W05_Vault79EntranceDoorTriggerScript",
    "W05_Vaut79EntranceKeypadScript",
)


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _member_name_list(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_members"), PATCHED_SCRIPTS.items())
def test_re_activator_patches_supply_expected_members(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected_members <= _member_names(patch)
    assert expected_members <= _member_names(_merged_source(script_name))


def test_blacklight_toggle_only_mutates_linked_markers_for_the_player():
    merged = _merged_source("W05_RE_BlacklightActivatorScript")

    player_guard = merged.find("akActionRef != Game.GetPlayer()")
    link_guard = merged.find("W05_RE_MapMasterDummyMarker_Keyword == None")
    children = merged.find("GetLinkedRefChildren(W05_RE_MapMasterDummyMarker_Keyword)")
    toggle = merged.find("markers[markerIndex].Disable()")

    assert -1 not in (player_guard, link_guard, children, toggle)
    assert player_guard < children
    assert link_guard < children < toggle


def test_clue_board_enables_only_evidence_owned_by_the_player():
    merged = _merged_source("W05_RE_ClueBoardActivatorScript")

    player_guard = merged.find("activatingPlayer != Game.GetPlayer()")
    first_link = merged.find("ClueEnableMarker01 = GetLinkedRef")
    first_enable = merged.find(
        "EnableClueMarker(ClueEnableMarker01, activatingPlayer, W05_Clue1_ActorValue)"
    )
    value_guard = merged.find("playerRef.GetValue(clueValue) >= 1.0")

    assert -1 not in (player_guard, first_link, first_enable, value_guard)
    assert player_guard < first_link < first_enable


def test_w05_re_v79_keypad_patch_keeps_the_bound_access_gate_and_targets():
    patch = _script_patch_source("W05_RE_V79KeypadActivatorScript")
    assert patch is not None
    assert _member_names(patch) == {"onactivate"}

    merged = _merged_source("W05_RE_V79KeypadActivatorScript")
    assert merged.lower().count("event onactivate(") == 1
    assert "activatingPlayer != Game.GetPlayer()" in merged
    assert "activatingPlayer.GetValue(W05_PlayerCanAccessVault79Entrance) <= 0.0" in merged
    assert "thisLaserGrid.Disable(False)" in merged
    assert "thisDoorToOpen.SetOpen(True)" in merged


def test_elevator_trigger_only_controls_its_two_evidenced_door_links():
    patch = _script_patch_source("W05_Vault79ElevatorDoorTriggerScript")
    assert patch is not None
    assert patch.splitlines().count("; TODO") == 1
    assert "GetLinkedRef(LinkCustom01)" in patch
    assert "GetLinkedRef(LinkCustom02)" in patch
    assert "SetOpen(shouldOpen)" in patch


def test_wayward_trigger_preserves_questline_guard_before_start_keyword():
    patch = _script_patch_source("W05_Wayward_IntTriggerRCScript")
    assert patch is not None
    assert patch.splitlines().count("; TODO") == 0
    assert "OwningPlayer.GetActorReference().GetValue(W05_Wayward_PlayerCompletedQuestline) >= 1.0" in patch
    assert "akActionRef.AddKeyword(W05_Wayward_Interior_RandomConvoHandlerStartKeyword)" in patch


@pytest.mark.parametrize(
    "script_name",
    ("W05_Vault79ElevatorDoorTriggerScript", "W05_Wayward_IntTriggerRCScript"),
)
def test_elevator_wayward_repairs_merge_once(script_name: str):
    merged = _merged_source(script_name)
    assert _member_name_list(merged).count("ontriggerenter") == 1


@pytest.mark.parametrize("script_name", sorted(UNPATCHED_SCRIPTS))
def test_bodyless_re_scripts_have_no_marker_only_patch(script_name: str):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")

    assert source_path.is_file(), source_path
    assert _member_names(source_path.read_text(encoding="utf-8")) == set()
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_re_activator_patches_native_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
