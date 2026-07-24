from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# Every script patched by shard
# w2-deterministic-item-inventory-currency-transaction-ungrouped-deterministic-item-inventory-currency-transaction,
# mapped to the top-level member(s) its patch must supply. VendorInteractChoiceScript
# and Perks:FrogCollectingPerkScript were reclassified `record-dependency` (the live
# converted plugin dropped their VMAD entirely) and are intentionally NOT patched —
# see the shard contract's "Record-dependency rows" section.
#
# All of these are deterministic guarded one-shot handlers (no named states or
# timers of their own) — a full compile of the merged patch is sufficient
# coverage; see repair-papyrus-stubs SKILL.md's dedicated-test-file criteria.
PATCH_CASES = {
    "capsStashScript": {"onactivate"},
    "RSVP00_OnContainerChangedSetAV": {"onequipped"},
    "VSTempResourceCollectorScript": {"onactivate"},
    "EggClusterContainerScript": {"oninit", "onitemremoved"},
    "MTRZ05_MapScript": {"onequipped"},
    "OnActivateAddItem": {"onactivate"},
    "TalesFromWV_OnActivateAddItem": {"onactivate"},
    "MQ_Overseer_HolotapeScript": {"onequipped"},
    "Fishing:LindaLeeChumTroughScript": {"onactivate"},
}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_patch_supplies_expected_members_and_merges_cleanly(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert expected_members <= _member_names(patch)

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_vendor_interact_choice_and_frog_collecting_perk_are_not_patched():
    """Both were reclassified `record-dependency`: the live converted plugin
    dropped VMAD entirely on every affected record, so there is no binding for
    a script-body patch to attach to. See contract sections 5 and 7."""
    assert _script_patch_source("VendorInteractChoiceScript") is None
    assert _script_patch_source("Perks:FrogCollectingPerkScript") is None
