from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


def _merged() -> str:
    skeleton = (SOURCE_ROOT / "AddItemOnMenuItemRun.psc").read_text(encoding="utf-8")
    patch = _script_patch_source("AddItemOnMenuItemRun")
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_add_item_terminal_uses_bound_menu_transaction():
    merged = _merged()
    assert "Event OnMenuItemRun" in merged
    assert "entry.iMenuItemTarget == auiMenuItemID" in merged
    assert "player.AddItem(entry.ItemToAdd, entry.iItemAmount, False)" in merged
    assert "player.SetValue(entry.BlockingActorValue, entry.iBlockingValue as Float)" in merged


def test_add_item_terminal_patch_compiles():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="AddItemOnMenuItemRun.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
