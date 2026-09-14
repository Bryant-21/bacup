from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
PROPERTY_SHELLS = {
    "W05_MQ_101P_QuestScript": (
        "referencealias Property MegAtTopOfTheWorld Auto mandatory",
        "referencealias Property RaiderBAtTopOfTheWorld Auto mandatory",
        "referencealias Property RaiderAAtTopOfTheWorld Auto mandatory",
    ),
    "W05_MQ_101P_CurrentPlayerScript": (
        "ReferenceAlias Property BiometricScanner Auto mandatory",
    ),
    "W05_MQ_101P_A_MegAliasScript": (
        "quest Property W05_MQ_101P Auto mandatory",
        "ReferenceAlias Property MegAtToTW Auto mandatory",
        "ReferenceAlias Property RaiderAAtToTW Auto mandatory",
        "ReferenceAlias Property RaiderBAtToTW Auto mandatory",
    ),
}

# Members each patch must supply. The FO76 client PEX are server-stripped, so the
# empty shells are evidence of stripping, not of intentionally empty scripts:
# W05_MQ_101P stage 1200 has no other setter anywhere in the converted output,
# and W05_MQ_101P aliases 42/43/44 ship with a NULL forced reference plus the
# Optional flag, so only W05_MQ_101P_A can populate them.
REQUIRED_PATCH_MEMBERS = {
    "W05_MQ_101P_QuestScript": {"filltopoftheworldaliases"},
    "W05_MQ_101P_CurrentPlayerScript": {"onsit"},
    "W05_MQ_101P_A_MegAliasScript": {"onaliasinit", "pushtopoftheworldrefs"},
}


@pytest.mark.parametrize(
    ("script_name", "expected_properties"), PROPERTY_SHELLS.items()
)
def test_101p_bound_source_scripts_are_stripped_property_shells(
    script_name: str, expected_properties: tuple[str, ...]
):
    pex_path = SOURCE_PEX_ROOT / f"{script_name}.pex"
    assert pex_path.is_file(), pex_path
    source = decompile_pex(pex_path, fo4_api_compat=True)

    assert _iter_top_level_papyrus_members(source.splitlines()) == []
    assert _iter_papyrus_states(source.splitlines()) == []
    assert all(property_line in source for property_line in expected_properties)


@pytest.mark.parametrize(
    ("script_name", "expected_members"), REQUIRED_PATCH_MEMBERS.items()
)
def test_101p_property_shells_are_repaired_by_patches(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None, script_name

    members = _iter_top_level_papyrus_members(patch.splitlines())
    assert {name for _kind, name, _start, _end in members} == expected_members
    assert _iter_papyrus_states(patch.splitlines()) == []
    for _kind, _name, start, end in members:
        body = patch.splitlines()[start + 1 : end]
        assert any(
            line.strip() and not line.lstrip().startswith(";") for line in body
        ), f"{script_name}: hollow member"
