from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
RS01A_SCRIPT = "Fragments:Quests:QF_RS01A_Contact_003C4C22"
W05_PENNINGTON_SCRIPT = (
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_Penni_005851DD"
)
DEPLOYED_QUEST_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "Fragments"
    / "Quests"
)
RAW_QUEST_ROOT = (
    REPO_ROOT / "extracted" / "fo76" / "scripts" / "client" / "fragments" / "quests"
)
TODO_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CONTRACT_RELATIVE_PATH = "contracts/pennington-quest-fragments.md"
CONTRACT_PATH = STATUS_PATH.parent / CONTRACT_RELATIVE_PATH
RS01A_MEMBER_NAMES = (
    "fragment_stage_0000_item_00",
    "fragment_stage_0010_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0160_item_00",
    "fragment_stage_0165_item_00",
    "fragment_stage_0170_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_1100_item_00",
    "fragment_stage_9000_item_00",
)
DEFERRED_RS01A_MEMBERS: set[str] = set()
IMPLEMENTED_RS01A_MEMBERS = set(RS01A_MEMBER_NAMES) - DEFERRED_RS01A_MEMBERS
RS01A_SKELETON = """Scriptname Fragments:Quests:QF_RS01A_Contact_003C4C22 Extends Quest hidden

actorvalue Property MQ_OverseerHolotape01Played Auto mandatory
referencealias Property Alias_Player Auto mandatory
keyword Property pW05_MQ_00P_StartKeyword Auto mandatory
keyword Property Tutorial_PlaceCAMPStartKeyword Auto mandatory
actorvalue Property RSVP00_AV_StartedRSVP01 Auto
keyword Property Tutorial_WeaponCraftingStartKeyword Auto mandatory
constructibleobject Property Pco_CondProxy_Photomode_Frame_Faction_Responders01 Auto mandatory
quest Property RSVP01_Quest Auto
actorvalue Property RS01A_Contact_Started Auto mandatory
actorvalue Property RS01A_Contact_Completed Auto mandatory
holotape Property MQ_Overseer_01_Vault76Holotape Auto mandatory
actorvalue Property MQ_OverseerHolotape01PickedUp Auto mandatory
keyword Property Tutorial_ArmorCraftingStartKeyword Auto mandatory

Function Fragment_Stage_0000_Item_00()
EndFunction

Function Fragment_Stage_0160_Item_00()
EndFunction

Function Fragment_Stage_0500_Item_00()
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction

Function Fragment_Stage_0400_Item_00()
EndFunction

Function Fragment_Stage_1100_Item_00()
EndFunction

Function Fragment_Stage_0170_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
EndFunction

Function Fragment_Stage_0200_Item_00()
EndFunction

Function Fragment_Stage_0165_Item_00()
EndFunction

Function Fragment_Stage_0150_Item_00()
EndFunction
"""


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source() -> str:
    pex_path = DEPLOYED_QUEST_ROOT / "QF_RS01A_Contact_003C4C22.pex"
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    patch = _script_patch_source(RS01A_SCRIPT)
    assert patch is not None
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    return _merge_script_method_patches(skeleton, patch)


def test_rs01a_patch_is_member_only_and_supplies_closed_vmad_members():
    patch = _script_patch_source(RS01A_SCRIPT)
    assert patch is not None

    lines = patch.splitlines()
    member_names = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(lines)
        if kind == "function"
    }

    assert member_names == IMPLEMENTED_RS01A_MEMBERS
    assert patch.count("; TODO") == 0
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in lines
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in lines)


def test_rs01a_synthetic_merge_preserves_deferred_hollow_members_and_is_idempotent():
    patch = _script_patch_source(RS01A_SCRIPT)
    assert patch is not None
    merged = _merge_script_method_patches(RS01A_SKELETON, patch)

    merged_names = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert set(merged_names) == set(RS01A_MEMBER_NAMES)
    assert all(merged_names.count(name) == 1 for name in RS01A_MEMBER_NAMES)
    for member_name in IMPLEMENTED_RS01A_MEMBERS:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for member_name in DEFERRED_RS01A_MEMBERS:
        assert _member_body(merged, member_name).count("\n") == 1
    assert "actorvalue Property RS01A_Contact_Started Auto mandatory" in merged
    assert _merge_script_method_patches(merged, patch) == merged


