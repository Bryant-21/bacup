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

# Fragment reward/consequence patches for the w3b-re-dtr contract family (TIF_
# TopicInfo fragments that grant/consume items or hand off scene control on a
# single Fragment_Begin/Fragment_End call). Merged from the former
# w3b_re_dtr_a/b/c shards, which each covered a disjoint sub-family (family
# tags below trace each row back to its origin batch -- kept only because the
# none-guard strictness rules in _FAMILY_ACTION_INDICATORS /
# _FAMILY_STRICT_GUARD were adjudicated slightly differently per batch).

# -- family a (5 rows) -- former w3b_re_dtr_a: Assault family.
_FAMILY_A_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:TopicInfos:TIF_W05_RE_AssaultAF01_0055DEAA": ("Game.GetPlayer().AddItem(Caps001, 1)",),
    "Fragments:TopicInfos:TIF_W05_RE_AssaultAF01_0055DEAB": ("Game.GetPlayer().AddItem(Caps001, 1)",),
    "Fragments:TopicInfos:TIF_W05_RE_AssaultBB02_0056F078": ("CurrentSpeakerScene.ForceRefTo(akSpeakerRef)",),
    "Fragments:TopicInfos:TIF_W05_RE_AssaultBB02_00573F0D": ("CurrentSpeakerScene.ForceRefTo(akSpeakerRef)",),
    "Fragments:TopicInfos:TIF_W05_RE_AssaultBB02_00573F0E": ("CurrentSpeakerScene.ForceRefTo(akSpeakerRef)",),
}
_FAMILY_A_MEMBERS: dict[str, set[str]] = {
    "Fragments:TopicInfos:TIF_W05_RE_AssaultAF01_0055DEAA": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_AssaultAF01_0055DEAB": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_AssaultBB02_0056F078": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_AssaultBB02_00573F0D": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_AssaultBB02_00573F0E": {"fragment_begin"},
}

