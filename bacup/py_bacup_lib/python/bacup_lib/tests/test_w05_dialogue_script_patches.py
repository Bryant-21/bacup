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
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "topicinfos"
)


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


# 35 of the shard's 48 TIF_W05_Dialogue_* rows are repaired here (Fragment_End
# confirmed live-bound in the converted plugin). The other 13 are non-defect:
# dev-time load-slot-01 authoring residue with zero bindings in both the live
# plugin and the FO76 source ESM (family01_probe.json), independently
# re-verified per-worksheet — see
# bacup/docs/stub_restoration/contracts/w3-w05-dialogue.md Section A.7. Those
# 13 ship no patch: dontrelle_01000fc7_1, dontrelle_01000fd9_1,
# dontrelle_01000fe0_1, dontrelle_01000fe0_2, dontrelleha_01000fc7,
# dontrelleha_01000fd9, dontrelleha_01000fe0, gilberthops_01000972,
# gilberthops_01000975, gilberthops_01000978, gilberthops_0100097b,
# heatherel_01000fe2_1, heatherelli_01000fe2.
TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_Dialogue_BethyMangan_00599806": (
        "akSpeakerRef.SetValue(W05_BethyMangano_IsSmartAV, 1.0)",
    ),
    "TIF_W05_Dialogue_BethyMangan_0059980C": (
        "akSpeakerRef.SetValue(W05_BethyMangano_KnownAV, 1.0)",
    ),
    "TIF_W05_Dialogue_BethyMangan_0059980F": (
        "W05_BethyMangano_GaveSuppliesAV != None",
        "Game.GetPlayer().AddItem(ScienceMagazine, 1)",
        "akSpeakerRef.SetValue(W05_BethyMangano_GaveSuppliesAV, 1.0)",
    ),
    "TIF_W05_Dialogue_BethyMangan_0059A014": (
        "Game.GetPlayer().AddItem(MedIntRock, 1)",
        "akSpeakerRef.SetValue(W05_BethyMangano_GaveSuppliesAV, 1.0)",
    ),
    "TIF_W05_Dialogue_BethyMangan_0059A015": (
        "Game.GetPlayer().AddItem(HighIntRock, 1)",
        "akSpeakerRef.SetValue(W05_BethyMangano_GaveSuppliesAV, 1.0)",
    ),
    "TIF_W05_Dialogue_DontrelleHa_00583D50": (
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToDontrelleHaines, 1.0)",
    ),
    "TIF_W05_Dialogue_DontrelleHa_00583D51": (
        "MorgantownMapMarker.AddToMap(False)",
        "MorgantownStationMapMarker.AddToMap(False)",
        "MorgantownTrainyardMapMarker.AddToMap(False)",
    ),
    "TIF_W05_Dialogue_DontrelleHa_00583D54": (
        "Game.GetPlayer().AddItem(RadAway, 1)",
        "akSpeakerRef.SetValue(W05_DontrelleHainesGaveSupplies, 1.0)",
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToDontrelleHaines, 1.0)",
    ),
    "TIF_W05_Dialogue_FridaMadani_00599449": (
        "Game.GetPlayer().AddItem(Brew_Wine, 1)",
    ),
    "TIF_W05_Dialogue_FuzzyBen_00598B1A": (
        "Game.GetPlayer().AddItem(Caps001, 1)",
    ),
    "TIF_W05_Dialogue_FuzzyBen_00598B25": (
        "Game.GetPlayer().AddItem(MTR04_MrFuzzyToken, 1)",
    ),
    "TIF_W05_Dialogue_GilbertHops_00596E3A": (
        "akSpeakerRef.SetValue(W05_GilbertHopsonMetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_GilbertHops_00596E3D": (
        "Game.GetPlayer().AddItem(BerryMentats, 1)",
        "akSpeakerRef.SetValue(W05_GilbertHopsonGaveSuppliesAV, 1.0)",
        "akSpeakerRef.SetValue(W05_GilbertHopsonMetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_GilbertHops_00596E41": (
        "Game.GetPlayer().AddItem(BerryMentats, 1)",
        "akSpeakerRef.SetValue(W05_GilbertHopsonGaveSuppliesAV, 1.0)",
        "akSpeakerRef.SetValue(W05_GilbertHopsonMetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_GilbertHops_00596E43": (
        "akSpeakerRef.SetValue(W05_GilbertHopsonMetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_HeatherElli_00577574": (
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)",
    ),
    "TIF_W05_Dialogue_HeatherElli_00577579": (
        "Game.GetPlayer().AddItem(Stimpak, 1)",
        "akSpeakerRef.SetValue(W05_HeatherEllisGaveSupplies, 1.0)",
        "akSpeakerRef.SetValue(W05_HeatherEllisLikesPlayer, 1.0)",
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)",
    ),
    "TIF_W05_Dialogue_HeatherElli_0057757A": (
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)",
    ),
    "TIF_W05_Dialogue_HeatherElli_00577580": (
        "Game.GetPlayer().AddItem(Stimpak, 1)",
        "akSpeakerRef.SetValue(W05_HeatherEllisGaveSupplies, 1.0)",
        "akSpeakerRef.SetValue(W05_HeatherEllisDislikesPlayer, 1.0)",
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)",
    ),
    "TIF_W05_Dialogue_HeatherElli_00583D4F": (
        "Game.GetPlayer().AddItem(Stimpak, 1)",
        "akSpeakerRef.SetValue(W05_HeatherEllisGaveSupplies, 1.0)",
        "Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)",
    ),
    "TIF_W05_Dialogue_JoeCreigh_00598B2D": (
        "Game.GetPlayer().AddItem(Ammo308Caliber, 1)",
    ),
    "TIF_W05_Dialogue_Johnny_00585551": (
        "Utility.GetCurrentGameTime()",
        "W05_MQR_JohnnyRepeatablerewardCooldown.GetValue()",
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_Johnny_00585552": (
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_Johnny_00585553": (
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_Johnny_00585554": (
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_Johnny_00585555": (
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_Johnny_00585556": (
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_Johnny_00585557": (
        "Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)",
        "akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)",
    ),
    "TIF_W05_Dialogue_JonahIto_005982BC": (
        "Game.GetPlayer().AddItem(W05_JonahIto_LL_GiveToPlayer, 1)",
        "akSpeakerRef.SetValue(W05_JonahIto_GaveSuppliesAV, 1.0)",
        "akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_JonahIto_005982BF": (
        "akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_JonahIto_005982C0": (
        "akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_JonahIto_005982C4": (
        "Game.GetPlayer().AddItem(W05_JonahIto_LL_GiveToPlayer, 1)",
        "akSpeakerRef.SetValue(W05_JonahIto_GaveSuppliesAV, 1.0)",
        "akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_JonahIto_005982CE": (
        "akSpeakerRef.SetValue(W05_JonahIto_MetPlayerAV, 1.0)",
    ),
    "TIF_W05_Dialogue_MaramAyari_00598B86": (
        "Game.GetPlayer().AddItem(W05_MaramAyari_LL_GiveToPlayer, 1)",
        "akSpeakerRef.SetValue(W05_MaramAyari_GaveSuppliesAV, 1.0)",
    ),
    "TIF_W05_Dialogue_MaramAyari_00598B90": (
        "Game.GetPlayer().AddItem(W05_MaramAyari_LL_GiveToPlayer, 1)",
        "akSpeakerRef.SetValue(W05_MaramAyari_GaveSuppliesAV, 1.0)",
    ),
}

NON_DEFECT_TOPICINFO_NAMES = (
    "TIF_W05_Dialogue_Dontrelle_01000FC7_1",
    "TIF_W05_Dialogue_Dontrelle_01000FD9_1",
    "TIF_W05_Dialogue_Dontrelle_01000FE0_1",
    "TIF_W05_Dialogue_Dontrelle_01000FE0_2",
    "TIF_W05_Dialogue_DontrelleHa_01000FC7",
    "TIF_W05_Dialogue_DontrelleHa_01000FD9",
    "TIF_W05_Dialogue_DontrelleHa_01000FE0",
    "TIF_W05_Dialogue_GilbertHops_01000972",
    "TIF_W05_Dialogue_GilbertHops_01000975",
    "TIF_W05_Dialogue_GilbertHops_01000978",
    "TIF_W05_Dialogue_GilbertHops_0100097B",
    "TIF_W05_Dialogue_HeatherEl_01000FE2_1",
    "TIF_W05_Dialogue_HeatherElli_01000FE2",
)

# Section B (quest-stage fragment) and Section C (coordinator-added shared
# default script) both already have generated skeletons under
# mods/SeventySix/Scripts/Source/User, so they are tested against that source
# directly rather than a deployed PEX.
QUEST_SCRIPT_MEMBERS: dict[str, set[str]] = {
    "Fragments:Quests:QF_W05_Dialogue_SecretServic_0054279C": {
        "fragment_stage_0000_item_00",
    },
    "DefaultShutdownQuestOnChangeLocation": {
        "onquestinit",
        "actor.onlocationchange",
        "onquestshutdown",
    },
}

# --- Folded from the former dialoguedavenport shard (single fragment_begin) ---
DAVENPORT_BASE_NAME = "TIF_W05_DialogueDavenport_0056F021"
DAVENPORT_EXPECTED_SNIPPETS = (
    "Actor playerRef = Game.GetPlayer()",
    "playerRef == None || W05_LookingForCameraAV == None",
    "playerRef.GetValue(W05_LookingForCameraAV) != 0.0",
    "P01C_Bucket != None && !P01C_Bucket.IsRunning() && !P01C_Bucket.IsCompleted()",
    "P01C_Bucket.Start()",
    "playerRef.SetValue(W05_LookingForCameraAV, 1.0)",
    "P01C_BucketMisc_StartQuestKeyword != None",
    "P01C_BucketMisc_StartQuestKeyword.SendStoryEvent(None, playerRef, playerRef)",
)

# --- Folded from the former dialogueradicals shard (single fragment_end) ------
# Only TIF_W05_DialogueRadicals_00411F61 is patched in this shard. The other 4
# rows (0040FA8F/0040FA90/0040FA93: non-defect, carrier record absent from
# both the live FO4 plugin and the current FO76 source ESM; Ext_005895B2:
# evidence-blocked, zero surviving property/variable literal) ship no patch —
# see bacup/docs/stub_restoration/contracts/w3-w05-dialogueradicals.md.
RADICALS_TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_DialogueRadicals_00411F61": (
        "Actor playerRef = Game.GetPlayer()",
        "playerRef != None && W05_RadicalEnemyFaction != None",
        "playerRef.AddToFaction(W05_RadicalEnemyFaction)",
        "playerRef != None && W05_RadicalFriendFaction != None",
        "playerRef.RemoveFromFaction(W05_RadicalFriendFaction)",
        "playerRef != None && W05_MQ_002P_Radical_RadicalFriendValue != None",
        "playerRef.SetValue(W05_MQ_002P_Radical_RadicalFriendValue, 0.0)",
    ),
}

# --- Folded from the former dialogueraiderscrate shard (7 Lou cooldown rows) --
# All 7 rows are random-flavor variants of the same Fragment_End: cooldown-gated
# repeatable-reward grant. Cooldown GlobalVariable value (1440.0) cross-validated
# against the independent Johnny_00585551-557 archetype (identical value, identical
# TimeStamp-AV Description) -- see contracts/w3a-raiders.md Section A and "Binding
# conditions" resolution 1. All 7 share an identical body.
RAIDERSCRATE_TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    name: (
        "W05_MQR_LouRepeatableRewardCooldown.GetValue() / 1440.0",
        "Utility.GetCurrentGameTime() - player.GetValue(W05_MQR_LouRepeatableRewardTimeStampValue)",
        "player.AddItem(W05_LL_LouRepeatableReward, 1, False)",
        "player.SetValue(W05_MQR_LouRepeatableRewardTimeStampValue, Utility.GetCurrentGameTime())",
    )
    for name in (
        "TIF_W05_DialogueRaidersCrate_00585959",
        "TIF_W05_DialogueRaidersCrate_0058595A",
        "TIF_W05_DialogueRaidersCrate_0058595B",
        "TIF_W05_DialogueRaidersCrate_0058595C",
        "TIF_W05_DialogueRaidersCrate_0058595D",
        "TIF_W05_DialogueRaidersCrate_0058595E",
        "TIF_W05_DialogueRaidersCrate_0058595F",
    )
}

