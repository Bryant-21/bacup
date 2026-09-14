from __future__ import annotations

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

PATCH_CASES = (
    "TIF_NWOT_Strongbot_Dialogue_0066D0B9",
    "TIF_W05_MQR_Choice_0053635B",
    "TIF_W05_RE_Scene_JP04_A_00563824",
    "TIF_W05_RE_Scene_JP04_A_00563829",
    "TIF_W05_RE_Scene_JP04_B_0056D200",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E178",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E179",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E17A",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E17B",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E17C",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E17D",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061E17E",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061EDCF",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061EDD1",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061EE00",
    "TIF_ATX_COMP_Quest_Camp_Lite_0061F45F",
    "TIF_BS_RE_CampJN01_005C8E1B",
    "TIF_BS_RE_CampJN01_005C8E1E",
    "TIF_BS_RE_CampJN01_005C8E22",
    "TIF_BS_RE_CampJN01_005C8E29",
    "TIF_BS_RE_CampJN01_005C8E2C",
    "TIF_BS_RE_CampJN01_005C8E2E",
    "TIF_BS_RE_CampJN01_005C8E2F",
    "TIF_BS_RE_CampJN01_005C8E37",
    "TIF_BS_RE_CampJN01_005C8E38",
    "TIF_BS_RE_CampJN01_005C8E3B",
    "TIF_BS_RE_CampJN01_005C8E3D",
    "TIF_BS_RE_CampJN01_005C8E3F",
    "TIF_BS_RE_SceneJN01_005CFA45",
    "TIF_BS01_COMP_Quest_Camp_Lit_005E42F3",
    "TIF_BS01_Dialogue_Shin_00602D24",
    "TIF_BS01_Dialogue_Valdez_005C5C51",
    "TIF_COMP_Astronaut_Quest_Cam_0056F67E",
    "TIF_COMP_Astronaut_Quest_Cam_0056F69E",
    "TIF_COMP_Astronaut_Quest_Cam_0056F6BB",
    "TIF_COMP_Astronaut_Quest_Cam_00572CAD",
    "TIF_COMP_Astronaut_Quest_Cam_00572CAE",
    "TIF_COMP_Astronaut_Quest_Cam_00572CAF",
    "TIF_COMP_Astronaut_Quest_Cam_00572CB2",
    "TIF_COMP_Astronaut_Quest_Cam_00572CBB",
    "TIF_COMP_Astronaut_Quest_Cam_00572CC4",
    "TIF_COMP_Astronaut_Quest_Cam_00572CC6",
    "TIF_COMP_Astronaut_Quest_Cam_005740E9",
    "TIF_COMP_Astronaut_Quest_Cam_005740EA",
    "TIF_COMP_Astronaut_Quest_Cam_005740EE",
    "TIF_COMP_Quest_Camp_Full_Ast_0055F867",
    "TIF_COMP_Quest_Camp_Full_Ast_00572CAC",
    "TIF_COMP_Quest_Camp_Full_Ast_00572CB3",
    "TIF_COMP_Quest_Camp_Full_Ast_00572CB6",
    "TIF_COMP_Quest_Camp_Full_Ast_00572CB8",
    "TIF_COMP_Quest_Camp_Full_Ast_00572CC2",
    "TIF_COMP_Quest_Camp_Full_Ast_005740EB",
    "TIF_COMP_Quest_Camp_Full_Ast_005740EF",
    "TIF_COMP_Quest_Camp_Full_Ast_005740F0",
    "TIF_COMP_Quest_Camp_Full_Ast_005740F1",
    "TIF_COMP_Quest_Camp_Full_Ast_0058567A",
    "TIF_COMP_Quest_Camp_Full_Ast_005896F4",
    "TIF_COMP_Quest_Camp_Full_Ast_00589702",
    "TIF_COMP_Quest_Camp_Full_Ast_0058970E",
    "TIF_COMP_Quest_Camp_Full_Ast_00589710",
    "TIF_COMP_Quest_Camp_Full_Ast_00589712",
    "TIF_COMP_Quest_Camp_Full_Ast_00589713",
    "TIF_COMP_Quest_Camp_Full_Ast_00589714",
    "TIF_COMP_Quest_Camp_Full_Ast_00589715",
    "TIF_COMP_Quest_Camp_Full_Ast_0058EB0B",
    "TIF_COMP_Quest_Camp_Full_Ast_00595809",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA4B",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA4D",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA4E",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA4F",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA50",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA51",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA52",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FA74",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FADB",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FADC",
    "TIF_COMP_Quest_Camp_Full_Ast_0059FADD",
    "TIF_COMP_Quest_Intro_Full_As_0055F649",
    "TIF_COMP_Quest_Intro_Full_As_0055F66A",
    "TIF_COMP_Quest_Intro_Full_As_00591AAC",
    "TIF_HolotapeQuest_CT_002C60DC",
    "TIF_MILE_AmmoMerchant_Tier1_0079B194",
    "TIF_MILE_CaravanEscort_00769D25",
    "TIF_MILE_CaravanEscort_00769D30",
    "TIF_MILE_CaravanEscort_00769D32",
    "TIF_MILE_CaravanEscort_0079AECE",
    "TIF_MILE_CaravanEscort_0079AED0",
    "TIF_MOON_MiddleMountainPitst_0069F384",
    "TIF_RE_Scene_JP01_0055C56A",
    "TIF_RE_Scene_JP01_0055C56E",
    "TIF_RE_Scene_JP01_0055C573",
    "TIF_RE_Scene_JP01_0055C575",
    "TIF_SFS09_Habitat_00412059",
    "TIF_SFS09_Habitat_0041205A",
    "TIF_SFS09_Habitat_0041205B",
    "TIF_W05_Community_BB_Quest_0059F58F",
    "TIF_W05_RE_SceneAF03_0056A234",
    "TIF_W05_RE_SceneAF03_0056A237",
    "TIF_W05_RE_SceneAF03_00577C13",
    "TIF_XPD_Dialogue_RegularDebb_0063D35E",
    "TIF_XPD_Pitt02_Mission_0064E54E",
)

