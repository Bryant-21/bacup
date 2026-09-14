from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source
from creation_lib.pex import parse_pex


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
FIXTURE_PATH = Path(__file__).with_name("fixtures") / (
    "cryptid_stories_client_server.json"
)
ACTIVE_CRYPTID_QUESTS = ("571ACC", "571ACB", "571ACA")
RE_CAMP_QUESTS = (
    "562281",
    "562BA5",
    "56385E",
    "569CC7",
    "571ACC",
    "571ACB",
    "571ACA",
    "586247",
    "586244",
    "586248",
    "586249",
    "58624A",
    "586246",
    "58CEA2",
    "58CEA3",
    "58CEA6",
    "58CEA7",
    "58CEA8",
    "58CEA9",
    "58CEB0",
    "58CEB5",
    "58CEB6",
    "58624B",
    "58CEB8",
    "58CEBB",
    "58CEBC",
    "58CEBD",
    "58CEBE",
    "58CEBF",
    "58D3B4",
    "5913EF",
    "59192E",
    "591931",
    "591935",
    "591937",
    "59193A",
    "5C5F59",
)


def _fixture() -> dict[str, object]:
    return json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))


def _functions(script_name: str) -> list[str]:
    pex = parse_pex(SOURCE_PEX_ROOT / f"{script_name.casefold()}.pex")
    return [
        function.name
        for script_object in pex.objects
        for state in script_object.states
        for function in state.functions
    ]


def test_cryptid_client_pex_files_are_exact_memberless_questinstance_shells():
    evidence = _fixture()["client_pex"]

    for script_name in ("RE_CryptidStories_FWM", "W05_RE_CryptidStories_Master"):
        expected = evidence[script_name]
        pex_path = SOURCE_PEX_ROOT / Path(expected["path"]).name
        assert pex_path.is_file()
        assert hashlib.sha256(pex_path.read_bytes()).hexdigest().upper() == expected[
            "sha256"
        ]

        pex = parse_pex(pex_path)
        assert len(pex.objects) == 1
        assert pex.objects[0].parent == expected["parent"] == "QuestInstance"
        assert _functions(script_name) == []
        assert pex.debug_info is not None
        assert pex.debug_info.functions == []
        assert expected["function_count"] == expected["debug_function_count"] == 0


def test_fwm_client_surface_has_no_master_quest_link_or_poll_entrypoint():
    evidence = _fixture()["client_pex"]["RE_CryptidStories_FWM"]
    pex = parse_pex(SOURCE_PEX_ROOT / "re_cryptidstories_fwm.pex")
    script = pex.objects[0]

    assert [prop.name for prop in script.properties] == evidence["properties"]
    assert evidence["variables"] == ["CryptidSpawnChanceDeterminant"]
    assert "MasterQuest" not in evidence["properties"]
    assert "ChosenCryptid" not in evidence["properties"]
    assert _functions("RE_CryptidStories_FWM") == []


def test_re_camp_node_selects_the_three_concrete_quests_without_the_master():
    evidence = _fixture()
    story_manager = evidence["story_manager"]

    assert story_manager["form_id"] == "5622DE"
    assert story_manager["flags"] == ["Random", "DoAllBeforeRepeating"]
    assert tuple(story_manager["quests"]) == RE_CAMP_QUESTS
    assert len(story_manager["quests"]) == 37
    assert all(form_id in story_manager["quests"] for form_id in ACTIVE_CRYPTID_QUESTS)
    assert evidence["master"]["form_id"] == "569CC5"
    assert evidence["master"]["form_id"] not in story_manager["quests"]


def test_master_is_only_catalogued_and_has_no_active_durable_patch():
    master = _fixture()["master"]

    assert master["editor_id"].endswith("_DoNotUse")
    assert master["references"] == [
        {
            "form_id": "5A1539",
            "editor_id": "RandomEncountersFormList",
            "record_type": "FLST",
        }
    ]
    assert _script_patch_source("W05_RE_CryptidStories_Master") is None
    assert _script_patch_source("RE_CryptidStories_FWM") is None


def test_concrete_quests_share_the_networked_counter_not_master_selection_avs():
    evidence = _fixture()
    concrete = evidence["concrete_quests"]

    assert set(concrete) == set(ACTIVE_CRYPTID_QUESTS)
    for binding in concrete.values():
        assert binding["counter_actor_value"] == "569D7A"
        assert binding["forest_location"] == "095004"
        assert binding["should_spawn_bound"] is False

    counter = evidence["counter_actor_value"]
    assert counter["editor_id"] == "W05_RE_CryptidStories_AV_Counter"
    assert counter["minimum"] == 0.0
    assert counter["maximum"] == 10.0
    assert counter["networking_flags"] == ["Unknown5"]


def test_encounter_wave_client_pex_exposes_no_local_spawn_service_body():
    evidence = _fixture()["client_pex"]["DefaultQuestEncounterWaveScript"]
    pex_path = SOURCE_PEX_ROOT / Path(evidence["path"]).name
    pex = parse_pex(pex_path)
    script = pex.objects[0]

    assert hashlib.sha256(pex_path.read_bytes()).hexdigest().upper() == evidence[
        "sha256"
    ]
    assert script.parent == evidence["parent"] == "EncounterWaveParentScript"
    assert [prop.name for prop in script.properties] == evidence["properties"]
    assert _functions("DefaultQuestEncounterWaveScript") == []
    assert pex.debug_info is not None
    assert pex.debug_info.functions == []
