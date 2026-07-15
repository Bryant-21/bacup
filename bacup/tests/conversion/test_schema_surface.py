from __future__ import annotations

from bacup_lib.record.schema_surface import (
    SchemaRecordError,
    get_schema_surface,
)


def test_surface_knows_fo4_weap_native_keys() -> None:
    surface = get_schema_surface("fo4")
    keys = surface.allowed_keys("WEAP")
    assert "FULL" in keys
    assert "MODL" in keys
    assert "Data" in keys
    assert "Name" in keys
    assert "Model" not in keys


def test_surface_knows_fo4_misc_name_alias() -> None:
    surface = get_schema_surface("fo4")
    keys = surface.allowed_keys("MISC")
    assert "Name" in keys
    assert "FULL" in keys


def test_surface_knows_fo4_race_group_aliases() -> None:
    surface = get_schema_surface("fo4")
    keys = surface.allowed_keys("RACE")
    assert "BodyDatas" in keys
    assert "SkeletalDatas" not in keys


def test_surface_knows_fo4_condition_group_aliases() -> None:
    surface = get_schema_surface("fo4")
    keys = surface.allowed_keys("QUST")
    assert "QuestDialogueConditions" in keys
    assert "StoryManagerConditions" in keys


def test_orchestrator_uses_schema_surface_order_aliases() -> None:
    surface = get_schema_surface("fo4")

    assert "Name" in surface.ordered_keys("MISC")
    assert "BodyDatas" in surface.ordered_keys("RACE")
    assert "QuestDialogueConditions" in surface.ordered_keys("QUST")


def test_surface_orders_fields_by_schema_order_hint() -> None:
    surface = get_schema_surface("fo4")
    record = {
        "form_id": "000800",
        "eid": "B21_TestWeapon",
        "fields": [
            {"Data": {"DamageBase": 12}},
            {"FULL": "Test Weapon"},
            {"ObjectBounds": {}},
        ],
    }
    normalized, decisions = surface.normalize_record(record, "WEAP")
    labels = [next(iter(entry)) for entry in normalized["fields"]]
    assert labels.index("ObjectBounds") < labels.index("FULL") < labels.index("Data")
    assert decisions == []


def test_surface_reports_invalid_target_field() -> None:
    surface = get_schema_surface("fo4")
    record = {
        "form_id": "000800",
        "eid": "B21_TestWeapon",
        "fields": [{"FULL": "Test Weapon"}, {"Model": "bad.nif"}],
    }
    try:
        surface.normalize_record(record, "WEAP")
    except SchemaRecordError as exc:
        assert "WEAP.Model" in str(exc)
    else:
        raise AssertionError("Expected SchemaRecordError")


def test_surface_reports_non_dict_field_entry() -> None:
    surface = get_schema_surface("fo4")
    record = {
        "form_id": "000800",
        "eid": "B21_TestWeapon",
        "fields": [{"FULL": "Test Weapon"}, "bad"],
    }
    try:
        surface.normalize_record(record, "WEAP")
    except SchemaRecordError as exc:
        assert "WEAP" in str(exc)
        assert "fields[1]" in str(exc)
    else:
        raise AssertionError("Expected SchemaRecordError")


def test_surface_reports_multi_key_field_entry() -> None:
    surface = get_schema_surface("fo4")
    record = {
        "form_id": "000800",
        "eid": "B21_TestWeapon",
        "fields": [{"FULL": "Test Weapon", "Data": {"DamageBase": 12}}],
    }
    try:
        surface.normalize_record(record, "WEAP")
    except SchemaRecordError as exc:
        assert "WEAP" in str(exc)
        assert "fields[0]" in str(exc)
        assert "FULL" in str(exc)
        assert "Data" in str(exc)
    else:
        raise AssertionError("Expected SchemaRecordError")


def test_surface_normalizes_flat_record_fields() -> None:
    surface = get_schema_surface("fo4")
    normalized, decisions = surface.normalize_record(
        {
            "form_id": "000800",
            "eid": "B21_TestWeapon",
            "FULL": "Test Weapon",
            "Data": {"DamageBase": 12},
        },
        "WEAP",
    )
    labels = [next(iter(entry)) for entry in normalized["fields"]]
    assert "FULL" in labels
    assert "Data" in labels
    assert "FULL" not in normalized
    assert "Data" not in normalized
    assert normalized["form_id"] == "000800"
    assert normalized["eid"] == "B21_TestWeapon"
    assert decisions == []


def test_surface_fills_safe_required_struct_defaults() -> None:
    surface = get_schema_surface("fo4")
    completed, decisions = surface.complete_struct_defaults(
        "WEAP",
        "Data",
        {"DamageBase": 18, "AnimationType": 9},
    )
    assert completed["DamageBase"] == 18
    assert completed["AnimationType"] == 9
    assert "Speed" not in completed
    assert completed["UnknownByte31"] == 0
    assert any(d.reason == "schema_default" for d in decisions)


def test_surface_sanitizes_nested_struct_values_and_flags() -> None:
    surface = get_schema_surface("fo4")
    sanitized, decisions = surface.sanitize_record_values(
        {
            "AttackData": {
                "DamageMult": 1.0,
                "AttackFlags": ["PowerAttack", "Unknown6", "0x40"],
                "Unknown": 1.0,
            }
        },
        "RACE",
        drop_unknown_flags=True,
    )

    attack_data = sanitized["AttackData"]
    assert attack_data == {
        "DamageMult": 1.0,
        "AttackFlags": ["PowerAttack"],
    }
    assert {
        (decision.field, decision.reason)
        for decision in decisions
    } == {
        ("AttackData.AttackFlags", "invalid_target_flag_member"),
        ("AttackData.Unknown", "invalid_target_nested_field"),
    }


def test_surface_sanitizes_nested_object_template_include_rows() -> None:
    surface = get_schema_surface("fo4")
    sanitized, decisions = surface.sanitize_record_values(
        {
            "ObjectTemplates": [
                {
                    "ParentCombinationIndex": -1,
                    "Includes": [
                        {
                            "Mod": {
                                "reference": {
                                    "plugin": "B21_Test.esp",
                                    "object_id": "00089F",
                                }
                            },
                            "DontUseAll": True,
                        }
                    ],
                    "Name": "Scorched Snallygaster",
                    "Marker": True,
                }
            ]
        },
        "NPC_",
        drop_unknown_flags=True,
    )

    include = sanitized["ObjectTemplates"][0]["Includes"][0]
    assert include["Mod"] == {
        "reference": {"plugin": "B21_Test.esp", "object_id": "00089F"}
    }
    assert include["DontUseAll"] is True
    assert sanitized["ObjectTemplates"][0]["Name"] == "Scorched Snallygaster"
    assert decisions == []


def test_surface_masks_integer_unknown_flag_bits() -> None:
    surface = get_schema_surface("fo4")
    sanitized, decisions = surface.sanitize_record_values(
        {"AttackData": {"AttackFlags": 65}},
        "RACE",
        drop_unknown_flags=True,
    )

    assert sanitized["AttackData"]["AttackFlags"] == 1
    assert decisions[0].field == "AttackData.AttackFlags"
    assert decisions[0].reason == "invalid_target_flag_member"
    assert decisions[0].value == 64


