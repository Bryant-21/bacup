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
    "Fragments:Terminals:TERM_LC006_SecurityRoomMainT_0013DD78": {
        "startlockdownscene",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_05",
    },
    "Fragments:Terminals:TERM_MTR11_Terminal_A_003D00E2": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MTR11_Terminal_B_003D00E3": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MTR11_Terminal_C_003D00E4": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_RSVP00_Terminal_Databas_00309C91": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_BoSZ03_ArtilleryNotific_00313080": {
        "fragment_terminal_03"
    },
    "Fragments:Terminals:TERM_MTR06_SystemTerminal_Sc_003AAB79": {
        "fragment_terminal_01",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_WL006_TurretSecurityTer_005A010E": {
        "powerlinkedturrets",
        "fragment_terminal_01",
    },
    "Fragments:Terminals:TERM_LC101ControlTerminal_0014E6F0": {
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_05",
    },
    "Fragments:Terminals:TERM_FS_AbbiePersonalTermina_003D75B5": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_FS01_AbbiesTerminal_0002A7AA": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_RSVP03_Terminal_Sub_Mig_004F683E": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_SFL02_Track_VertibotTer_00184A03": {
        "fragment_terminal_05"
    },
    "Fragments:Terminals:TERM_FFG01_Terminal_Responde_00379240": {
        "setplayerterminalvalue",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_05",
        "fragment_terminal_06",
    },
}


def _merged(script_name: str) -> str:
    relative_path = _script_relative_path(script_name, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), SCRIPT_MEMBERS.items())
def test_terminal_deep_tranche2_merges_idempotently_and_compiles_full_source(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    patch_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert len(patch_members) == len(expected_members)
    assert set(patch_members) == expected_members

    merged = _merged(script_name)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
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


def test_terminal_deep_tranche2_semantics_follow_bound_owners():
    lockdown = _script_patch_source(
        "Fragments:Terminals:TERM_LC006_SecurityRoomMainT_0013DD78"
    )
    panels = [
        _script_patch_source(f"Fragments:Terminals:TERM_MTR11_Terminal_{letter}_{form}")
        for letter, form in (("A", "003D00E2"), ("B", "003D00E3"), ("C", "003D00E4"))
    ]
    newsletter = _script_patch_source(
        "Fragments:Terminals:TERM_RSVP00_Terminal_Databas_00309C91"
    )
    reward = _script_patch_source(
        "Fragments:Terminals:TERM_BoSZ03_ArtilleryNotific_00313080"
    )
    assert lockdown is not None
    assert all(panel is not None for panel in panels)
    assert newsletter is not None
    assert reward is not None

    for scene in (
        "FacilityLockdownStart",
        "FacilityLockdownEnd",
        "ReactorLockdownStart",
        "ReactorLockdownEnd",
    ):
        assert scene in lockdown
    for index, panel in enumerate(panels, start=1):
        assert f"MTR11_Panel0{index}_Scene.Start()" in panel
    assert "RSVP00_NewsletterSpawnRef.PlaceAtMe" in newsletter
    assert "playerRef.GetValue(pBoSz03RegisteredForRecon) < 1.0" in reward


def test_terminal_deep_tranche2_story_and_status_updates_keep_exact_ownership():
    abbie = _script_patch_source(
        "Fragments:Terminals:TERM_FS_AbbiePersonalTermina_003D75B5"
    )
    statuses = _script_patch_source(
        "Fragments:Terminals:TERM_FFG01_Terminal_Responde_00379240"
    )
    vertibot = _script_patch_source(
        "Fragments:Terminals:TERM_SFL02_Track_VertibotTer_00184A03"
    )
    assert abbie is not None
    assert statuses is not None
    assert vertibot is not None

    assert "BoS01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)" in abbie
    assert "BoS01.Start()" not in abbie
    for value in (
        "TylerCountyStatus",
        "PointPleasantStatus",
        "FlatwoodsStatus",
        "HelvetiaStatus",
        "SummersvilleStatus",
        "MorgantownStatus",
    ):
        assert f"SetPlayerTerminalValue({value})" in statuses
    assert "playerRef.SetValue(pBoSz01_PlayerKACacheDepot, 1.0)" in vertibot
