from types import SimpleNamespace

import pytest

from bacup_lib.native_runtime import _ConversionNativeProxy, _weapon_metadata_from_raw


LEGACY_ROW = (
    "11A8E4@FalloutNV.esm",
    "Hatchet",
    r"Weapons\Hatchet\Hatchet.nif",
    "",
    "",
    "",
    "melee",
    "no_ammo_field",
    "1",
)


def test_legacy_weapon_metadata_row_keeps_exact_dict_contract() -> None:
    assert _weapon_metadata_from_raw(LEGACY_ROW) == {
        "source_form_key": "11A8E4@FalloutNV.esm",
        "editor_id": "Hatchet",
        "base_model": r"Weapons\Hatchet\Hatchet.nif",
        "model_mod1": "",
        "model_mod2": "",
        "model_mod3": "",
        "weapon_role": "melee",
        "ammo_decision": "no_ammo_field",
        "anim_type": "1",
    }


def test_bulk_melee_weapon_metadata_decodes_optional_receipt_tail() -> None:
    raw = SimpleNamespace(
        conversion_run_weapon_metadata=lambda _run_id, _form_keys: [
            LEGACY_ROW
            + (
                "admitted",
                "bulk_melee_v1",
                "machete_one_hand",
                "fnv",
                r"Weapons\Hatchet\Hatchet.nif",
                r"Weapons\Hatchet\1stPersonHatchet.nif",
                "wnam_stat",
                ["enchantment", "legacy_weapon_mods"],
                "",
            )
        ]
    )

    row = _ConversionNativeProxy(raw).conversion_run_weapon_metadata(7, [])[0]

    assert row["source_form_key"] == LEGACY_ROW[0]
    assert row["base_model"] == LEGACY_ROW[2]
    assert row["status"] == row["disposition"] == "admitted"
    assert row["policy"] == "bulk_melee_v1"
    assert row["target_profile"] == "machete_one_hand"
    assert row["source_family"] == row["provenance"] == "fnv"
    assert row["world_model"] == r"Weapons\Hatchet\Hatchet.nif"
    assert row["first_person_model"] == r"Weapons\Hatchet\1stPersonHatchet.nif"
    assert row["first_person_resolution"] == "wnam_stat"
    assert row["lowering_drops"] == ["enchantment", "legacy_weapon_mods"]
    assert row["rejection_reason"] == ""
    assert row["reason_code"] == "admitted"


def test_bulk_melee_rejection_exposes_stable_reason_code() -> None:
    row = _weapon_metadata_from_raw(
        LEGACY_ROW[:6]
        + ("gun", "unknown", "3")
        + (
            "rejected",
            "bulk_melee_v1",
            "",
            "fnv",
            "",
            "",
            "none",
            [],
            "ranged_animation",
        )
    )

    assert row["weapon_role"] == "gun"
    assert row["status"] == "rejected"
    assert row["rejection_reason"] == "ranged_animation"
    assert row["reason_code"] == "ranged_animation"


def test_weapon_metadata_rejects_unknown_raw_tuple_lengths() -> None:
    with pytest.raises(ValueError, match="9 or 18"):
        _weapon_metadata_from_raw(("short",))