def test_rs01a_closed_stage_contract_is_ordered_and_bounded():
    patch = _script_patch_source(RS01A_SCRIPT)
    assert patch is not None

    stage_0 = _member_body(patch, "fragment_stage_0000_item_00")
    assert stage_0.index("!IsStageDone(10)") < stage_0.index("SetStage(10)")
    assert stage_0.count("SetStage(") == 1

    stage_150 = _member_body(patch, "fragment_stage_0150_item_00")
    assert stage_150.count("AddItem(") == 1
    assert stage_150.index("GetItemCount(") < stage_150.index("AddItem(")
    assert "SetValue(MQ_OverseerHolotape01PickedUp, 1.0)" in stage_150

    objective_pairs = {
        "fragment_stage_0160_item_00": (100, 110),
        "fragment_stage_0170_item_00": (110, 120),
        "fragment_stage_0200_item_00": (120, 200),
        "fragment_stage_0400_item_00": (200, 400),
        "fragment_stage_0500_item_00": (400, 500),
    }
    for member_name, (completed, displayed) in objective_pairs.items():
        body = _member_body(patch, member_name)
        complete_call = f"SetObjectiveCompleted({completed})"
        display_call = f"SetObjectiveDisplayed({displayed})"
        assert body.index(complete_call) < body.index(display_call)

    stage_1100 = _member_body(patch, "fragment_stage_1100_item_00")
    assert "SetValue(RS01A_Contact_Started, 1.0)" in stage_1100
    assert stage_1100.count("SetObjectiveDisplayed(120)") == 1
    assert stage_1100.index("!IsObjectiveCompleted(120)") < stage_1100.index(
        "SetObjectiveDisplayed(120)"
    )
    assert "SetStage(" not in stage_1100

    stage_165 = _member_body(patch, "fragment_stage_0165_item_00")
    assert stage_165.count(".SendStoryEvent(None, playerRef, playerRef)") == 3

    stage_1000 = _member_body(patch, "fragment_stage_1000_item_00")
    story_keyword = (
        'Game.GetFormFromFile(0x003B32EC, "SeventySix.esm") as Keyword'
    )
    story_send = (
        "questActiveKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    convergence_guard = (
        "RSVP01_Quest.IsRunning() || RSVP01_Quest.IsCompleted()"
    )
    assert "RSVP01_Quest.Start()" not in stage_1000
    assert stage_1000.index("SetObjectiveCompleted(500)") < stage_1000.index(
        story_keyword
    )
    assert stage_1000.index(story_keyword) < stage_1000.index(story_send)
    assert stage_1000.index(story_send) < stage_1000.index(convergence_guard)
    assert stage_1000.index(convergence_guard) < stage_1000.index(
        "SetStage(9000)"
    )
    assert "!RSVP01_Quest.IsRunning()" in stage_1000
    assert "!RSVP01_Quest.IsCompleted()" in stage_1000
    assert "&& !IsStageDone(9000)" in stage_1000

    stage_9000 = _member_body(patch, "fragment_stage_9000_item_00")
    assert "SetValue(RS01A_Contact_Completed, 1.0)" in stage_9000
    story_send = (
        "accepted = pW05_MQ_00P_StartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    )
    initial_success = (
        "Bool accepted = followOn && "
        "(followOn.IsRunning() || followOn.IsCompleted())"
    )
    final_success = (
        "accepted = followOn.IsRunning() || followOn.IsCompleted()"
    )
    assert story_send in stage_9000
    assert "pW05_MQ_00P_StartKeyword.SendStoryEvent(" not in stage_9000
    assert "followOn.Start()" not in stage_9000
    assert "Pco_CondProxy_Photomode_Frame_Faction_Responders01" not in stage_9000
    assert '0x005698E4, "SeventySix.esm"' in stage_9000
    assert stage_9000.index(initial_success) < stage_9000.index(story_send)
    assert stage_9000.index(story_send) < stage_9000.index(final_success)
    assert stage_9000.count("If !accepted") == 2


def test_rs01a_production_merge_is_unique_idempotent_and_native_compiles_for_fo4():
    patch = _script_patch_source(RS01A_SCRIPT)
    assert patch is not None
    merged = _merged_production_source()

    merged_names = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert set(merged_names) == IMPLEMENTED_RS01A_MEMBERS
    assert all(merged_names.count(name) == 1 for name in IMPLEMENTED_RS01A_MEMBERS)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Fragments/Quests/QF_RS01A_Contact_003C4C22.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_w05_pennington_is_an_unbound_nondefect_without_a_patch():
    assert _script_patch_source(W05_PENNINGTON_SCRIPT) is None

    pex_paths = (
        RAW_QUEST_ROOT / "qf_w05_mq_001p_wayward_penni_005851dd.pex",
        DEPLOYED_QUEST_ROOT / "QF_W05_MQ_001P_Wayward_Penni_005851DD.pex",
    )
    for pex_path in pex_paths:
        assert pex_path.is_file(), f"Pennington PEX unavailable: {pex_path}"
        source = decompile_pex(pex_path, fo4_api_compat=True)
        assert _iter_top_level_papyrus_members(source.splitlines()) == []

    with STATUS_PATH.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))
    row = next(item for item in rows if item["script_name"] == W05_PENNINGTON_SCRIPT)
    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == "contracts/w3c-mq.md"


def test_rs01a_todo_marker_and_status_are_synchronized():
    patch = _script_patch_source(RS01A_SCRIPT)
    assert patch is not None
    todo_lines = [
        line
        for line in TODO_PATH.read_text(encoding="utf-8").splitlines()
        if line.startswith("RS01A-TODO|")
    ]

    assert len(todo_lines) == 1
    assert any(
        "category=EB-CHECKPOINT|members=1100:00|" in line
        for line in todo_lines
    )
    assert all("category=ONLINE-RECIPE" not in line for line in todo_lines)
    assert all("status=deferred" in line for line in todo_lines)
    assert all(
        f"contract={CONTRACT_RELATIVE_PATH}|evidence={CONTRACT_RELATIVE_PATH}|"
        in line
        for line in todo_lines
    )
    assert patch.count("; TODO") == 0
    contract = CONTRACT_PATH.read_text(encoding="utf-8")
    assert all(member_name in contract for member_name in (
        "Fragment_Stage_0000_Item_00",
        "Fragment_Stage_1100_Item_00",
        "Fragment_Stage_9000_Item_00",
    ))

    with STATUS_PATH.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))
    row = next(item for item in rows if item["script_name"] == RS01A_SCRIPT)
    assert row["terminal_state"] == "patched"
    assert row["evidence"] == CONTRACT_RELATIVE_PATH
    assert "user regeneration and runtime verification pending" in row["notes"]
