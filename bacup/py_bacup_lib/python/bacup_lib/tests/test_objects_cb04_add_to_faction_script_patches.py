from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Objects:CB04_AddToFactionScript"


def test_cb04_add_to_faction_patch_loads_merges_and_compiles_for_fo4():
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)

    assert source_path.is_file(), source_path
    assert patch is not None
    assert "Scriptname " not in patch

    merged = _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)
    assert merged.lower().count("event oneffectstart(") == 1

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
