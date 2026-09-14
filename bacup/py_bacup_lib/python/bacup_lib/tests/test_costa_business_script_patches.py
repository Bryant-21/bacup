from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
FAMILY_SCRIPT = "QUESTS:E05_Caravan:Master_QuestScript"
QUEST_FRAGMENT_STAGES = {
    "Fragments:Quests:QF_Moon_SQ02_Eugenie_006A173A": {
        100,
        200,
        300,
        400,
        425,
        475,
        500,
        600,
        750,
        9000,
        9999,
    },
    "Fragments:Quests:QF_Moon_SQ04_Rudy_006A0F94": {
        100,
        200,
        210,
        220,
        230,
        300,
        410,
        420,
        430,
        440,
        450,
        500,
        600,
        610,
        620,
        630,
        9000,
        9999,
    },
    "Fragments:Quests:QF_Moon_SQ03_Libby_006A1030": {
        100,
        200,
        300,
        325,
        350,
        375,
        380,
        400,
        500,
        510,
        520,
        530,
        600,
        610,
        620,
        9000,
    },
    "Fragments:Quests:QF_Moon_SQ05_Herschel_006A17A9": {
        100,
        200,
        300,
        410,
        420,
        430,
        440,
        450,
        500,
        600,
        610,
        620,
        700,
        800,
        9000,
    },
    "Fragments:Quests:QF_Moon_SQ06_Vera_0069F2C7": {
        100,
        110,
        200,
        205,
        210,
        240,
        250,
        255,
        260,
        265,
        300,
        405,
        410,
        415,
        420,
        425,
        430,
        500,
        550,
        610,
        620,
        630,
        640,
        650,
        700,
        9000,
        9999,
    },
    "Fragments:Quests:QF_MOON_SQ07_Carver_006A21E3": {
        100,
        200,
        300,
        400,
        450,
        475,
        500,
        610,
        620,
        700,
        9000,
    },
    "Fragments:Quests:QF_MOON_SQ08_Aries_006A21E4": {
        100,
        200,
        410,
        420,
        430,
        440,
        450,
        550,
        600,
        650,
        700,
        800,
        850,
        900,
        950,
        9999,
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize("script_name", [FAMILY_SCRIPT, *QUEST_FRAGMENT_STAGES])
def test_costa_patches_are_member_only_and_merge_idempotently(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    patch_members = _member_names(patch)
    merged_members = _member_names(merged)
    assert len(patch_members) == len(set(patch_members))
    for member in patch_members:
        assert merged_members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(("script_name", "stages"), QUEST_FRAGMENT_STAGES.items())
def test_costa_fragments_cover_every_bound_stage_member(
    script_name: str, stages: set[int]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    members = set(_member_names(patch))
    for stage in stages:
        assert f"fragment_stage_{stage:04d}_item_00" in members

    if script_name.endswith("Vera_0069F2C7"):
        assert "fragment_stage_0240_item_01" in members
        assert "fragment_stage_0240_item_02" in members


def test_costa_scheduler_uses_daily_registration_tokens_and_story_keywords():
    master_patch = _script_patch_source("sq_masterscript")
    family_patch = _script_patch_source(FAMILY_SCRIPT)
    assert master_patch is not None
    assert family_patch is not None

    assert master_patch.count("RefreshCostaBusinessEligibility()") == 4
    assert 'Game.GetFormFromFile(0x0056B640, "SeventySix.esm")' in master_patch

    for start_keyword in (
        "006CAC85",
        "006CC9A5",
        "006CADEE",
        "006CADC8",
        "006CCAF3",
        "006A9EEB",
        "006CAC86",
        "006CAC84",
    ):
        assert start_keyword in family_patch
    assert "SendStoryEventAndWait" in family_patch
    assert 'RegisterForRemoteEvent(vinny, "OnActivate")' in family_patch
    assert 'UnregisterForRemoteEvent(akVinny, "OnActivate")' in family_patch
    assert family_patch.count(".Start()") == 1
    assert "started = eugenieQuest.Start()" in family_patch
    assert "startKeyword == eugenieStartKeyword" in family_patch
    assert "RefreshCostaBusinessEligibility()" in family_patch
    assert "003FBF2E" in family_patch
    assert "005F5E1A" in family_patch


def test_costa_local_substitutes_are_binding_guarded():
    libby = _script_patch_source("Fragments:Quests:QF_Moon_SQ03_Libby_006A1030")
    vera = _script_patch_source("Fragments:Quests:QF_Moon_SQ06_Vera_0069F2C7")
    protocol = _script_patch_source("Fragments:Quests:QF_MOON_SQ08_Aries_006A21E4")
    assert libby is not None
    assert vera is not None
    assert protocol is not None

    assert 'Game.GetFormFromFile(0x006B5BE3, "SeventySix.esm")' in libby
    assert "MiscObject_RepairParts" in libby
    assert "DefaultAliasOnActivateGiveItem" not in libby
    for near_stage, photo_stage in ((405, 410), (415, 420), (425, 430)):
        assert f"Fragment_Stage_{near_stage:04d}_Item_00" in vera
        assert f"SetStage({photo_stage})" in vera
    assert "PlaceHolotape(" in protocol
    assert "Alias_container_Holo_ScootsCabin_05" in protocol


def test_costa_full_merged_sources_native_compile(tmp_path: Path):
    script_names = [FAMILY_SCRIPT, *QUEST_FRAGMENT_STAGES]
    merged_root = tmp_path / "Scripts" / "Source" / "User"
    merged_root.mkdir(parents=True)

    merged_sources: dict[str, str] = {}
    for script_name in script_names:
        merged = _merged_source(script_name)
        merged_sources[script_name] = merged
        destination = merged_root / _script_relative_path(script_name, ".psc")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    for script_name, merged in merged_sources.items():
        result = compile_psc(
            merged,
            imports=[str(merged_root), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}\n{diagnostics}"
        assert result.pex_bytes is not None
