from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Creatures:_Default:SetOnFireScript"
RAW_PEX_PATH = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "creatures"
    / "_default"
    / "setonfirescript.pex"
)
DEPLOYED_PEX_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "Creatures"
    / "_Default"
    / "SetOnFireScript.pex"
)


def test_set_on_fire_script_remains_an_unpatched_memberless_carrier():
    assert _script_patch_source(SCRIPT_NAME) is None

    for pex_path in (RAW_PEX_PATH, DEPLOYED_PEX_PATH):
        assert pex_path.is_file(), f"PEX unavailable: {pex_path}"
        source = decompile_pex(pex_path, fo4_api_compat=True)
        assert "extends activemagiceffect" in source.lower()
        assert _iter_top_level_papyrus_members(source.splitlines()) == []
