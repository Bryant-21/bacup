from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wastelanders-keypad-package-source-closure-2026-09-03.md"
)

def _code_lines(patch: str) -> str:
    """Patch text with comment lines dropped, so prose cannot fail a guard."""
    return "\n".join(
        line for line in patch.splitlines() if not line.lstrip().startswith(";")
    )


# DefaultAliasSetStageOnKeypadSuccess left this list when SH-01 delivered the
# adapter the contract demanded; the two W05 fallbacks stay retired.
UNPROVEN_MEMBER_PATCHES = (
    "W05_Vaut79EntranceKeypadScript",
    "W05_MQR_Vault79KeypadAliasScript",
    "Fragments:Packages:PF_W05_Derek_Vault79Operatio_0057507F",
    "Fragments:Packages:PF_W05_TravelToNukeLinkRef_0053820F",
)

TARGET_STAGE_FRAGMENTS = {
    "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976": (
        "Function Fragment_Stage_0765_Item_00()",
        "SetStage(1000)",
    ),
    "Fragments:Quests:QF_W05_MQ_000P_005698E4": (
        "Function Fragment_Stage_2100_Item_00()",
        "W05_PlayerKnows_BeenToVault79",
    ),
    "Fragments:Quests:QF_AC_SQ05_Regent_006FCF87": (
        "Function Fragment_Stage_0250_Item_00()",
        "BookcaseDoor.GetReference()",
    ),
}


@pytest.mark.parametrize("script_name", UNPROVEN_MEMBER_PATCHES)
def test_unproven_keypad_and_package_members_stay_out_of_registry(
    script_name: str,
) -> None:
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize(
    ("script_name", "member", "effect"),
    [
        (script_name, expected[0], expected[1])
        for script_name, expected in TARGET_STAGE_FRAGMENTS.items()
    ],
)
def test_keypad_target_stage_consumers_remain_repaired(
    script_name: str, member: str, effect: str
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert member in patch
    assert effect in patch


def test_keypad_adapter_supersedes_the_record_dependency_blocker() -> None:
    """The blocker this contract raised is answered, not deleted.

    The adapter must still honour the prohibitions the contract named:
    activation is not a result, and cancel/failure set no stage.
    """
    patch = _script_patch_source("DefaultAliasSetStageOnKeypadSuccess")

    assert patch is not None
    code = _code_lines(patch)
    assert "OnActivate" not in code
    assert "DefaultKeypadScript.KeypadSuccess" in code
    assert "If preReqStage > 0 && !owningQuest.IsStageDone(preReqStage)" in code
    assert "If !ActivatorIsAllowed(Game.GetPlayer())" in code


def test_contract_pins_exact_keypad_alias_and_reference_topology() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for token in (
        "`41C976`",
        "alias 18 `CageKeypad`",
        "`42486A`",
        "`43048F`",
        "preset `71990`",
        "stage `765`, prerequisite `700`",
        "`5698E4`",
        "alias 34 `Vault79Keypad`",
        "LCTN `58D8C3`",
        "`597A63`",
        "REFR `56BA45`",
        "stage `2100`",
        "alias 0 `Player`",
        "`6FCF87`",
        "alias 33 `Keypad`",
        "`726DDC`",
        "REFR `726DE9`",
        "preset `68492`",
        "stage `250`",
        "`PlayerActivateType = 2`",
    ):
        assert token in contract


def test_contract_rejects_ambiguous_keypad_and_nuke_end_paths() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "`OnActivate` is emitted before any code can be verified" in contract
    assert "`OnMenuClose` is shared by successful,\ncancelled, and failed" in contract
    assert "cancellation and\nan incorrect submission must leave stages" in contract
    assert "emit distinct success, cancel, and failure results" in contract
    assert "keep cancellation/failure side-effect free" in contract
    assert "any arbitrary\nsix-digit actor-value value" in contract
    assert "activation, cancel, wrong code, and correct\ncode alike" in contract
    assert "Clearing `53820D` on arrival is specifically prohibited" in contract


def test_contract_pins_derek_and_nuke_package_ownership_and_order() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for token in (
        "`57507F` `W05_Derek_Vault79Operations`",
        "alias 53 `ElevatorMarker`",
        "no\nrecord references `57507F`",
        "Alias 2 `DerekGarrison`",
        "`5674A3`",
        "`553AD8`",
        "`553AED`",
        "alias 62\n`DerekGarrisonOperations` uses `5A2930`",
        "`53820F` `W05_TravelToNukeLinkRef`",
        "`GetActorValue(53820D) >= 1.0`",
        "`53820E` `W05_NPCNukeFleeTargetKeyword`",
        "`53820B` `TEST_W05_Raider_NukeBlast`",
        "`536B52` `W05_Raider_SandboxFoodF01`",
        "`53820F`, then `3FE949`",
    ):
        assert token in contract


A1_ACCEPTANCE = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wastelanders-a1-acceptance-keypad-rescue-perk-2026-09-09.md"
)


