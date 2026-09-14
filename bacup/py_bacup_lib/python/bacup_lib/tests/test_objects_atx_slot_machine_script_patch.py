from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "Objects"
    / "atxslotmachinescript.psc"
)


def test_atx_slot_machine_patch_defers_unbound_network_only_surface():
    patch = _script_patch_source("Objects:ATXSlotMachineScript")
    assert patch is not None
    # Still a deferred surface (nothing drives the spin locally yet), but the FO76-only
    # OnSyncVariableNetworkChanged event is rejected by the stock FO4 compiler, so the
    # patch drops it and re-homes SpinReels() behind a local entry point.
    assert patch.count("; TODO") == 1
    assert "@drop-member OnSyncVariableNetworkChanged" in patch

    source = SOURCE_PATH.read_text(encoding="utf-8")
    merged = _merge_script_method_patches(source, patch)
    assert "OnSyncVariableNetworkChanged" not in merged
    assert "Function BumpTumblerUpdateTick()" in merged
    assert "Self.SpinReels()" in merged
    assert "Event OnActivate(" not in merged
