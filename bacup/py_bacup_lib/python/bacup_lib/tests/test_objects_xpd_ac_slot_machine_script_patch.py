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
    / "XPD_AC_SlotMachine.psc"
)


def test_xpd_ac_slot_machine_patch_defers_network_authoritative_surface():
    patch = _script_patch_source("Objects:XPD_AC_SlotMachine")
    assert patch is not None
    assert patch.strip() == "; TODO"

    source = SOURCE_PATH.read_text(encoding="utf-8")
    merged = _merge_script_method_patches(source, patch)
    assert merged == source
    assert "Event OnSyncVariableNetworkChanged(String varName)" in merged
    assert "Event OnActivate(" not in merged