# -- family b (23 rows) -- former w3b_re_dtr_b: Camp/Object/Scene_JP01-02 family.
_FAMILY_B_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:TopicInfos:TIF_W05_RE_CampAF02_00567A9F": (
        "Game.GetPlayer().RemoveItem(Stimpak, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A224": (
        "Game.GetPlayer().RemoveItem(c_FiberOptics_scrap, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A239": (
        "Game.GetPlayer().RemoveItem(c_Circuitry_scrap, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A242": (
        "Game.GetPlayer().RemoveItem(AmmoFusionCore, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A230": (
        "Alias_PlayerHelper.ForceRefTo(akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F811": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F815": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F81A": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F81E": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F823": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F837": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F838": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_00573F0B": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_00573F13": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C56C": (
        "Game.GetPlayer().AddItem(ScrapRef1, 1)",
        "Game.GetPlayer().AddItem(WoodRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C572": (
        "Game.GetPlayer().AddItem(ScrapRef1, 1)",
        "Game.GetPlayer().AddItem(WoodRef, 1)",
        "Game.GetPlayer().AddItem(ScrapRef2, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C574": (
        "Game.GetPlayer().AddItem(ScrapRef1, 1)",
        "Game.GetPlayer().AddItem(WoodRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C576": (
        "Game.GetPlayer().AddItem(ScrapRef1, 1)",
        "Game.GetPlayer().AddItem(WoodRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_00575178": (
        "Game.GetPlayer().AddItem(FoodRef, 1)",
        "Game.GetPlayer().AddItem(WaterRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_CampAF01_00563870_1": (
        "Game.GetPlayer().AddItem(DayTripper, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_ObjectBB01_00568E60": (
        "Game.GetPlayer().RemoveItem(ToyAlien, 1, true, akSpeakerRef)",
        "akSpeakerRef.RemoveItem(ToyForPlayer, 1, true, Game.GetPlayer())",
        "PlayerAlias.ForceRefTo(Game.GetPlayer())",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F83F": (
        "Game.GetPlayer().RemoveItem(CapsRef, 25, true, akSpeakerRef)",
        "Game.GetPlayer().AddItem(BrahminMeatRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_00573F0C": (
        "Game.GetPlayer().AddItem(BrahminMeatRef, 1)",
    ),
}
_FAMILY_B_MEMBERS: dict[str, set[str]] = {
    "Fragments:TopicInfos:TIF_W05_RE_CampAF02_00567A9F": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A224": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A239": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A242": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_ObjectAF01_0056A230": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F811": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F815": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F81A": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F81E": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F823": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F837": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F838": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_00573F0B": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_00573F13": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C56C": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C572": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C574": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_0055C576": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP01_00575178": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_CampAF01_00563870_1": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_ObjectBB01_00568E60": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F83F": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_00573F0C": {"fragment_end"},
}

# -- family c (25 rows) -- former w3b_re_dtr_c: Scene_JP04/SceneAF/Template/Travel family.
_FAMILY_C_CASES: dict[str, tuple[str, ...]] = {
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563818": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056381B": (
        "Game.GetPlayer().RemoveItem(CapsRef, 15, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056381C": (
        "PlayerRef.ForceRefTo(Game.GetPlayer())",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563827_1": (
        "Game.GetPlayer().AddItem(RewardRef1, 1)",
        "Game.GetPlayer().AddItem(RewardRef2, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056382E": (
        "PlayerRef.ForceRefTo(Game.GetPlayer())",
        "raider2Actor.StartCombat(targetActor)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056382F": (
        "Game.GetPlayer().AddItem(RewardRef, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563831": (
        "PlayerRef.ForceRefTo(Game.GetPlayer())",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563832_1": (
        "Game.GetPlayer().RemoveItem(CapsRef, 20, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563838": (
        "Game.GetPlayer().RemoveItem(CapsRef, 5, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563839": (
        "PlayerRef.ForceRefTo(Game.GetPlayer())",
        "raider1Actor.StartCombat(targetActor)",
        "raider2Actor.StartCombat(targetActor)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_B_0056D1FE": (
        "Game.GetPlayer().RemoveItem(CapsRef, 30, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_B_0056D20C": (
        "Game.GetPlayer().RemoveItem(CapsRef, 10, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_B_0056D218": (
        "Game.GetPlayer().RemoveItem(CapsRef, 25, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF03_0056A23C": (
        "akSpeakerRef.AddToFaction(REPlayerEnemy)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF03_0056A245": (
        "Game.GetPlayer().AddItem(RadX, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF03_005849FF": (
        "Game.GetPlayer().AddItem(RadX, 1)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_005849FC": (
        "Game.GetPlayer().RemoveItem(Ammo44, 10, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_005849FD": (
        "Game.GetPlayer().RemoveItem(Ammo308Caliber, 10, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_00584A00": (
        "Game.GetPlayer().RemoveItem(Ammo556, 10, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_00584A04": (
        "Game.GetPlayer().RemoveItem(Ammo38Caliber, 10, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_00584A05": (
        "Game.GetPlayer().RemoveItem(Ammo10mm, 10, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneTemplate01_0055DEA4": (
        "Game.GetPlayer().RemoveItem(WaterBoiled, 1, true, akSpeakerRef)",
        "Game.GetPlayer().RemoveItem(WaterPurified, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_SceneTemplate01_0055DEAD": (
        "Game.GetPlayer().RemoveItem(WaterDirty, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_TravelAF02_0056A22E": (
        "Game.GetPlayer().RemoveItem(RadX, 1, true, akSpeakerRef)",
    ),
    "Fragments:TopicInfos:TIF_W05_RE_TravelAF02_0056A23F": (
        "Game.GetPlayer().RemoveItem(RadAway, 1, true, akSpeakerRef)",
    ),
}
_FAMILY_C_MEMBERS: dict[str, set[str]] = {
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563818": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056381B": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056381C": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563827_1": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056382E": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_0056382F": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563831": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563832_1": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563838": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_A_00563839": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_B_0056D1FE": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_B_0056D20C": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP04_B_0056D218": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF03_0056A23C": {"fragment_begin"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF03_0056A245": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF03_005849FF": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_005849FC": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_005849FD": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_00584A00": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_00584A04": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneAF04_00584A05": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneTemplate01_0055DEA4": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_SceneTemplate01_0055DEAD": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_TravelAF02_0056A22E": {"fragment_end"},
    "Fragments:TopicInfos:TIF_W05_RE_TravelAF02_0056A23F": {"fragment_end"},
}

TIF_PATCH_CASES: dict[str, tuple[str, ...]] = {
    **_FAMILY_A_CASES,
    **_FAMILY_B_CASES,
    **_FAMILY_C_CASES,
}
EXPECTED_MEMBERS: dict[str, set[str]] = {
    **_FAMILY_A_MEMBERS,
    **_FAMILY_B_MEMBERS,
    **_FAMILY_C_MEMBERS,
}
SCRIPT_FAMILY: dict[str, str] = {
    **{name: "a" for name in _FAMILY_A_CASES},
    **{name: "b" for name in _FAMILY_B_CASES},
    **{name: "c" for name in _FAMILY_C_CASES},
}

# Per-family none-guard rules preserved verbatim from the original shards:
# family a/b require one "!= None" guard per detected action line; family c
# (a later, looser adjudication) only requires at least one guard per patch.
# The indicator substrings also differ per family -- b adds the
# akSpeakerRef.RemoveItem( idiom, c drops it but adds AddToFaction(/StartCombat(.
FAMILY_ACTION_INDICATORS: dict[str, tuple[str, ...]] = {
    "a": ("Game.GetPlayer().", ".ForceRefTo("),
    "b": ("Game.GetPlayer().", ".ForceRefTo(", "akSpeakerRef.RemoveItem("),
    "c": ("Game.GetPlayer().", ".ForceRefTo(", ".AddToFaction(", ".StartCombat("),
}
FAMILY_STRICT_GUARD: dict[str, bool] = {"a": True, "b": True, "c": False}


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_calls"), TIF_PATCH_CASES.items())
def test_dtr_patch_restores_confirmed_behavior(
    script_name: str, expected_calls: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert _member_names(patch) == EXPECTED_MEMBERS[script_name]
    for call in expected_calls:
        assert call in patch


@pytest.mark.parametrize("script_name", TIF_PATCH_CASES)
def test_dtr_actions_are_none_guarded(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None

    family = SCRIPT_FAMILY[script_name]
    indicators = FAMILY_ACTION_INDICATORS[family]
    action_lines = [
        line
        for line in patch.splitlines()
        if any(indicator in line for indicator in indicators)
    ]
    assert action_lines
    guard_count = patch.count("!= None")
    if FAMILY_STRICT_GUARD[family]:
        assert guard_count >= len(action_lines)
    else:
        assert guard_count >= 1


def test_scenetemplate01_water_branch_mirrors_ctda_order():
    # 0055DEA4's own record lists the WaterBoiled CTDA before the WaterPurified
    # CTDA; the patch's If/ElseIf order must match so a player holding both
    # gets the same first-match-wins outcome the record's condition order implies.
    patch = _script_patch_source(
        "Fragments:TopicInfos:TIF_W05_RE_SceneTemplate01_0055DEA4"
    )
    assert patch is not None
    boiled_pos = patch.index("WaterBoiled")
    purified_pos = patch.index("WaterPurified")
    assert boiled_pos < purified_pos


@pytest.mark.parametrize("script_name", TIF_PATCH_CASES)
def test_dtr_patch_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_dtr_patch_count_matches_confirmed_batches():
    assert len(_FAMILY_A_CASES) == 5
    assert len(_FAMILY_B_CASES) == 23
    assert len(_FAMILY_C_CASES) == 25
    assert len(TIF_PATCH_CASES) == 53
