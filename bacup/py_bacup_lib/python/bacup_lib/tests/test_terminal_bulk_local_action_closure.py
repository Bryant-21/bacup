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

SCRIPT_MEMBERS = {
    "Fragments:Terminals:TERM_MTR06_SystemTerminal_Le_003AAB8B": {
        "fragment_terminal_02",
        "fragment_terminal_03",
    },
    "Fragments:Terminals:TERM_RSVP00_Terminal_Sub_Hub_003E1FC0": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_EN06_RegistrationTermin_002B47C2": {
        "fragment_terminal_06",
        "fragment_terminal_07",
    },
    "Fragments:Terminals:TERM_V96_Access_TerminalVaul_00324189": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_LC006_FacilityAccessCon_0013DB7B": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_LC006_ReactorAccessCont_00240B54": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_LC006_ReactorAccessCont_003C2E3D": {
        "fragment_terminal_01",
        "fragment_terminal_02",
    },
    "Fragments:Terminals:TERM_WL006_AssaultronRoomTer_0058E9BD": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_WL006_DoorControlTermin_0058FF21": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_WL006_DoorControlTermin_0058FF2C": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_WL006_SecurityDoorContr_0058E9BF": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_RSz00_SelfServeTerminal_003B3308": {
        "fragment_terminal_03"
    },
    "Fragments:Terminals:TERM_P01A_Nukashine_JudysTer_0046D780": {
        "fragment_terminal_03"
    },
    "Fragments:Terminals:TERM_RSVP01_Terminal_Kiosk_S_003B56EC": {
        "fragment_terminal_04",
        "fragment_terminal_06",
    },
    "Fragments:Terminals:TERM_PowerPlant_TerminalMono_003EB68F": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_PowerPlant_TerminalPose_003EB685": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_PowerPlant_TerminalThun_003EB68E": {
        "fragment_terminal_01"
    },
}


def _merged(script_name: str) -> str:
    relative_path = _script_relative_path(script_name, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), SCRIPT_MEMBERS.items())
def test_terminal_bulk_patches_merge_idempotently_and_compile_full_sources(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "property "))
        for line in patch.splitlines()
    )

    merged = _merged(script_name)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for expected_member in expected_members:
        assert members.count(expected_member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    relative_path = _script_relative_path(script_name, ".psc")
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


def test_terminal_bulk_local_actions_remain_bounded_and_repeat_safe():
    holotape_patch = _script_patch_source(
        "Fragments:Terminals:TERM_RSVP00_Terminal_Sub_Hub_003E1FC0"
    )
    notifications_patch = _script_patch_source(
        "Fragments:Terminals:TERM_EN06_RegistrationTermin_002B47C2"
    )
    door_patch = _script_patch_source(
        "Fragments:Terminals:TERM_LC006_ReactorAccessCont_003C2E3D"
    )
    vault_patch = _script_patch_source(
        "Fragments:Terminals:TERM_V96_Access_TerminalVaul_00324189"
    )
    assert holotape_patch is not None
    assert notifications_patch is not None
    assert door_patch is not None
    assert vault_patch is not None

    assert "GetItemCount(PatrolHolotape) == 0" in holotape_patch
    assert "SetValue(EN06_OptOutofNotifications, 1.0)" in notifications_patch
    assert "SetValue(EN06_OptOutofNotifications, 0.0)" in notifications_patch
    assert "linkedDoor.Unlock(False)" in door_patch
    assert "linkedDoor.SetOpen(True)" in door_patch
    assert "V96VaultGearDoor.SetOpen(True)" in vault_patch


def test_terminal_bulk_item_and_controller_actions_are_repeat_safe():
    rations_patch = _script_patch_source(
        "Fragments:Terminals:TERM_RSz00_SelfServeTerminal_003B3308"
    )
    key_patch = _script_patch_source(
        "Fragments:Terminals:TERM_P01A_Nukashine_JudysTer_0046D780"
    )
    power_patch = _script_patch_source(
        "Fragments:Terminals:TERM_PowerPlant_TerminalPose_003EB685"
    )
    assert rations_patch is not None
    assert key_patch is not None
    assert power_patch is not None

    assert rations_patch.count("playerRef.AddItem(") == 8
    assert "playerRef.GetValue(GotKit) < 1.0" in rations_patch
    assert "playerRef.SetValue(GotKit, 1.0)" in rations_patch
    assert "GetItemCount(P01A_Nukashine_DistillerySupplyPassword) == 0" in key_patch
    assert "linkedController.Activate(akTerminalRef, False)" in power_patch