def test_a1_acceptance_contract_carries_the_nine_negative_cases_and_exact_values() -> None:
    """A3 supplies acceptance only; A1 implements SH-01 and the rescue perk."""
    contract = A1_ACCEPTANCE.read_text(encoding="utf-8")

    for token in (
        "`StageToSet = 765`, `preReqStage = 700`",
        "`StageToSet = 2100`, `ActivatedByAliases = [alias 0 of 5698E4]`",
        "**`preReqStage`** (lower-case `p`)",
        "**Crane has no `ActivatedByAliases`; Vault 79 has no `preReqStage`.**",
        "Crane's alias 0 is `Duchess`",
        "preset `071990`",
        "A3 will not restore the retired activation fallbacks",
    ):
        assert token in contract, token

    negatives = contract.split("### The nine negative cases", 1)[1].split(
        "**Positive case:**", 1
    )[0]
    for index in range(1, 10):
        assert f"{index}. " in negatives
    assert "Positive case:" in contract


def test_a1_acceptance_contract_pins_the_rescue_perk_two_defects_and_effect_order() -> None:
    contract = A1_ACCEPTANCE.read_text(encoding="utf-8")

    for token in (
        "**two independent defects**",
        "`VirtualMachineAdapter` field at all**",
        "unexpected token Dot in statement",
        "753 bytes",
        "`Fragment_Entry_00`",
        "SetValue",
        "EvaluatePackage",
        "Activate",
        "`DefaultAliasOnActivate` -> `StageToSet = 200`",
    ):
        assert token in contract, token



def test_a1_acceptance_contract_answers_the_vault_79_code_source_and_binding_question() -> None:
    """Addendum 2: the two items A1 said A3 still owed on Vault 79.

    Both are record facts rather than design choices, so they are pinned to the
    exact values a future read has to reproduce.
    """
    contract = A1_ACCEPTANCE.read_text(encoding="utf-8")

    for token in (
        # the code source and the digit count that follows from it
        "`5698F3 W05_MQ00_CodeAV`",
        "Utility.RandomInt(100000, 999999)",
        "Fragment_Stage_1999_Item_00",
        "**6**, forced by that range",
        # which binding is authoritative: all four, on one shared reference
        "**All of them, and they are not alternatives.**",
        "`LocationRefType 597A63 W05_MQR_Vault79Keypad_LocRef`",
        "No alias uses a forced reference",
        "`5698E4 W05_MQ_000P` (Treasure Unknown)",
        "`535E55 W05_MQR_204P`",
        "`548B7A W05_MQR_205P`",
        "`54EDB9 W05_MQA_206P`",
        # the defect the shared reference creates in the landed adapter
        "`bCompletionDelegated` needs to be a count",
        # and the conflicting write A3 found while proving the code source
        "`pW05_MQ00_CodeAV = 1.0` write",
    ):
        assert token in contract, token
