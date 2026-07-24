from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "topicinfos"
)
DEPLOYED_QUEST_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "quests"
)

# All 12 are once/boolean AVIF flags (DefaultTo0, "0 = not present / 1 = present"
# or "fires off once per player" style descriptions; the 3 undocumented
# Sunny*_Insult AVs share the identical structural CTDA shape -- every sibling
# INFO gates at ==0.0, never a progressive threshold -- see
# contracts/w3-w05-dialoguesettlers.md Section A). SetValue(1.0) is therefore
# correct for every row; none are accumulating counters.
TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_DialogueSettlers_00562162": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Settlers_GenericIntroLines, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0058FE02": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Foundation_GateGuard_Intro, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0058FE03": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Settlers_GenericIntroLines, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0058FE04": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Foundation_GateGuard_Intro, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0058FE05": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Foundation_GateGuard_Intro, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_00595B53": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_Foundation_GateGuard_Intro, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0059EEA7": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_SunnyMisc_Insult, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0059EEA9": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_SunnyFood_Insult, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0059EEAC": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_SunnyArmory_Insult, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0059EEAE": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_SunnyMisc_Insult, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0059EEB1": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_SunnyFood_Insult, 1.0)",
    ),
    "TIF_W05_DialogueSettlers_Fou_0059EEB2": (
        "akSpeakerRef == None",
        "akSpeakerRef.SetValue(W05_SunnyArmory_Insult, 1.0)",
    ),
}

QUEST_BASE_NAME = "QF_W05_DialogueSettlers_Inte_00570D57"


def _topicinfo_script_name(base_name: str) -> str:
    return f"Fragments:TopicInfos:{base_name}"


def _quest_script_name() -> str:
    return f"Fragments:Quests:{QUEST_BASE_NAME}"


def _merged_production_topicinfo_source(base_name: str) -> str:
    pex_path = DEPLOYED_TOPICINFO_ROOT / f"{base_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_topicinfo_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _merged_production_quest_source() -> str:
    pex_path = DEPLOYED_QUEST_ROOT / f"{QUEST_BASE_NAME.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_quest_script_name())
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


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


def test_quest_patch_syncs_presence_avs_and_guards_unfilled_player_alias():
    patch = _script_patch_source(_quest_script_name())

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
    assert members == {"fragment_stage_0100_item_00"}

    guard_index = patch.find("akPlayerRef == None")
    paige_check_index = patch.find("Alias_Paige.GetReference() != None")
    paige_set_index = patch.find("akPlayerRef.SetValue(W05_PaigeIsInFoundation, 1.0)")
    penny_check_index = patch.find("Alias_Penny.GetReference() != None")
    penny_set_index = patch.find("akPlayerRef.SetValue(W05_PennyIsInFoundation, 1.0)")
    jen_check_index = patch.find("Alias_Jen.GetReference() != None")
    jen_set_index = patch.find("akPlayerRef.SetValue(W05_JenIsInFoundation, 1.0)")

    assert guard_index != -1
    assert -1 not in (
        paige_check_index, paige_set_index,
        penny_check_index, penny_set_index,
        jen_check_index, jen_set_index,
    )
    # The unresolved-alias guard runs before any AV write.
    assert guard_index < min(paige_set_index, penny_set_index, jen_set_index)
    # The two *EnableMarker properties (LCRT alias-fill targets whose actual
    # trigger lives inside the out-of-shard W05_MQS_203P quest, per the
    # contract's Section D gap note) are deliberately left untouched -- no
    # regression vs. the current no-op, no fabricated behavior for an
    # unevidenced coupling.
    assert "EnableMarker" not in patch


def test_quest_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_quest_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Quests/{QUEST_BASE_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialoguesettlers_patch_count_matches_shard_contract():
    assert len(TOPICINFO_PATCH_CASES) == 12
