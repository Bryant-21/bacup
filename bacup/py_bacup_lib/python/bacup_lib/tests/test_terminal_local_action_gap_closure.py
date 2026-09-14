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
    "Fragments:Terminals:TERM_EN01_SamsPersonalTermin_0031140E": {"fragment_terminal_02"},
    "Fragments:Terminals:TERM_LC179_MonorailTerminal_004E22DF": {
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_LC022_CouncilRoomTermin_004E6101": {"fragment_terminal_01"},
    "Fragments:Terminals:TERM_LC022_InfirmaryTerminal_00206DD1": {"fragment_terminal_01"},
    "Fragments:Terminals:TERM_MoM_CouncilChamberTermi_004ECFC0": {"fragment_terminal_01"},
    "Fragments:Terminals:TERM_MoM_InfirmaryTerminalLo_004ECFBE": {"fragment_terminal_01"},
    "Fragments:Terminals:TERM_X01X_PlayerTerminal_Ale_00417C32": {"fragment_terminal_03"},
    "Fragments:Terminals:TERM_X01X_PlayerTerminal_Ale_00537EC0": {"fragment_terminal_01"},
    "Fragments:Terminals:TERM_X01X_PlayerTerminal_Ale_00537EC1": {"fragment_terminal_02"},
    "Fragments:Terminals:TERM_X01X_PlayerTerminal_Ale_00566191": {"fragment_terminal_04"},
}


def _merged(script_name: str) -> str:
    relative_path = _script_relative_path(script_name, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), SCRIPT_MEMBERS.items())
def test_terminal_local_action_patches_merge_once_and_compile_full_sources(
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


def test_terminal_local_actions_keep_transactions_bounded_and_repeat_safe():
    marker_patch = _script_patch_source(
        "Fragments:Terminals:TERM_LC179_MonorailTerminal_004E22DF"
    )
    council_patch = _script_patch_source(
        "Fragments:Terminals:TERM_MoM_CouncilChamberTermi_004ECFC0"
    )
    alert_patch = _script_patch_source(
        "Fragments:Terminals:TERM_X01X_PlayerTerminal_Ale_00417C32"
    )
    assert marker_patch is not None
    assert council_patch is not None
    assert alert_patch is not None

    assert marker_patch.count(".AddToMap(False)") == 4
    assert "GetItemCount(MoMHolotapeCouncil) == 0" in council_patch
    assert "akTerminalRef.SetValue(MoMCouncilChamberTerminalValue, 1.0)" in council_patch
    assert "Arktos_Startkeyword.SendStoryEventAndWait(None, Game.GetPlayer())" in alert_patch
    assert ".Start()" not in alert_patch
