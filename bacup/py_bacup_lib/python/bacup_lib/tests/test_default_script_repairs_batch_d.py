from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "DefaultRefOnActivateSendEvent"
CONTRACT_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "ad-hoc-default-script-repairs-batch-d.md"
)


def _merged_source() -> str:
    skeleton = (SOURCE_ROOT / f"{SCRIPT_NAME}.psc").read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_default_ref_on_activate_send_event_patch_merges_one_handler():
    patch = _script_patch_source(SCRIPT_NAME)
    contract = CONTRACT_PATH.read_text(encoding="utf-8")

    assert patch is not None
    assert patch.count("; TODO") == 1
    assert "Scriptname " not in patch
    assert SCRIPT_NAME in contract
    assert "BlockWhilePlayerIsInsideShelter" in contract
    assert "Remove the TODO only when" in contract

    merged = _merged_source()
    assert merged.lower().count("event onactivate(objectreference akactionref)") == 1
    assert "If PlayerTriggerOnly && akActionRef != player" in merged
    assert "If BlockWhilePlayerIsSitting && player.GetSitState() != 0" in merged
    assert "If BlockWhilePlayerIsInPowerArmor && player.IsInPowerArmor()" in merged
    assert "If BlockWhilePlayerIsInCombat && player.IsInCombat()" in merged
    assert "SendConfiguredStoryEvent()" in merged


def test_default_ref_on_activate_send_event_merged_source_native_compiles():
    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts")],
        game="fo4",
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
