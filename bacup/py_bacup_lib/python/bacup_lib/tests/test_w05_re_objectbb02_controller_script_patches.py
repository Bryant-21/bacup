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
CHECK_STASH_SCRIPT = "W05_RE_ObjectBB02_CheckStashScript"
GET_HAPPY_SCRIPT = "Fragments:Scenes:SF_W05_RE_ObjectBB02_GetHapp_0056F2CB"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "w05-re-objectbb02-controller-repairs-2026-09-01.md"
)


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _patch_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch_source(script_name)
    )


def test_objectbb02_package_completion_maps_each_package_to_its_arrival_stage():
    merged = _merged_source(CHECK_STASH_SCRIPT)

    assert _member_names(merged) == {"onaliasinit", "actor.onpackageend"}
    assert "Event OnPackageEnd(" not in merged
    assert "Event OnPackageStart(" not in merged
    assert "Event OnPackageChange(" not in merged
    assert "Event Actor.OnPackageStart(" not in merged
    assert "Event Actor.OnPackageChange(" not in merged
    assert (
        'Event Actor.OnPackageEnd(Actor akSender, Package akOldPackage)'
        in merged
    )
    assert (
        "akOldPackage == W05_RE_ObjectBB02_BadGuyCharge\n"
        "        kQuest.SetStage(110)"
    ) in merged
    assert (
        "akOldPackage == W05_RE_ObjectBB02_BadGuyJogHappy\n"
        "        kQuest.SetStage(210)"
    ) in merged
    assert (
        "akOldPackage == W05_RE_ObjectBB02_BadGuyAngry\n"
        "        kQuest.SetStage(260)"
    ) in merged
    assert (
        "akOldPackage == W05_RE_ObjectBB02_SearchForPlayer\n"
        "        kQuest.SetStage(270)"
    ) in merged
    assert merged.count("kQuest.SetStage(") == 4


def test_objectbb02_package_completion_registers_the_actor_remote_event_safely():
    merged = _merged_source(CHECK_STASH_SCRIPT)

    actor_lookup = merged.find("Actor kActor = GetActorReference()")
    actor_guard = merged.find("If kActor != None")
    registration = merged.find('RegisterForRemoteEvent(kActor, "OnPackageEnd")')
    sender_guard = merged.find("If akSender == None")
    quest_guard = merged.find("kQuest == None")
    package_guard = merged.find("akOldPackage == None")
    first_package_branch = merged.find(
        "akOldPackage == W05_RE_ObjectBB02_BadGuyCharge"
    )

    assert -1 not in (
        actor_lookup,
        actor_guard,
        registration,
        sender_guard,
        quest_guard,
        package_guard,
        first_package_branch,
    )
    assert actor_lookup < actor_guard < registration
    assert sender_guard < quest_guard < package_guard < first_package_branch


def test_objectbb02_get_happy_resets_the_base_package_av_at_phase_two_end():
    merged = _merged_source(GET_HAPPY_SCRIPT)

    assert _member_names(merged) == {"fragment_phase_02_end"}
    property_guard = merged.find(
        "Alias_BadGuy == None || W05_RE_ObjectBB02_BadGuyAV == None"
    )
    actor_lookup = merged.find("Alias_BadGuy.GetActorReference()")
    actor_guard = merged.find("If kBadGuy != None")
    av_reset = merged.find(
        "kBadGuy.SetValue(W05_RE_ObjectBB02_BadGuyAV, 0.0)"
    )

    assert -1 not in (property_guard, actor_lookup, actor_guard, av_reset)
    assert property_guard < actor_lookup < actor_guard < av_reset
    assert merged.count(".SetValue(") == 1
    assert "EvaluatePackage(" not in merged
    assert "SetStage(" not in merged

    contract = CONTRACT.read_text(encoding="utf-8")
    assert "Fragment_Phase_02_End" in contract
    assert "at the end of the dialogue phase immediately before\nthe Crouch package action" in contract


@pytest.mark.parametrize("script_name", (CHECK_STASH_SCRIPT, GET_HAPPY_SCRIPT))
def test_objectbb02_controller_patches_merge_once_and_native_compile_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    patch = _patch_source(script_name)
    merged = _merged_source(script_name)
    assert _merge_script_method_patches(merged, patch) == merged

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None
