from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCH_CASES = {
    "COMP_RQ_MasterScript": {
        "resolvekeywordglobal",
        "resolvequesttype",
        "resolvequestlocation",
        "resolvequestobject",
        "resolvequesttargetactor",
        "resolvequestenemy",
    },
    "COMP_RQ_Restore_Fetch_Script": {"restorequestcustom", "clearobjectaliases"},
    "COMP_RQ_SpecificAliasesScript": {"onquestinit", "copyreferencealias"},
    "COMP_RQ_Script": {
        "onquestinit",
        "onstageset",
        "onquestshutdown",
        "refreshruntimereferences",
        "getfirstobjective",
        "getsecondobjective",
        "clearradiantqueststate",
        "trycompleteradiantquest",
        "radiantcompletionpaysout",
        "completionrewardscriptcoversstage",
        "nativecompletionxpiscarried",
    },
    "CompanionConversationScript": {"onload", "onunload", "ondeath", "ontimer"},
    "CompanionScript": {
        "oninit",
        "onload",
        "initializesingleplayerstate",
        "synchronizeactorvalues",
        "startdailyquest",
        "findradiantquestdataindex",
        "startradiantquestbyindex",
        "getcurrentradiantfirstobjective",
        "getcurrentradiantsecondobjective",
        "getcurrentradiantquestcooldown",
        "getcurrentradiantquest",
        "setrqacceptancestage",
        "trystartoutroquest",
    },
    "COMP_PlayerReturnToQuestGiverInfo": {"onend"},
    "CompanionVisitorScript": {
        "onload",
        "onunload",
        "ondeath",
        "ontimer",
        "checkandprocessvisitors",
        "visitordatummatches",
        "hasspawnedvisitor",
        "spawnvisitor",
        "cleanupexpiredvisitors",
        "findvisitordatum",
    },
    "Fragments:Packages:PF_W05_Beckett_FinalAlliesLe_005A13C2": {
        "fragment_end"
    },
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


def _member_name_list(source: str) -> list[str]:
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
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_companion_radiant_patch_merges_once(
    script_name: str, expected_members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _member_names(patch) == expected_members

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1
    for expected_member in expected_members:
        assert _member_name_list(merged).count(expected_member) == 1


def test_companion_radiant_contract_uses_exact_single_player_routes() -> None:
    companion = _merged_source("CompanionScript")
    quest = _merged_source("COMP_RQ_Script")
    specific_aliases = _merged_source("COMP_RQ_SpecificAliasesScript")
    visitor = _merged_source("CompanionVisitorScript")

    assert "If player.GetValue(currentDatum.PlayerAV) > synchronizedValue" in companion
    assert "currentDatum.QuestCountEqualOrGreaterThan" in companion
    assert "specificAliases.Start()" not in companion
    assert "_COMP_RQ_Ins = specificAliases.QuestTarget as COMP_RQ_Script" in companion
    assert (
        "currentDatum.StoryEventKeyword.SendStoryEventAndWait(eventLocation, Self, "
        "currentDatum.akRef2, currentDatum.aiValue1, eventValue2)"
    ) in companion
    assert "_COMP_RQ_Ins.SetStage(_COMP_RQ_Ins.QuestStageAcceptQuest)" in companion
    assert (
        "auiStageID != QuestCompletionStage_DO_NOT_SET_QUESTSTAGE_AV && "
        "auiStageID != QuestShutDownStage_DO_NOT_SET_QUESTSTAGE_AV"
    ) in quest
    assert "PlayerRef.SetValue(CompanionRef.PlayerQuestCountAV, questCount)" in quest
    assert (
        "CompanionRef.GetCurrentRadiantFirstObjective()" in quest
    )
    assert (
        "CompanionRef.GetCurrentRadiantSecondObjective()" in quest
    )
    assert "CompanionRef.GetCurrentRadiantQuestCoolDown()" in quest
    assert "Return CurrentRadiantQuestExtraData.CustomFirstObjective" in companion
    assert "Return CurrentRadiantQuestExtraData.CustomSecondObjective" in companion
    assert "Return CurrentRadiantQuestExtraData.NextQuestAllowedCoolDown" in companion
    assert "QuestTarget.Start()" not in specific_aliases
    assert (
        "StoryEventKeyword.SendStoryEventAndWait(Alias_Location.GetLocation(), "
        "Alias_Companion.GetReference(), Alias_Object.GetReference())"
        in specific_aliases
    )
    assert "Stop()" in specific_aliases
    assert "COMP_VisitorStart.SendStoryEventAndWait(GetCurrentLocation(), Self, visitor)" in visitor
    assert "COMP_Visitor.Start()" not in visitor


def test_restore_conversation_and_package_contracts_are_bounded() -> None:
    restore = _merged_source("COMP_RQ_Restore_Fetch_Script")
    conversation = _merged_source("CompanionConversationScript")
    package = _merged_source(
        "Fragments:Packages:PF_W05_Beckett_FinalAlliesLe_005A13C2"
    )

    assert "COMP_RQ_Fetch_KnownObjectsList.Find(currentObject.GetBaseObject())" in restore
    assert "objectIndex = 0" in restore
    assert "restoreQuestStage >= RestoreStage_AddObjectToPlayer" in restore
    assert "GetValue(COMP_AV_VisitorWaiting) <= 0.0" in conversation
    assert "StartTimer(TimerDuration, 1)" in conversation
    assert "akActor.Disable()" in package


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_companion_radiant_patch_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged_dependency_root = tmp_path / "merged"
    merged_dependency_root.mkdir()
    (merged_dependency_root / "CompanionScript.psc").write_text(
        _merged_source("CompanionScript"), encoding="utf-8"
    )
    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(merged_dependency_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
