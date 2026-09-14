from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import _augment_fo76_to_fo4_script_skeleton


SCRIPT_NAME = "WeaponTestingRangeSpellScript"
SKELETON = """Scriptname WeaponTestingRangeSpellScript Extends ActiveMagicEffect

actor selfRef

actorvalue Property HealthAV Auto
actorvalue Property HealthPercentage Auto
"""
PATCH_PATH = (
    Path(__file__).resolve().parents[1]
    / "script_patches"
    / "WeaponTestingRangeSpellScript.psc"
)


def test_weapon_testing_range_restores_bound_stimpak_declaration() -> None:
    augmented = _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, SKELETON)

    assert augmented.count("Potion Property Stimpak Auto Mandatory") == 1
    assert "actorvalue Property HealthAV Auto" in augmented
    assert "actorvalue Property HealthPercentage Auto" in augmented


def test_weapon_testing_range_declaration_augmentation_is_idempotent() -> None:
    augmented = _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, SKELETON)

    assert _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, augmented) == augmented
    assert (
        _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME.swapcase(), SKELETON)
        == augmented
    )


def test_unrelated_script_does_not_receive_stimpak_declaration() -> None:
    unrelated = SKELETON.replace(SCRIPT_NAME, "UnrelatedScript")

    assert (
        _augment_fo76_to_fo4_script_skeleton("UnrelatedScript", unrelated) == unrelated
    )


def test_weapon_testing_range_augmentation_does_not_invent_runtime_behavior() -> None:
    augmented = _augment_fo76_to_fo4_script_skeleton(SCRIPT_NAME, SKELETON)

    assert "actorvalue Property HealthPercentage Auto" in augmented
    assert "Float Property HealthPercentage" not in augmented
    assert "Event OnEffectStart(" not in augmented
    assert "Event OnHit(" not in augmented
    assert "Event OnTimer(" not in augmented
    assert "EquipItem(" not in augmented
    assert "AddItem(" not in augmented


def test_weapon_testing_range_has_no_speculative_behavior_fragment() -> None:
    assert not PATCH_PATH.exists()
