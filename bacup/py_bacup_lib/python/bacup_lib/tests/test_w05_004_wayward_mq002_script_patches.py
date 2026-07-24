from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

PATCHED_SCRIPTS = {
    "W05_004P_Crane_DispenserTriggerScript": {"onactivate", "ontimer"},
    "W05_DnD_MainDoor_Script": {"onopen"},
    "W05_MQ_002P_RadioTerminalScript": {"onmenuitemrun"},
    "W05_MQ_002P_StartSceneOnTriggerEnter": {"ontriggerenter"},
}
ZERO_MEMBER_SCRIPT = "W05_MQ_001P_Wayward_LaceyIselaTrigger"


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / f"{script_name}.pex"
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _merged_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize(("script_name", "expected_members"), PATCHED_SCRIPTS.items())
def test_verified_w05_patches_supply_only_surviving_members(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "property "))
        for line in patch.splitlines()
    )
    assert _member_names(patch) == expected_members

    merged = _merged_source(script_name)
    assert _member_names(merged) == expected_members
    assert _merge_script_method_patches(merged, patch) == merged


def test_crane_dispenser_preserves_transaction_and_active_state_paths():
    merged = _merged_source("W05_004P_Crane_DispenserTriggerScript")

    for snippet in (
        "akActionRef != playerRef || InActivateCooldown",
        "MessageToDisplay.Show() != MessageConfirmIndex",
        "playerRef.GetItemCount(W05_MQ_004P_Crane_MegaToken) < 1",
        "playerRef.RemoveItem(W05_MQ_004P_Crane_MegaToken, 1, True)",
        "playerRef.AddItem(RewardList, 1, False)",
        "W05_MQ_004P_Crane.SetStage(StageToSetOnAcquireItem)",
        "State active",
        "Event OnBeginState(String asOldState)",
        "Event OnTimer(Int aiTimerID)",
    ):
        assert snippet in merged


def test_lacey_isela_trigger_is_a_zero_member_carrier_without_a_safe_patch_surface():
    skeleton = _production_skeleton(ZERO_MEMBER_SCRIPT)

    assert _script_patch_source(ZERO_MEMBER_SCRIPT) is None
    assert _member_names(skeleton) == set()
    assert "Int PlayerTriggerCount = 0" in skeleton
    assert "keyword Property LinkKeyword Auto" in skeleton
    assert "Float Property AttentionValue Auto mandatory" in skeleton
    assert "Float Property IdleValue Auto mandatory" in skeleton
    assert "actorvalue Property AV Auto mandatory" in skeleton
    assert "State idle" in skeleton
    assert "State playerattention" in skeleton


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_verified_w05_patches_native_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