LIVE_FRAGMENT_MEMBERS = {
    'TIF_NWOT_Strongbot_Dialogue_0066D0B9': 'fragment_end',
    'TIF_W05_MQR_Choice_0053635B': 'fragment_end',
    'TIF_W05_RE_Scene_JP04_A_00563824': 'fragment_end',
    'TIF_W05_RE_Scene_JP04_A_00563829': 'fragment_end',
    'TIF_W05_RE_Scene_JP04_B_0056D200': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E178': 'fragment_begin',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E179': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E17A': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E17B': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E17C': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E17D': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061E17E': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061EDCF': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061EDD1': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061EE00': 'fragment_end',
    'TIF_ATX_COMP_Quest_Camp_Lite_0061F45F': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E1B': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E1E': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E22': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E29': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E2C': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E2E': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E2F': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E37': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E38': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E3B': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E3D': 'fragment_end',
    'TIF_BS_RE_CampJN01_005C8E3F': 'fragment_end',
    'TIF_BS_RE_SceneJN01_005CFA45': 'fragment_begin',
    'TIF_BS01_COMP_Quest_Camp_Lit_005E42F3': 'fragment_begin',
    'TIF_BS01_Dialogue_Shin_00602D24': 'fragment_begin',
    'TIF_BS01_Dialogue_Valdez_005C5C51': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_0056F67E': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_0056F69E': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_0056F6BB': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CAD': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CAE': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CAF': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CB2': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CBB': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CC4': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_00572CC6': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_005740E9': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_005740EA': 'fragment_end',
    'TIF_COMP_Astronaut_Quest_Cam_005740EE': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0055F867': 'fragment_begin',
    'TIF_COMP_Quest_Camp_Full_Ast_00572CAC': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00572CB3': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00572CB6': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00572CB8': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00572CC2': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_005740EB': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_005740EF': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_005740F0': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_005740F1': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0058567A': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_005896F4': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00589702': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0058970E': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00589710': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00589712': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00589713': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00589714': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_00589715': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0058EB0B': 'fragment_begin',
    'TIF_COMP_Quest_Camp_Full_Ast_00595809': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA4B': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA4D': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA4E': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA4F': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA50': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA51': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA52': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FA74': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FADB': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FADC': 'fragment_end',
    'TIF_COMP_Quest_Camp_Full_Ast_0059FADD': 'fragment_end',
    'TIF_COMP_Quest_Intro_Full_As_0055F649': 'fragment_end',
    'TIF_COMP_Quest_Intro_Full_As_0055F66A': 'fragment_end',
    'TIF_COMP_Quest_Intro_Full_As_00591AAC': 'fragment_end',
    'TIF_HolotapeQuest_CT_002C60DC': 'fragment_end',
    'TIF_MILE_AmmoMerchant_Tier1_0079B194': 'fragment_end',
    'TIF_MILE_CaravanEscort_00769D25': 'fragment_end',
    'TIF_MILE_CaravanEscort_00769D30': 'fragment_end',
    'TIF_MILE_CaravanEscort_00769D32': 'fragment_end',
    'TIF_MILE_CaravanEscort_0079AECE': 'fragment_end',
    'TIF_MILE_CaravanEscort_0079AED0': 'fragment_end',
    'TIF_MOON_MiddleMountainPitst_0069F384': 'fragment_end',
    'TIF_RE_Scene_JP01_0055C56A': 'fragment_begin',
    'TIF_RE_Scene_JP01_0055C56E': 'fragment_end',
    'TIF_RE_Scene_JP01_0055C573': 'fragment_end',
    'TIF_RE_Scene_JP01_0055C575': 'fragment_begin',
    'TIF_SFS09_Habitat_00412059': 'fragment_begin',
    'TIF_SFS09_Habitat_0041205A': 'fragment_begin',
    'TIF_SFS09_Habitat_0041205B': 'fragment_begin',
    'TIF_W05_Community_BB_Quest_0059F58F': 'fragment_begin',
    'TIF_W05_RE_SceneAF03_0056A234': 'fragment_begin',
    'TIF_W05_RE_SceneAF03_0056A237': 'fragment_begin',
    'TIF_W05_RE_SceneAF03_00577C13': 'fragment_begin',
    'TIF_XPD_Dialogue_RegularDebb_0063D35E': 'fragment_end',
    'TIF_XPD_Pitt02_Mission_0064E54E': 'fragment_end',
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


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_topicinfo_feature_completion_patch_merges_and_compiles(base_name: str):
    source_path = next(
        path
        for path in GENERATED_TOPICINFO_ROOT.glob("*.psc")
        if path.stem.casefold() == base_name.casefold()
    )
    source = source_path.read_text(encoding="utf-8-sig")
    patch = _script_patch_source(f"Fragments:TopicInfos:{base_name}")

    assert patch is not None
    assert not any(
        line.strip().casefold().startswith("scriptname ")
        for line in patch.splitlines()
    )
    patch_members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert patch_members == [
        ("function", LIVE_FRAGMENT_MEMBERS[base_name])
    ]

    merged = _merge_script_method_patches(source, patch)
    for _kind, member_name in patch_members:
        assert merged.casefold().count(f"function {member_name}(".casefold()) == 1

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


def test_topicinfo_feature_completion_patch_count():
    assert len(PATCH_CASES) == 101


def test_topicinfo_corrected_semantic_order_and_ownership():
    beer = _script_patch_source(
        "Fragments:TopicInfos:TIF_MOON_MiddleMountainPitst_0069F384"
    )
    assert beer is not None
    assert "GetItemCount(Potion_Beer) > 0" in beer
    assert beer.index("EquipItem(Potion_Beer") < beer.index("Message_ENDMid.Show()")

    pointer = _script_patch_source(
        "Fragments:TopicInfos:TIF_NWOT_Strongbot_Dialogue_0066D0B9"
    )
    assert pointer is not None
    assert "NukacadePointerQuest.IsRunning()" in pointer
    assert "NukacadePointerQuest.IsCompleted()" in pointer
    assert pointer.index("NukacadePointerQuest.IsCompleted()") < pointer.index(
        "PQStartKeyword.SendStoryEvent"
    )

    for script_name, actor_value in (
        ("TIF_BS01_Dialogue_Shin_00602D24", "BS02_AV_FlirtedWithShin"),
        ("TIF_BS01_Dialogue_Valdez_005C5C51", "BoSz01_PlayerSpokeToValdez"),
    ):
        source = _script_patch_source(f"Fragments:TopicInfos:{script_name}")
        assert source is not None
        assert f"Game.GetPlayer().SetValue({actor_value}" in source
        assert f"akSpeakerRef.SetValue({actor_value}" not in source
