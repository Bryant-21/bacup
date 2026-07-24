from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
PARENT_SOURCE_DIR = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCH_CASES = {
    "EN07_NukeSoundCategoryMagicEffect": {
        "pex": Path("en07_nukesoundcategorymagiceffect.pex"),
        "members": {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "ontimer"),
        },
    },
    "Nuke_CodePageRefScript": {
        "pex": Path("nuke_codepagerefscript.pex"),
        "members": {("event", "onread")},
    },
}


def _fo4_base_source() -> Path | None:
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    candidates: list[Path] = []
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


def _merged_production_source(script_name: str) -> str:
    case = PATCH_CASES[script_name]
    pex_path = DEPLOYED_SCRIPT_ROOT / case["pex"]
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_is_a_method_fragment(script_name: str):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    actual_members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert PATCH_CASES[script_name]["members"] <= actual_members


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_merges_into_production_skeleton(script_name: str):
    merged = _merged_production_source(script_name)

    assert merged.lower().count("scriptname ") == 1
    for _kind, member_name in PATCH_CASES[script_name]["members"]:
        assert merged.lower().count(f"event {member_name}(") == 1


def test_sound_category_effect_push_guarded_by_none_check():
    merged = _merged_production_source("EN07_NukeSoundCategoryMagicEffect")

    none_guard = re.search(r"If\s+SnapshotToApply\s*!=\s*None", merged)
    push_call = re.search(
        r"SnapshotToApply\.Push\(TransitionTime\)", merged
    )
    assert none_guard and push_call
    assert none_guard.start() < push_call.start()


def test_sound_category_effect_timer_uses_declared_nonzero_id():
    """Regression guard: lesson #12 — StartTimer/CancelTimer must use an explicit,
    non-zero timer id, never the implicit default 0. The skeleton declares
    `Int iTimerID = 1`; the patch must reference that variable, not a literal."""
    patch = _script_patch_source("EN07_NukeSoundCategoryMagicEffect")
    assert patch is not None

    start_timer = re.search(
        r"StartTimer\(iRemovalTimerLength as Float,\s*iTimerID\)", patch
    )
    assert start_timer is not None
    assert "StartTimer(iRemovalTimerLength as Float, 0)" not in patch


def test_sound_category_effect_removal_paths_are_total_and_exactly_once():
    """Regression guard: every path that Push()-es must Remove() exactly once.
    OnEffectFinish always cancels the timer and removes only if the timer path
    has not already completed; OnTimer removes only for its own id and only if
    OnEffectFinish has not already completed. Both remove paths set bCompleted."""
    patch = _script_patch_source("EN07_NukeSoundCategoryMagicEffect")
    assert patch is not None

    events = {
        name: "\n".join(patch.splitlines()[start : end + 1])
        for kind, name, start, end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "event"
    }
    assert {"oneffectstart", "oneffectfinish", "ontimer"} <= set(events)

    finish_body = events["oneffectfinish"]
    cancel_timer = re.search(r"CancelTimer\(iTimerID\)", finish_body)
    finish_guard = re.search(r"If\s+!bCompleted", finish_body)
    finish_none_guard = re.search(r"If\s+SnapshotToApply\s*!=\s*None", finish_body)
    finish_remove = re.search(r"SnapshotToApply\.Remove\(\)", finish_body)
    finish_complete = re.search(r"bCompleted\s*=\s*True", finish_body)
    assert all(
        m is not None
        for m in (
            cancel_timer,
            finish_guard,
            finish_none_guard,
            finish_remove,
            finish_complete,
        )
    )
    assert (
        finish_guard.start()
        < finish_none_guard.start()
        < finish_remove.start()
        < finish_complete.start()
    )

    timer_body = events["ontimer"]
    timer_id_guard = re.search(r"aiTimerID\s*==\s*iTimerID\s*&&\s*!bCompleted", timer_body)
    timer_none_guard = re.search(r"SnapshotToApply\s*!=\s*None", timer_body)
    timer_remove = re.search(r"SnapshotToApply\.Remove\(\)", timer_body)
    timer_complete = re.search(r"bCompleted\s*=\s*True", timer_body)
    assert all(
        m is not None
        for m in (timer_id_guard, timer_none_guard, timer_remove, timer_complete)
    )
    assert (
        timer_id_guard.start()
        < timer_none_guard.start()
        < timer_remove.start()
        < timer_complete.start()
    )


def test_sound_category_effect_does_not_reference_unsupported_deja_channel():
    """Regression guard: DejaChannel is FO76 audio-channel/ducking terminology with
    no FO4 Papyrus analog (data search wiki "Deja" -> []). Per the w1
    DefaultRepairableActorScript precedent it stays declared-but-inert; the patch
    must not invent a call for it."""
    patch = _script_patch_source("EN07_NukeSoundCategoryMagicEffect")
    assert patch is not None
    assert "dejachannel" not in patch.lower()


def test_code_page_ref_reveal_guarded_by_none_check():
    merged = _merged_production_source("Nuke_CodePageRefScript")

    none_guard = re.search(r"If\s+MapMarkerToAdd\s*!=\s*None", merged)
    add_to_map = re.search(r"MapMarkerToAdd\.AddToMap\(False\)", merged)
    assert none_guard and add_to_map
    assert none_guard.start() < add_to_map.start()


def test_code_page_ref_uses_onread_not_onactivate():
    """Regression guard: OnRead is the semantically precise ObjectReference event for
    a BOOK-semantic record (fires on world-read and inventory-equip-to-read alike);
    OnActivate is the right hook for the sibling WorldMapActivatorScript's ACTI
    records but not for this Book, and the two must not be conflated."""
    patch = _script_patch_source("Nuke_CodePageRefScript")
    assert patch is not None
    assert "onread" in patch.lower()
    assert "onactivate" not in patch.lower()


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not PARENT_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPT_ROOT.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")

    source = _merged_production_source(script_name)
    result = compile_psc(
        source,
        imports=[str(base_source), str(PARENT_SOURCE_DIR), str(DEPLOYED_SCRIPT_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
