from __future__ import annotations

from pathlib import Path

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
CONTRACT_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "remaining-standalone-quest-gaps-2026-09-03.md"
)
HOME_SCRIPT_NAME = "Fragments:Quests:QF_SHELS01_OpenHouse_005C390B"
DAVENPORT_SCRIPT_NAME = "Fragments:TopicInfos:TIF_W05_DialogueDavenport_0056F021"
EXPECTED_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (0, 100, 200, 300, 400, 500, 9000)
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> tuple[str, str]:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return patch, _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_contract_resolves_all_six_candidate_gap_identities():
    contract = CONTRACT_PATH.read_text(encoding="utf-8")
    for form_id, editor_id in (
        ("00454CB6", "E01B_Encryptid"),
        ("004845B0", "P01C_Bucket"),
        ("0047A44D", "P01A_Nukashine"),
        ("00560B13", "E05_Caravan"),
        ("0055B16E", "W05_Community_RaiderFishCamp_Quest"),
        ("005C390B", "SHELS01_OpenHouse"),
    ):
        assert form_id in contract
        assert editor_id in contract


def test_home_expansion_patch_is_member_only_complete_and_idempotent():
    patch, merged = _merged_source(HOME_SCRIPT_NAME)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in patch.splitlines())
    assert set(_member_names(patch)) == EXPECTED_MEMBERS

    merged_members = _member_names(merged)
    for member in EXPECTED_MEMBERS:
        assert merged_members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_home_expansion_local_stage_spine_is_reachable_and_no_crafting_is_added():
    patch, _merged = _merged_source(HOME_SCRIPT_NAME)
    assert "Fragment_Stage_0000_Item_00" in patch
    assert "SetStage(100)" in patch
    assert "MrClarkScene_200.Start()" in patch
    assert "SetObjectiveCompleted(10, True)" in patch
    assert "SetObjectiveDisplayed(20, True)" in patch
    assert "SetObjectiveCompleted(20, True)" in patch
    assert "SetObjectiveDisplayed(30, True)" in patch
    assert "playerRef.SetValue(QuestCompletedValue, 1.0)" in patch
    assert "SetStage(9000)" in patch
    assert "Stop()" in patch
    for forbidden in ("Recipe", "Craft", "ConstructibleObject", "AddItem"):
        assert forbidden.casefold() not in patch.casefold()


def test_home_expansion_full_merged_source_native_compiles(tmp_path: Path):
    _patch, merged = _merged_source(HOME_SCRIPT_NAME)
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    source_path = merged_source_root / _script_relative_path(HOME_SCRIPT_NAME, ".psc")
    source_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(merged_source_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(HOME_SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{HOME_SCRIPT_NAME}\n{diagnostics}"
    assert result.pex_bytes is not None


def test_bucket_list_uses_exact_story_manager_start_path_before_pointer_dispatch():
    patch, merged = _merged_source(DAVENPORT_SCRIPT_NAME)
    exact_lookup = 'Game.GetFormFromFile(0x004845B4, "SeventySix.esm") as Keyword'
    main_dispatch = (
        "started = bucketStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    result_check = (
        "If !started || (!P01C_Bucket.IsRunning() && !P01C_Bucket.IsCompleted())"
    )
    pointer_dispatch = (
        "P01C_BucketMisc_StartQuestKeyword.SendStoryEvent(None, playerRef, playerRef)"
    )

    assert exact_lookup in patch
    assert main_dispatch in patch
    assert result_check in patch
    assert "P01C_Bucket.Start()" not in patch
    assert "P01C_Bucket.Start()" not in merged
    assert "P01C_Bucket.IsRunning() && P01C_BucketMisc_StartQuestKeyword != None" in patch
    assert patch.index(main_dispatch) < patch.index(result_check)
    assert patch.index(result_check) < patch.index(pointer_dispatch)


def test_bucket_list_full_merged_source_native_compiles(tmp_path: Path):
    _patch, merged = _merged_source(DAVENPORT_SCRIPT_NAME)
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    source_path = merged_source_root / _script_relative_path(
        DAVENPORT_SCRIPT_NAME, ".psc"
    )
    source_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(merged_source_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(DAVENPORT_SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{DAVENPORT_SCRIPT_NAME}\n{diagnostics}"
    assert result.pex_bytes is not None
