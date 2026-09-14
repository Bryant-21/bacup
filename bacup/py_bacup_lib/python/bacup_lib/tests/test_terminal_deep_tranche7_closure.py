from __future__ import annotations

from pathlib import Path
import re

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
LEDGER = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "contracts" / "terminal-fragment-remainder-ledger-2026-08.md"
MTR04_SCRIPT = "Fragments:Terminals:TERM_MTR04_RedeemSubTerminal_00324E11"
MTR04_MEMBERS = {
    "redeemprize",
    "fragment_terminal_01",
    "fragment_terminal_03",
    "fragment_terminal_04",
    "fragment_terminal_05",
    "fragment_terminal_06",
    "fragment_terminal_08",
    "fragment_terminal_09",
    "fragment_terminal_10",
    "fragment_terminal_11",
    "fragment_terminal_12",
}
MTR08_SCRIPT = "Fragments:Terminals:TERM_MTR08_ClaimTokenExchang_0004C6E5"
MTR08_MEMBERS = {
    "redeemclaimtokens",
    "fragment_terminal_01",
    "fragment_terminal_02",
    "fragment_terminal_03",
    "fragment_terminal_04",
}
REDEMPTIONS = {
    "01": ("PencilTop", 1, 5),
    "03": ("MiningHelmet", 1, 20),
    "04": ("CommieWhacker", 1, 50),
    "05": ("MascotSuit", 1, 150),
    "06": ("MascotHead", 1, 300),
    "08": ("CottonCandy", 1, 5),
    "09": ("Gumdrops", 1, 5),
    "10": ("PaddleBallAmmo", 10, 5),
    "11": ("PaddleBall", 1, 50),
    "12": ("Comic", 1, 100),
}
ARCADE_REDEMPTIONS = {
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC5": {
        "01": ("Form_PipePistol", 1, 200),
        "03": ("Form_38Ammo", 16, 100),
        "04": ("Form_PaddleBallString", 20, 100),
        "05": ("Form_BoiledWater", 1, 60),
        "06": ("Form_EmptyNukaColaBottle", 1, 20),
    },
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC6": {
        "01": ("Form_10mmAmmo", 28, 200),
        "03": ("Form_GoldfishPlan", 1, 1600),
        "04": ("Form_NWOTShirt", 1, 1600),
        "05": ("Form_CommieWhacker", 1, 800),
        "06": ("Form_NukaCherry", 1, 600),
        "07": ("Form_Snowglobe", 1, 1600),
        "08": ("Form_Snowglobe", 1, 1600),
        "09": ("Form_CommieWhacker", 1, 800),
        "10": ("Form_BaseballGrenade", 1, 200),
        "11": ("Form_GoldfishPlan", 1, 1600),
    },
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC7": {
        "01": ("Form_ThirstZapper", 1, 6000),
        "03": ("Form_NWOTJumpsuit", 1, 2500),
        "04": ("Form_NukacadeTokenDispenserPlan", 1, 800),
        "05": ("Form_NukaColaWild", 1, 800),
        "06": ("Form_Missile", 1, 800),
        "07": ("Form_WeaponizedNukaColaAmmo", 6, 1600),
        "08": ("Form_Stimpak", 1, 400),
        "09": ("Form_Psycho", 1, 400),
        "10": ("Form_45Ammo", 28, 600),
        "11": ("Form_Arrows", 16, 600),
        "12": ("Form_NukacadeTokenDispenserPlan", 1, 800),
    },
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC8": {
        "00": ("Form_NukaColaQuantum", 1, 2000),
        "02": ("Form_NukaWorldSpeakersPlan", 1, 12000),
        "03": ("Form_NukelelePlans", 1, 12000),
        "04": ("Form_WeaponizedQuantum", 6, 12000),
        "05": ("Form_WeaponizedCherry", 6, 6000),
        "06": ("Form_NukaQuantumGrenades", 1, 8000),
        "07": ("Form_NukaGrenades", 1, 5000),
        "08": ("Form_MiniNuke", 1, 2000),
        "09": ("Form_SuperStim", 1, 2000),
        "10": ("Form_NukaWorldSpeakersPlan", 1, 12000),
        "11": ("Form_NukelelePlans", 1, 12000),
    },
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal__0065CEC9_1": {
        "01": ("Form_BanditRoundupPlan", 1, 20000),
        "02": ("Form_BanditRoundupPlan", 1, 20000),
        "04": ("Form_NukaZapperRacePlan", 1, 20000),
        "05": ("Form_WhackACommiePlan", 1, 20000),
        "06": ("Form_WeaponisedNukaColaPlan", 1, 20000),
        "07": ("Form_WeaponisedNukaColaPlan", 1, 20000),
        "08": ("Form_WhackACommiePlan", 1, 20000),
        "09": ("Form_NukaZapperRacePlan", 1, 20000),
        "10": ("Form_BottleBlasterPlan", 1, 20000),
        "11": ("Form_BottleBlasterPlan", 1, 20000),
    },
}
EN06_SCRIPT = "Fragments:Terminals:TERM_EN06_PresidentialVendor_0052BDE0"


