from pathlib import Path

import yaml

from tools.update_conversion_whitelists import update_game


def test_update_game_regenerates_yaml_whitelist_and_preserves_overrides(tmp_path):
    data_dir = tmp_path / "data"
    whitelist_dir = tmp_path / "whitelists"
    record_dir = data_dir / "fo4_esm_yaml" / "Fallout4" / "records" / "RACE"
    record_dir.mkdir(parents=True)
    (record_dir / "CreatureRace - 000001_Fallout4.esm.yaml").write_text(
        "\n".join(
            [
                "form_id: 000001",
                "eid: CreatureRace",
                "fields:",
                "- FULL: Creature",
                "- Properties:",
                "  - PropertiesActorValue:",
                "      reference:",
                "        plugin: Fallout4.esm",
                "        object_id: 0002D4",
                "    PropertiesValue: 50.0",
                "- BehaviourGraph: Actors\\Creature\\Behavior.hkx",
            ]
        ),
        encoding="utf-8",
    )
    nested_record_dir = (
        data_dir
        / "fo4_esm_yaml"
        / "Fallout4"
        / "records"
        / "CELL"
        / "0"
        / "1"
        / "Interior - 000002_Fallout4.esm"
    )
    nested_record_dir.mkdir(parents=True)
    (nested_record_dir / "RecordData.yaml").write_text(
        "\n".join(
            [
                "form_id: 000002",
                "eid: Interior",
                "fields:",
                "- EditorID: Interior",
                "- Lighting:",
                "    Ambient: 1",
            ]
        ),
        encoding="utf-8",
    )
    whitelist_dir.mkdir()
    (whitelist_dir / "fo4.yaml").write_text(
        yaml.safe_dump(
            {
                "game": "fo4",
                "record_types": {"OLD": ["OldField"]},
                "nested": {},
                "canonical_order": {},
                "overrides": {"add": {"RACE": ["ManualField"]}, "drop": {}},
                "notes": ["keep me"],
            },
            sort_keys=False,
        ),
        encoding="utf-8",
    )

    changed = update_game("fo4", data_dir=data_dir, whitelist_dir=whitelist_dir, scanner="python")

    assert changed is True
    out = yaml.safe_load((whitelist_dir / "fo4.yaml").read_text(encoding="utf-8"))
    assert out["record_types"]["RACE"] == ["BehaviourGraph", "FULL", "Properties"]
    assert out["record_types"]["CELL"] == ["EditorID", "Lighting"]
    assert out["nested"]["RACE"]["Properties"] == [
        "PropertiesActorValue",
        "PropertiesValue",
    ]
    assert out["canonical_order"]["RACE"] == [
        "FULL",
        "Properties",
        "BehaviourGraph",
    ]
    assert out["overrides"] == {"add": {"RACE": ["ManualField"]}, "drop": {}}
    assert out["notes"] == ["keep me"]

    assert (
        update_game(
            "fo4",
            data_dir=data_dir,
            whitelist_dir=whitelist_dir,
            check=True,
            scanner="python",
        )
        is False
    )
