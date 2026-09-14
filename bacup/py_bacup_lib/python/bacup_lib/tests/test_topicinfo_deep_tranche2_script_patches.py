from __future__ import annotations

import csv
import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "topicinfos"
)
LIVE_BINDING_LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "topicinfo-fragment-gap-ledger-2026-08-11.csv"
)

PATCH_MEMBERS = {
    "TIF_COMP_Quest_Camp_Full_A_00572CC8_1": "fragment_end",
    "TIF_Dialogue_MTNL01_Protectr_004E5FF4": "fragment_begin",
    "TIF_E09D_MostWanted_Gunthe_0066F34F_1": "fragment_begin",
    "TIF_MTNM04_Guest_00585578": "fragment_begin",
    "TIF_NWOT_Betty_Dialogue_0066F9CA": "fragment_end",
    "TIF_NWOT_Betty_Dialogue_0066F9CB": "fragment_end",
    "TIF_NWOT_Del_Dialogue_0066F98A": "fragment_begin",
    "TIF_NWOT_FortuneTeller_Dialo_0066D0B1": "fragment_end",
    "TIF_NWOT_Strongbot_Dialogue_0066D10A": "fragment_end",
    "TIF_RE_TravelCT01_004E10A5": "fragment_end",
    "TIF_RE_TravelCT01_004E10A6": "fragment_end",
    "TIF_RE_TravelCT01_004E10A7": "fragment_end",
    "TIF_Storm_Alyssa_Dialogue_00783A6F": "fragment_end",
    "TIF_Storm_Craig_dialogue_00760006": "fragment_end",
    "TIF_W05_Community_RaiderFish_0057CEF2": "fragment_begin",
    "TIF_W05_Community_RaiderFish_0057CEF8": "fragment_begin",
    "TIF_W05_Community_RaiderFish_0057CF15": "fragment_begin",
    "TIF_W05_Community_RaiderFish_0057CF16": "fragment_begin",
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


@pytest.mark.parametrize("base_name,member_name", PATCH_MEMBERS.items())
def test_topicinfo_deep_tranche2_exact_member_merges_and_compiles(
    base_name: str, member_name: str
):
    source_path = next(
        path
        for path in GENERATED_TOPICINFO_ROOT.glob("*.psc")
        if path.stem.casefold() == base_name.casefold()
    )
    source = source_path.read_text(encoding="utf-8-sig")
    patch = _script_patch_source(f"Fragments:TopicInfos:{base_name}")

    assert patch is not None
    assert [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ] == [("function", member_name)]

    merged = _merge_script_method_patches(source, patch)
    assert merged.casefold().count(f"function {member_name}(") == 1

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_topicinfo_deep_tranche2_semantics():
    player_state_scripts = {
        "TIF_COMP_Quest_Camp_Full_A_00572CC8_1": "ModValue(AV_FlirtCount, 1.0)",
        "TIF_MTNM04_Guest_00585578": "SetValue(pMTNM04_InterviewedAV, 1.0)",
        "TIF_NWOT_Betty_Dialogue_0066F9CA": "SetValue(HasSpokenToAV, 1.0)",
        "TIF_NWOT_Betty_Dialogue_0066F9CB": "SetValue(HasSpokenToAV, 1.0)",
        "TIF_NWOT_Del_Dialogue_0066F98A": "SetValue(KnowsAboutSideBusinessAV, 1.0)",
        "TIF_NWOT_FortuneTeller_Dialo_0066D0B1": "SetValue(pNWOT_FortuneTeller_AboutOtherPeopleAV, 1.0)",
        "TIF_NWOT_Strongbot_Dialogue_0066D10A": "SetValue(pNWOT_Strongbot_AboutTestStrength_AV, 1.0)",
        "TIF_Storm_Alyssa_Dialogue_00783A6F": "SetValue(AlyssaAV, 1.0)",
        "TIF_Storm_Craig_dialogue_00760006": "SetValue(AV_MQ02_PE, 1.0)",
    }
    for script_name, effect in player_state_scripts.items():
        patch = _script_patch_source(f"Fragments:TopicInfos:{script_name}")
        assert patch is not None
        assert f"Game.GetPlayer().{effect}" in patch
        assert f"akSpeakerRef.{effect}" not in patch

    alias_patch = _script_patch_source(
        "Fragments:TopicInfos:TIF_Dialogue_MTNL01_Protectr_004E5FF4"
    )
    assert alias_patch is not None
    assert "Alias_ScenePlayer.ForceRefTo(Game.GetPlayer())" in alias_patch

    reward_patch = _script_patch_source(
        "Fragments:TopicInfos:TIF_E09D_MostWanted_Gunthe_0066F34F_1"
    )
    assert reward_patch is not None
    assert "Game.GetPlayer().AddItem(LuckGiveObject, 1, False)" in reward_patch

    for script_name in (
        "TIF_RE_TravelCT01_004E10A5",
        "TIF_RE_TravelCT01_004E10A6",
        "TIF_RE_TravelCT01_004E10A7",
    ):
        patch = _script_patch_source(f"Fragments:TopicInfos:{script_name}")
        assert patch is not None
        assert "Game.GetPlayer().AddItem(RE_TravelCT01_Note, 1, False)" in patch

    fish_items = {
        "TIF_W05_Community_RaiderFish_0057CEF2": "MirelurkMeatRef",
        "TIF_W05_Community_RaiderFish_0057CEF8": "MirelurkEggRef",
        "TIF_W05_Community_RaiderFish_0057CF15": "MirelurkQueenMeatRef",
        "TIF_W05_Community_RaiderFish_0057CF16": "MirelurkSoftshellMeatRef",
    }
    for script_name, item_name in fish_items.items():
        patch = _script_patch_source(f"Fragments:TopicInfos:{script_name}")
        assert patch is not None
        assert (
            f"Game.GetPlayer().RemoveItem({item_name}, 1, True, akSpeakerRef)"
            in patch
        )


def test_topicinfo_deep_tranche2_patch_count():
    assert len(PATCH_MEMBERS) == 18


def test_topicinfo_deep_tranche2_manifest_matches_live_vmad_ledger():
    with LIVE_BINDING_LEDGER.open(encoding="utf-8", newline="") as stream:
        tranche_rows = {
            row["script"]: row
            for row in csv.DictReader(stream)
            if row["disposition"] == "patched-deterministic-tranche2"
        }

    assert tranche_rows.keys() == PATCH_MEMBERS.keys()
    for script_name, member_name in PATCH_MEMBERS.items():
        row = tranche_rows[script_name]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == member_name
        assert row["patch_members"].casefold() == member_name