def test_terminal_deep_tranche7_exact_members_merge_and_compile():
    patch = _script_patch_source(MTR04_SCRIPT)
    assert patch is not None
    patch_members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert patch_members == MTR04_MEMBERS

    relative_path = _script_relative_path(MTR04_SCRIPT, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    merged_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in MTR04_MEMBERS:
        assert merged_members.count(member) == 1

    base_source = _fo4_base_source()
    assert base_source is not None
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_terminal_deep_tranche7_redemption_contract_is_exact_and_guarded():
    patch = _script_patch_source(MTR04_SCRIPT)
    assert patch is not None
    assert "playerRef.GetItemCount(Token) >= aiTokenCost" in patch
    assert patch.index("playerRef.RemoveItem(Token, aiTokenCost, True)") < patch.index(
        "playerRef.AddItem(akPrize, aiQuantity, False)"
    )
    for fragment, (reward, quantity, cost) in REDEMPTIONS.items():
        member = re.search(
            rf"Function Fragment_Terminal_{fragment}\([^)]*\)(.*?)EndFunction",
            patch,
            re.DOTALL,
        )
        assert member is not None
        assert f"RedeemPrize({reward}, {quantity}, {cost})" in member.group(1)

    assert "Fragment_Terminal_02" not in patch
    assert "Utility.Random" not in patch
    assert "Game.GetForm" not in patch


def test_terminal_deep_tranche7_manifest_and_remainder_reconcile():
    skeleton = (
        SOURCE_ROOT / _script_relative_path(MTR04_SCRIPT, ".psc")
    ).read_text(encoding="utf-8")
    expected_properties = {
        "Token",
        *(reward for reward, _quantity, _cost in REDEMPTIONS.values()),
    }
    for property_name in expected_properties:
        assert f" Property {property_name} Auto" in skeleton

    ledger = LEDGER.read_text(encoding="utf-8")
    assert "`TERM_MTR04_RedeemSubTerminal_00324E11.psc`" not in ledger
    assert "`TERM_MTR08_ClaimTokenExchang_0004C6E5.psc`" not in ledger
    rows = re.findall(r"^\| `[^`]+` \| ([a-z0-9-]+) \|$", ledger, re.MULTILINE)
    assert len(rows) == 261
    assert {disposition: rows.count(disposition) for disposition in set(rows)} == {
        "bound-property-contract-blocked": 34,
        "display-service-blocked": 11,
        "live-binding-type-mismatch-blocked": 1,
        "live-topology-or-orphan-trace-blocked": 34,
        "raid-or-newer-controller-blocked": 52,
        "service-transaction-blocked": 41,
        "source-identity-blocked": 71,
        "test-content-nondefect": 17,
    }


def test_terminal_deep_tranche7_mtr08_exact_members_merge_and_compile():
    patch = _script_patch_source(MTR08_SCRIPT)
    assert patch is not None
    assert {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    } == MTR08_MEMBERS

    relative_path = _script_relative_path(MTR08_SCRIPT, ".psc")
    current = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    assert "Property MTR08_ClaimToken" not in current
    augmented = _augment_fo76_to_fo4_script_skeleton(MTR08_SCRIPT, current)
    assert "MiscObject Property MTR08_ClaimToken Auto Mandatory" in augmented
    merged = _merge_script_method_patches(augmented, patch)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_terminal_deep_tranche7_mtr08_reward_mapping_and_order_are_exact():
    patch = _script_patch_source(MTR08_SCRIPT)
    assert patch is not None
    expected = {
        "01": ("MTR08_LL_01_MineHaulStandard", 10),
        "02": ("MTR08_LL_03_MineHaulGreat", 40),
        "03": ("MTR08_LL_02_MineHaulGood", 20),
        "04": ("MTR08_LL_04_MineHaulJackpot", 100),
    }
    for fragment, (reward, cost) in expected.items():
        member = re.search(
            rf"Function Fragment_Terminal_{fragment}\([^)]*\)(.*?)EndFunction",
            patch,
            re.DOTALL,
        )
        assert member is not None
        assert f"RedeemClaimTokens({reward}, {cost})" in member.group(1)
    assert "playerRef.GetItemCount(MTR08_ClaimToken) >= aiTokenCost" in patch
    assert patch.index(
        "playerRef.RemoveItem(MTR08_ClaimToken, aiTokenCost, True)"
    ) < patch.index("playerRef.AddItem(akReward, 1, False)")


@pytest.mark.parametrize(
    ("script_name", "redemptions"), ARCADE_REDEMPTIONS.items()
)
def test_terminal_deep_tranche7_arcade_exact_members_semantics_and_compile(
    script_name: str, redemptions: dict[str, tuple[str, int, int]]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    expected_members = {"redeemarcadeprize"} | {
        f"fragment_terminal_{fragment}" for fragment in redemptions
    }
    assert {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    } == expected_members
    for fragment, (reward, quantity, cost) in redemptions.items():
        member = re.search(
            rf"Function Fragment_Terminal_{fragment}\([^)]*\)(.*?)EndFunction",
            patch,
            re.DOTALL,
        )
        assert member is not None
        assert f"RedeemArcadePrize({reward}, {quantity}, {cost})" in member.group(1)
    assert "playerRef.GetValue(PointsAV) >= aiPointCost" in patch
    assert patch.index("playerRef.ModValue(PointsAV, -aiPointCost)") < patch.index(
        "playerRef.AddItem(akPrize, aiQuantity, False)"
    )

    relative_path = _script_relative_path(script_name, ".psc")
    current = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, current)
    assert "ActorValue Property PointsAV Auto Mandatory" in augmented
    merged = _merge_script_method_patches(augmented, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    base_source = _fo4_base_source()
    assert base_source is not None
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics


def test_terminal_deep_tranche7_en06_exact_members_semantics_and_compile():
    patch = _script_patch_source(EN06_SCRIPT)
    assert patch is not None
    assert {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    } == {
        "redeempresidentialseal",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
    }
    for fragment, reward in {
        "01": "PAC_PowerArmor_T60_Presidential_Full",
        "02": "GaussRifle_Presidential",
        "03": "Clothes_SuitClean_Blue_Presidential",
    }.items():
        member = re.search(
            rf"Function Fragment_Terminal_{fragment}\([^)]*\)(.*?)EndFunction",
            patch,
            re.DOTALL,
        )
        assert member is not None
        assert f"RedeemPresidentialSeal({reward})" in member.group(1)
    assert patch.index(
        "playerRef.RemoveItem(EN06_PresidentialSeal, 1, True)"
    ) < patch.index("playerRef.AddItem(akReward, 1, False)")

    relative_path = _script_relative_path(EN06_SCRIPT, ".psc")
    current = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(EN06_SCRIPT, current)
    assert "MiscObject Property EN06_PresidentialSeal Auto Mandatory" in augmented
    merged = _merge_script_method_patches(augmented, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    base_source = _fo4_base_source()
    assert base_source is not None
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics


def test_terminal_deep_tranche7_all_eight_live_pscs_leave_remainder():
    ledger = LEDGER.read_text(encoding="utf-8")
    scripts = {MTR04_SCRIPT, MTR08_SCRIPT, EN06_SCRIPT, *ARCADE_REDEMPTIONS}
    assert len(scripts) == 8
    for script_name in scripts:
        assert f"`{script_name.rsplit(':', 1)[-1]}.psc`" not in ledger
    assert sum(len(items) for items in ARCADE_REDEMPTIONS.values()) == 47
