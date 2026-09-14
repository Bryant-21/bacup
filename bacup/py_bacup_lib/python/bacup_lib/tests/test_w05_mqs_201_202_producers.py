from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


QF_201 = "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3"
QF_202 = "Fragments:Quests:QF_W05_MQS_202P_Acrobat_003F28C7"
REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
PERK_CLOSURE_CONTRACT = (
    "contracts/w05-mqs-201-perk-producer-closure.md"
)

PERK_CASES = {
    "Fragments:Perks:PRKF_W05_MQS_201P_EyebotPart_0040483A": {
        "entry": "fragment_entry_01",
        "helper": "setw05mqs201stage",
        "quest": "W05_MQS_201P_Industrialist",
        "stage": 952,
    },
    "Fragments:Perks:PRKF_W05_MQS_201P_RobobrainP_0040483B": {
        "entry": "fragment_entry_01",
        "helper": "setw05mqs201stage",
        "quest": "W05_MQS_201P_Industrialist",
        "stage": 953,
    },
    "Fragments:Perks:PRKF_W05_MQS_202P_CollectLib_0041B765": {
        "entry": "fragment_entry_00",
        "helper": None,
        "quest": "W05_MQS_202P_Acrobat",
        "stage": 150,
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _perk_skeleton(script_name: str, case: dict[str, object]) -> str:
    quest = case["quest"]
    entry = case["entry"]
    helper = case["helper"]
    stage = case["stage"]
    source = (
        f"Scriptname {script_name} Extends Perk hidden\n\n"
        f"quest Property {quest} Auto mandatory\n"
    )
    if helper is not None:
        source += (
            f"Int Property StagetoIncrement = {stage} Auto\n\n"
            f"Function Fragment_Entry_01(objectreference akTargetRef, actor akActor)\n"
            "    Self.SetW05MQS201Stage(akActor as Actor)\n"
            "EndFunction\n\n"
            "Function SetW05MQS201Stage(Actor akPlayer)\n"
            "EndFunction\n"
        )
    else:
        source += (
            "\nFunction Fragment_Entry_00(objectreference akTargetRef, actor akActor)\n"
            "EndFunction\n"
        )
    assert entry in _member_names(source)
    return source


@pytest.mark.parametrize(("script_name", "case"), PERK_CASES.items())
def test_activate_choice_fragments_merge_once_with_exact_signature_and_compile(
    script_name: str, case: dict[str, object]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    entry = str(case["entry"])
    helper = case["helper"]
    stage = int(case["stage"])
    quest = str(case["quest"])
    merged = _merge_script_method_patches(_perk_skeleton(script_name, case), patch)
    patched_members = [entry] + ([str(helper)] if helper is not None else [])

    for member in patched_members:
        assert _member_names(merged).count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged

    entry_body = _member_body(patch, entry)
    assert entry_body.splitlines()[0] == (
        f"Function Fragment_Entry_{entry[-2:]}(ObjectReference akTargetRef, Actor akActor)"
    )
    assert "akActor == Game.GetPlayer()" in entry_body
    assert patch.count(f"{quest}.SetStage({stage})") == 1
    assert patch.count(f"{quest}.IsStageDone({stage})") == 1

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
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


def test_201_robot_part_perk_rows_are_closed_as_patched() -> None:
    with STATUS_PATH.open(encoding="utf-8", newline="") as stream:
        rows = {row["script_name"].lower(): row for row in csv.DictReader(stream)}

    for script_name in tuple(PERK_CASES)[:2]:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "patched"
        assert row["evidence"] == PERK_CLOSURE_CONTRACT


def test_201_collection_stages_grant_parts_before_progress_and_convergence():
    patch = _script_patch_source(QF_201)
    assert patch is not None

    cases = {
        951: (
            "W05_MQS_201P_MiscItem_VertibotPart",
            "W05_MQS_201P_PlayerFoundVertibotPart",
            (952, 953),
        ),
        952: (
            "W05_MQS_201P_MiscItem_EyebotPart",
            "W05_MQS_201P_PlayerFoundEyebotPart",
            (951, 953),
        ),
        953: (
            "W05_MQS_201P_MiscItem_RobobrainPart",
            "W05_MQS_201P_PlayerFoundRobobrainPart",
            (951, 952),
        ),
    }
    for stage, (item, actor_value, dependencies) in cases.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"GetItemCount({item}) < 1" in body
        assert body.count(f"AddItem({item}, 1, False)") == 1
        assert body.index(f"AddItem({item}, 1, False)") < body.index(
            f"SetValue({actor_value}, 1.0)"
        )
        for dependency in dependencies:
            assert body.index(f"SetValue({actor_value}, 1.0)") < body.index(
                f"IsStageDone({dependency})"
            )
        assert body.index(f"SetValue({actor_value}, 1.0)") < body.index(
            "SetStage(975)"
        )

    bypass = _member_body(patch, "fragment_stage_0902_item_00")
    vertibot = _member_body(patch, "fragment_stage_0951_item_00")
    assert "AddItem(" not in bypass
    assert "!IsStageDone(902)" in vertibot

    cleanup = _member_body(patch, "fragment_stage_1010_item_00")
    for item, _actor_value, _dependencies in cases.values():
        assert cleanup.count(f"RemoveItem({item}, 1, True)") == 1


def test_202_liberator_placement_conversion_and_cleanup_are_ordered():
    patch = _script_patch_source(QF_202)
    assert patch is not None

    placement = _member_body(patch, "fragment_stage_0202_item_00")
    assert placement.index(
        "SetValue(W05_MQS_202P_CanPlaceLiberator, 1.0)"
    ) < placement.index("enableMarker.Enable()")

    conversion = _member_body(patch, "fragment_stage_0210_item_00")
    assert conversion.index(
        "RemoveItem(W05_MQS_202_MiscItem_DeactivatedLiberator"
    ) < conversion.index(
        "AddItem(W05_MQS_202P_MiscItem_RecalibratedLiberator"
    )
    assert conversion.index(
        "AddItem(W05_MQS_202P_MiscItem_RecalibratedLiberator"
    ) < conversion.index(
        "SetValue(W05_MQS_202P_CanPlaceLiberator, 0.0)"
    )
    assert conversion.index(
        "SetValue(W05_MQS_202P_CanPlaceLiberator, 0.0)"
    ) < conversion.index("SetStage(211)")

    cleanup = _member_body(patch, "fragment_stage_0211_item_00")
    assert cleanup.index("corpseRef.Disable()") < cleanup.index("SetStage(225)")
    assert cleanup.index("corpseEnableMarker.Disable()") < cleanup.index(
        "SetStage(225)"
    )


def test_201_and_202_terminal_handoffs_remain_story_manager_driven():
    cases = (
        (QF_201, "W05_MQS_202P_QuestStartKeyword.SendStoryEvent"),
        (QF_202, "W05_MQS_203P_QuestStartKeyword.SendStoryEvent"),
    )
    for script_name, keyword_call in cases:
        patch = _script_patch_source(script_name)
        assert patch is not None
        handoff = _member_body(patch, "fragment_stage_9000_item_00")
        assert keyword_call in handoff
        assert ".Start()" not in handoff


def test_202_stage_730_uses_the_source_remove_essential_contract():
    patch = _script_patch_source(QF_202)
    assert patch is not None
    body = _member_body(patch, "fragment_stage_0730_item_00")
    assert "Alias_TL_Instance_Actor_JenEnableMarker" not in patch
    assert "Alias_TL_Instance_Actor_Spy.GetActorReference()" in body
    assert "spyRef.GetActorBase().SetEssential(False)" in body


def test_201_and_202_live_fragment_surfaces_are_complete():
    cases = {
        QF_201: (59, (1, 125, 311, 430, 431, 432, 9999, 10000)),
        QF_202: (40, (51, 251, 730, 746, 749, 851, 9999, 10000)),
    }
    for script_name, (expected_count, restored_stages) in cases.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        members = set(_member_names(patch))
        assert len(members) == expected_count
        for stage in restored_stages:
            assert f"fragment_stage_{stage:04d}_item_00" in members

    assert _script_patch_source("W05_QT_TriggerScript") is not None
