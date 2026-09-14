"""Regression cover for the W05 Settlers early-chain repairs that unblock play.

203P: the three brain-jar activators and their jar meshes are enable-parented to
the ``EnableMarker_BrainStorage_*`` refs, which ship initially disabled with no
self-enabling VMAD. Until stage 700 enables them, stages 710/720/730 are
unreachable and the quest stalls on objective 70 forever.

201P: objective 400 ("Get an access keycard for the safe room gate") was
displayed at stage 400 but never completed, so it stayed in the journal past
quest completion. Stage 450 is where the keycard is actually granted.
"""

from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

QF_201P = "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3"
QF_203P = "Fragments:Quests:QF_W05_MQS_203P_0040571C"

# Bound on W05_MQS_203P_QuestScript's VMAD as Stage_EnteredBrainRoom /
# Stage_TookDiasBrain / Stage_TookGregBrain / Stage_TookGinaBrain.
STAGE_ENTERED_BRAIN_ROOM = 700
STAGE_TOOK_BRAIN = (710, 720, 730)

BRAIN_STORAGE_ALIASES = (
    "Alias_EnableMarker_BrainStorage_Dias",
    "Alias_EnableMarker_BrainStorage_Greg",
    "Alias_EnableMarker_BrainStorage_Gina",
)


def _fragment_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None, f"no durable patch for {script_name}"
    return patch


def test_203p_stage_700_enables_every_brain_storage_marker():
    body = _member_body(_patch(QF_203P), _fragment_member(STAGE_ENTERED_BRAIN_ROOM))
    for alias in BRAIN_STORAGE_ALIASES:
        assert f"{alias}.GetReference()" in body, alias
    # One guarded Enable() per marker, plus none left unguarded.
    assert body.count(".Enable()") == len(BRAIN_STORAGE_ALIASES)
    assert body.count("If storageMarker != None") == len(BRAIN_STORAGE_ALIASES)
    # The pre-existing scene start must survive the repair.
    assert "W05_MQS_203P_009_EnterBrainRoom.Start()" in body


def test_203p_brain_storage_markers_are_enabled_before_the_take_stages_run():
    patch = _patch(QF_203P)
    members = _member_names(patch)
    assert _fragment_member(STAGE_ENTERED_BRAIN_ROOM) in members
    entered_at = members.index(_fragment_member(STAGE_ENTERED_BRAIN_ROOM))
    for stage in STAGE_TOOK_BRAIN:
        member = _fragment_member(stage)
        assert member in members, stage
        # Each Took* stage still advances to the shared 800 hand-off.
        assert "SetStage(800)" in _member_body(patch, member)
    assert all(
        entered_at < members.index(_fragment_member(stage))
        for stage in STAGE_TOOK_BRAIN
    )


def test_203p_enable_markers_are_not_disabled_anywhere_in_the_patch():
    patch = _patch(QF_203P)
    for alias in BRAIN_STORAGE_ALIASES:
        assert f"{alias}" in patch
    # Nothing may re-disable the storage markers; the jars must stay reachable
    # across save/load and the 10000 cleanup sweep.
    for member in ("fragment_stage_9000_item_00", "fragment_stage_10000_item_00"):
        body = _member_body(patch, member)
        for alias in BRAIN_STORAGE_ALIASES:
            assert alias not in body


def test_201p_stage_450_completes_the_keycard_objective():
    body = _member_body(_patch(QF_201P), _fragment_member(450))
    assert "SetObjectiveCompleted(400)" in body
    assert "SetObjectiveCompleted(449)" in body
    assert "SetObjectiveDisplayed(450)" in body
    # Objective 400 completes where the keycard is actually granted.
    assert "W05_MQS_201P_HornwrightSaferoomKeycard" in body


def test_201p_keycard_objective_is_completed_exactly_once():
    patch = _patch(QF_201P)
    assert patch.count("SetObjectiveCompleted(400)") == 1


@pytest.mark.parametrize("script_name", (QF_201P, QF_203P))
def test_repairs_merge_into_production_without_duplicating_members(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []

    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    counts = Counter(_member_names(merged))
    assert counts, script_name
    assert max(counts.values()) == 1, [n for n, c in counts.items() if c > 1]
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", (QF_201P, QF_203P))
def test_repaired_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    merged = _merge_script_method_patches(
        _production_skeleton(script_name), _patch(script_name)
    )
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
