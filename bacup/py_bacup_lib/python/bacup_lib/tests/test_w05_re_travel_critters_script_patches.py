from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SOURCE_ESM = Path("N:/Steam Games/steamapps/common/Fallout76/Data/SeventySix.esm")
LIVE_ESM = REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm"
DEATH_CASES = (
    "W05_RE_TravelBB02_ChickenScript",
    "W05_RE_TravelBB03_SquirrelScript",
)
UNPATCHED_CASES = (
    "W05_RE_TravelBB02RunnerScript",
    "Fragments:TopicInfos:TIF_W05_RE_TravelAF01_00567AA7",
)
RUNNER_RECORD_IDS = (
    "56F034",
    "587BC6",
    "56F474",
    "56F475",
    "56F479",
    "56F47A",
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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def _read_records(plugin: Path) -> dict[str, dict]:
    result = subprocess.run(
        [
            "modkit.exe",
            "esp",
            "get-records",
            "--authoring",
            str(plugin),
            *RUNNER_RECORD_IDS,
        ],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    payload = json.loads(result.stdout)
    assert payload["found"] == len(RUNNER_RECORD_IDS)
    return {record["form_id"].upper(): record for record in payload["records"]}


def _field(record: dict, field_name: str):
    for entry in record["fields"]:
        if field_name in entry:
            return entry[field_name]
    return None


def _alias_fields(quest: dict, alias_id: int) -> list[dict]:
    collecting = False
    result: list[dict] = []
    for field in quest["fields"]:
        if "ALST" in field:
            collecting = field["ALST"] == alias_id
        if collecting:
            result.append(field)
            if "ALED" in field:
                return result
    raise AssertionError(f"alias {alias_id} is missing")


def _alias_value(fields: list[dict], name: str):
    for field in fields:
        if name in field:
            return field[name]
    return None


def _ref(object_id: str) -> dict:
    return {"reference": {"plugin": "SeventySix.esm", "object_id": object_id}}


def _scene_package(scene: dict) -> dict | None:
    package_action = False
    for field in scene["fields"]:
        if field.get("Type") == "Package":
            package_action = True
            continue
        if package_action and "AnimArchType" in field:
            return field["AnimArchType"]
    return None


@pytest.mark.skipif(
    shutil.which("modkit.exe") is None
    or not SOURCE_ESM.is_file()
    or not LIVE_ESM.is_file(),
    reason="source and live SeventySix masters are required",
)
def test_source_and_live_runner_records_own_the_complete_package_transition():
    for records in (_read_records(SOURCE_ESM), _read_records(LIVE_ESM)):
        quest = records["56F034"]
        vmad = _field(quest, "VirtualMachineAdapter")
        fragment_aliases = vmad["Script Fragments"]["Aliases"]
        runner_vmad = next(
            entry for entry in fragment_aliases if entry["Object"]["Alias"] == 21
        )
        runner_script = next(
            script
            for script in runner_vmad["Alias Scripts"]
            if script["ScriptName"] == "W05_RE_TravelBB02RunnerScript"
        )
        assert runner_script["Properties"] == [
            {
                "propertyName": "W05_RE_TravelBB02_Flee",
                "Type": "Object",
                "Flags": 1,
                "Value": {"Alias": -1, "FormID": _ref("587BC6")},
            },
            {
                "propertyName": "DefaultStayAtSelfSceneIgnoreCombat",
                "Type": "Object",
                "Flags": 1,
                "Value": {"Alias": -1, "FormID": _ref("56D236")},
            },
        ]

        runner_alias = _alias_fields(quest, 21)
        assert _alias_value(runner_alias, "ALID") == "Runner"
        assert _alias_value(runner_alias, "Package") == _ref("587BC6")
        linked_aliases = _alias_value(runner_alias, "LinkedAliases")
        assert len(linked_aliases) == 1
        assert linked_aliases[0]["LinkedAliasesAlias"] == 5
        assert (
            linked_aliases[0]["LinkedAliasesKeyword"]["reference"]["object_id"]
            == "02FD66"
        )

        chicken_alias = _alias_fields(quest, 23)
        assert _alias_value(chicken_alias, "ALID") == "Chicken1"
        assert _alias_value(chicken_alias, "Package") == _ref("56F474")

        flee = records["587BC6"]
        assert _field(flee, "OwnerQuest") == _ref("56F034")
        assert {
            "MustComplete",
            "IgnoreCombat",
            "NoCombatAlert",
        }.issubset(_field(flee, "PackData")["GeneralFlags"])
        flee_target = _field(flee, "PackageDatas")[0]["Target"]
        assert flee_target == {
            "TargetDataType": "LinkedReference",
            "TargetDataTarget": {"variant": "keyword", "value": 195942},
        }

        chase = records["56F474"]
        assert _field(chase, "OwnerQuest") == _ref("56F034")
        assert {"IgnoreCombat", "NoCombatAlert"}.issubset(
            _field(chase, "PackData")["GeneralFlags"]
        )
        chase_target = _field(chase, "PackageDatas")[0]["Target"]
        assert chase_target == {
            "TargetDataType": "RefAlias",
            "TargetDataTarget": {"variant": "alias", "value": 21},
        }

        runner_scene = records["56F475"]
        assert "BeginOnQuestStart" in _field(runner_scene, "FNAM")
        assert any(field.get("Type") == "Dialogue" for field in runner_scene["fields"])
        assert any(field.get("ALID") == 21 for field in runner_scene["fields"])
        assert any(
            field.get("Scene") == _ref("56F475") for field in runner_scene["fields"]
        )

        assert _scene_package(records["56F479"]) == _ref("56D236")
        assert _scene_package(records["56F47A"]) == _ref("56D236")


def test_runner_declaration_only_carrier_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    script_name = "W05_RE_TravelBB02RunnerScript"
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    source = source_path.read_text(encoding="utf-8")
    members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }
    assert members == set()
    assert _script_patch_source(script_name) is None

    result = compile_psc(
        source,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", DEATH_CASES)
def test_w05_travel_critter_patch_is_one_player_guarded_death_member(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "Scriptname " not in patch

    members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert members == {"ondeath"}
    assert "akKiller == None || PlayerAlias == None" in patch
    assert "kPlayer != None && akKiller == kPlayer && kQuest != None" in patch
    assert "kQuest.SetStage(100)" in patch


@pytest.mark.parametrize("script_name", DEATH_CASES)
def test_w05_travel_critter_patch_merges_once_and_native_compiles_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    assert merged.lower().count("event ondeath(") == 1

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


@pytest.mark.parametrize("script_name", UNPATCHED_CASES)
def test_w05_travel_native_carriers_have_no_invented_patch(
    script_name: str,
):
    assert _script_patch_source(script_name) is None