# --- Folded from the former dialogueraidersgener shard (2 GenericIntroLines) --
# Both rows share W05_Raiders_GenericIntroLines, a once/boolean AV ("...to make
# sure they only fire off once per player" -- AV 42A0D2 Description). Ruling
# matches the already-shipped Settlers-faction sibling for the identical
# *_GenericIntroLines pattern (TIF_W05_DialogueSettlers_Fou_0058FE03.psc):
# akSpeakerRef.SetValue(AV, 1.0), not an increment -- see
# contracts/w3a-raiders.md Section C and "Binding conditions" resolution 4.
RAIDERSGENER_TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_DialogueRaidersGener_0042A0A0": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Raiders_GenericIntroLines, 1.0)",
    ),
    "TIF_W05_DialogueRaidersGener_0042A0A1": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Raiders_GenericIntroLines, 1.0)",
    ),
}


def _topicinfo_script_name(base_name: str) -> str:
    return f"Fragments:TopicInfos:{base_name}"


def _merged_production_topicinfo_source(base_name: str) -> str:
    pex_path = DEPLOYED_TOPICINFO_ROOT / f"{base_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_topicinfo_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _merged_quest_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("base_name", "expected_snippets"), TOPICINFO_PATCH_CASES.items())
def test_topicinfo_patch_restores_confirmed_fragment_call(
    base_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(_topicinfo_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}
    for snippet in expected_snippets:
        assert snippet in patch


@pytest.mark.parametrize("base_name", TOPICINFO_PATCH_CASES)
def test_topicinfo_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dontrellehaines_gaveitem_row_grants_before_flagging():
    merged = _merged_production_topicinfo_source("TIF_W05_Dialogue_DontrelleHa_00583D54")

    add_index = merged.find("AddItem(RadAway, 1)")
    flag_index = merged.find("SetValue(W05_DontrelleHainesGaveSupplies, 1.0)")

    assert add_index != -1
    assert flag_index != -1
    assert add_index < flag_index


def test_johnny_repeatable_reward_first_use_and_cooldown_guard():
    patch = _script_patch_source(_topicinfo_script_name("TIF_W05_Dialogue_Johnny_00585551"))

    assert patch is not None
    # First-use semantics: an unset (0.0 default) timestamp AV is eligible
    # immediately, not held to a full cooldown from an undefined baseline.
    assert "lastGiven == 0.0" in patch
    # The GlobalVariable cooldown property itself is None-guarded before any
    # .GetValue() call is made on it.
    assert "W05_MQR_JohnnyRepeatablerewardCooldown == None" in patch
    # Cross-shard cooldown-formula alignment (w3a-raiders corroboration, two
    # independently-authored Johnny families both carry a 1440.0 GLOB and an
    # identical timestamp-AV description): the raw GLOB value is
    # game-minutes-per-day, so it must be divided by 1440.0 to compare
    # against Utility.GetCurrentGameTime()'s game-days unit.
    assert "W05_MQR_JohnnyRepeatablerewardCooldown.GetValue() / 1440.0" in patch


def test_all_johnny_repeatable_reward_rows_apply_cooldown_divisor():
    johnny_names = [
        name for name in TOPICINFO_PATCH_CASES if "Johnny" in name
    ]
    assert len(johnny_names) == 7
    for base_name in johnny_names:
        patch = _script_patch_source(_topicinfo_script_name(base_name))
        assert patch is not None
        assert "/ 1440.0" in patch, base_name


def test_no_patch_authored_for_non_defect_orphaned_rows():
    for base_name in NON_DEFECT_TOPICINFO_NAMES:
        assert _script_patch_source(_topicinfo_script_name(base_name)) is None


def test_dialogue_shard_patch_count_matches_contract():
    assert len(TOPICINFO_PATCH_CASES) == 35
    assert len(NON_DEFECT_TOPICINFO_NAMES) == 13
    assert len(TOPICINFO_PATCH_CASES) + len(NON_DEFECT_TOPICINFO_NAMES) == 48


@pytest.mark.parametrize(("script_name", "expected"), QUEST_SCRIPT_MEMBERS.items())
def test_quest_and_default_patch_supplies_expected_members(
    script_name: str, expected: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected <= _member_names(patch)

    merged = _merged_quest_source(script_name)
    assert expected <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


def test_default_shutdown_quest_on_change_location_no_new_declarations():
    patch = _script_patch_source("DefaultShutdownQuestOnChangeLocation")

    assert patch is not None
    # Only the two skeleton-declared properties and function-local variables
    # are referenced; no new script-level Property/variable is introduced
    # (the merger cannot safely add one, and the review gate required this).
    assert "Property" not in patch
    assert "TargetLocation" in patch
    assert "InstanceOwner" in patch


def test_default_shutdown_quest_on_change_location_unregisters_on_shutdown():
    patch = _script_patch_source("DefaultShutdownQuestOnChangeLocation")

    assert patch is not None
    assert "Event OnQuestShutdown()" in patch
    assert "UnregisterForAllRemoteEvents()" in patch


def test_secret_service_fragment_checkpoint_write_targets_player_alias():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_Dialogue_SecretServic_0054279C"
    )

    assert patch is not None
    assert "Alias_Player.GetActorReference()" in patch
    assert "Alias_ReginaldStone.GetActorReference()" in patch
    assert "W05_MQA_206P.IsRunning()" in patch
    assert "playerActor.SetValue(W05_MQS_206P_Checkpoint, 1.0)" in patch


def test_quest_and_default_patch_set_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged_sources: dict[str, str] = {}
    for script_name in QUEST_SCRIPT_MEMBERS:
        source = _merged_quest_source(script_name)
        merged_sources[script_name] = source
        source_path = tmp_path / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(source, encoding="utf-8")

    for script_name, source in merged_sources.items():
        result = compile_psc(
            source,
            imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}:\n{diagnostics}"
        assert result.pex_bytes is not None


# --- Folded from the former dialoguedavenport shard (single fragment_begin) ---


def test_davenport_patch_restores_confirmed_fragment_begin():
    patch = _script_patch_source(_topicinfo_script_name(DAVENPORT_BASE_NAME))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_begin"}
    for snippet in DAVENPORT_EXPECTED_SNIPPETS:
        assert snippet in patch


def test_davenport_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(DAVENPORT_BASE_NAME),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{DAVENPORT_BASE_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# --- Folded from the former dialogueradicals shard (single fragment_end) ------


@pytest.mark.parametrize(
    ("base_name", "expected_snippets"), RADICALS_TOPICINFO_PATCH_CASES.items()
)
def test_radicals_topicinfo_patch_restores_confirmed_fragment_call(
    base_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(_topicinfo_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}
    for snippet in expected_snippets:
        assert snippet in patch


@pytest.mark.parametrize("base_name", RADICALS_TOPICINFO_PATCH_CASES)
def test_radicals_topicinfo_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialogueradicals_patch_count_matches_shard_runbook():
    assert len(RADICALS_TOPICINFO_PATCH_CASES) == 1


# --- Folded from the former dialogueraiderscrate shard (7 Lou cooldown rows) --


@pytest.mark.parametrize(
    ("base_name", "expected_snippets"), RAIDERSCRATE_TOPICINFO_PATCH_CASES.items()
)
def test_raiderscrate_topicinfo_patch_restores_confirmed_fragment_call(
    base_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(_topicinfo_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}
    for snippet in expected_snippets:
        assert snippet in patch


@pytest.mark.parametrize("base_name", RAIDERSCRATE_TOPICINFO_PATCH_CASES)
def test_raiderscrate_topicinfo_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialogueraiderscrate_patch_count_matches_shard_contract():
    assert len(RAIDERSCRATE_TOPICINFO_PATCH_CASES) == 7


# --- Folded from the former dialogueraidersgener shard (2 GenericIntroLines) --


@pytest.mark.parametrize(
    ("base_name", "expected_snippets"), RAIDERSGENER_TOPICINFO_PATCH_CASES.items()
)
def test_raidersgener_topicinfo_patch_restores_confirmed_fragment_call(
    base_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(_topicinfo_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}
    for snippet in expected_snippets:
        assert snippet in patch


@pytest.mark.parametrize("base_name", RAIDERSGENER_TOPICINFO_PATCH_CASES)
def test_raidersgener_topicinfo_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialogueraidersgener_patch_count_matches_shard_contract():
    assert len(RAIDERSGENER_TOPICINFO_PATCH_CASES) == 2
