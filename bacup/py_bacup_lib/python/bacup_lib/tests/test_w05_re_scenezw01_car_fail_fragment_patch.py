from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
SCRIPT_NAME = "Fragments:Quests:QF_W05_RE_SceneZW01_0056368E"
EXPECTED_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_0020_item_00",
    "fragment_stage_1000_item_00",
}
INTENTIONAL_EMPTY_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_1000_item_00",
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind == "function"
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind == "function" and name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _executable_lines(body: str) -> list[str]:
    return [
        line.strip()
        for line in body.splitlines()[1:-1]
        if line.strip() and not line.lstrip().startswith(";")
    ]


def _production_skeleton() -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(SCRIPT_NAME, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def test_scenezw01_patch_keeps_the_mixed_member_shape_exact():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    names = _member_names(patch)
    assert set(names) == EXPECTED_MEMBERS
    assert len(names) == 3
    assert set(Counter(names).values()) == {1}

    for member_name in INTENTIONAL_EMPTY_MEMBERS:
        assert _executable_lines(_member_body(patch, member_name)) == []

    stage_20 = _member_body(patch, "fragment_stage_0020_item_00")
    assert _executable_lines(stage_20) == [
        "ObjectReference carRef = Alias_Car.GetReference()",
        "If carRef != None",
        "carRef.DamageObject(9999.0)",
        "EndIf",
    ]
    assert "PlaceAtMe(" not in stage_20
    assert "SetDestroyed(" not in stage_20
    assert "SetStage(" not in stage_20


def test_scenezw01_production_merge_is_idempotent_and_full_source_compiles():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merge_script_method_patches(_production_skeleton(), patch)

    names = _member_names(merged)
    assert set(names) == EXPECTED_MEMBERS
    assert all(names.count(member_name) == 1 for member_name in EXPECTED_MEMBERS)
    for member_name in EXPECTED_MEMBERS:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Fragments/Quests/QF_W05_RE_SceneZW01_0056368E.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
