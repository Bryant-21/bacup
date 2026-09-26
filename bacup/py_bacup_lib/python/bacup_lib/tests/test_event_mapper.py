"""Tests for EventMapper — animation event name translation."""
from __future__ import annotations

import pytest

from bacup_lib.animation.event_mapper import EventMapper
from bacup_lib.models import AnimationEvent


@pytest.mark.parametrize(
    ("source", "target", "text", "expected"),
    [
        ("fo3", "fo4", "hit", "HitFrame"),
        ("fo3", "fo4", "Equip", "weaponDraw"),
        ("fo3", "fo4", "Unequip", "weaponSheathe"),
        ("fo3", "fo4", "Sound: WPNLaserFire", "SoundPlay.WPNLaserFire"),
        ("fo3", "fo4", "Attack: Power", "weaponSwing"),
        ("fo3", "fo4", "start", None),
        ("fo3", "fo4", "end", None),
        ("fo3", "fo4", "prn: Weapon", None),
        ("fnv", "fo4", "hit", "HitFrame"),
        ("fo4", "fo3", "HitFrame", "hit"),
        ("fo4", "fo3", "preHitFrame", "hit"),
        ("fo4", "fo3", "weaponDraw", "Equip"),
        ("fo4", "fo3", "weaponSheathe", "Unequip"),
        ("fo4", "fo3", "weaponFire", "Sound: WeaponFire"),
        ("fo4", "fo3", "weaponSwing", "Attack: Swing"),
        ("fo4", "fo3", "SoundPlay.WPNReload", "Sound: WPNReload"),
        ("fo4", "fo3", "FootLeft", None),
        ("fo4", "fo3", "FootBack", None),
        ("fo76", "fo4", "PathTweenerStart", None),
        ("fallout76", "fo4", "PathTweenerEnd", None),
        ("fo76", "fo4", "CharFXOnWild", None),
        ("fo76", "fo4", "RightSlam", None),
        ("fo76", "fo4", "FireBehemothSalvo", None),
        ("fo76", "fo4", "weaponFire.1", "weaponFire"),
        ("fo76", "fo4", "WeaponFire.2", "weaponFire"),
        ("fo76", "fo4", "weaponFire.12", "weaponFire"),
    ],
)
def test_map_event(source, target, text, expected):
    result, warning = EventMapper(source, target).map_event(
        AnimationEvent(time=0.9333334, text=text)
    )
    if expected is None:
        assert result is None
    else:
        assert result == AnimationEvent(time=0.9333334, text=expected)
    assert warning is None


@pytest.mark.parametrize(
    ("source", "target", "text"),
    [
        ("fo3", "fo4", "CustomModEvent"),
        ("fo4", "fo3", "SyncLeft"),
        ("fo4", "fo3", "start"),
        ("fo76", "fo4", "SomeModderCustomEvent"),
    ],
)
def test_unmapped_event_passes_through_with_warning(source, target, text):
    ev = AnimationEvent(time=0.7, text=text)
    result, warning = EventMapper(source, target).map_event(ev)
    assert result == ev
    assert warning == f"Unmapped animation event '{text}' passed through as-is"


def test_missing_pair_raises():
    with pytest.raises(FileNotFoundError):
        EventMapper("skyrimse", "fo4")


def test_fo76_common_events_pass_through():
    """Events snallygaster has and works fine with must NOT be dropped."""
    mapper = EventMapper("fo76", "fo4")
    for text in (
        "HitFrame", "preHitFrame", "weaponSwing",
        "weaponFire",
        "WeaponSweepAttackStart", "WeaponSweepAttackStop",
        "startAllowRotation", "startAnimationDriven",
        "SoundPlay.NPCSnallygasterAttackD",
        "SoundPlay.NPCMegaSlothAttackAoE1",
        "CameraShake.0.9,0.35,0.1",
        "FootLeft", "FootRight", "FootFrontLeft", "FootBackRight",
    ):
        result, _ = mapper.map_event(AnimationEvent(time=0.5, text=text))
        assert result is not None, f"{text} should not be dropped"
        assert result.text == text, f"{text} should pass through unchanged"


def test_fo3_map_events_batch():
    events = (
        AnimationEvent(time=0.0, text="start"),
        AnimationEvent(time=0.1, text="Equip"),
        AnimationEvent(time=0.5, text="hit"),
        AnimationEvent(time=0.8, text="Sound: Reload"),
        AnimationEvent(time=0.9, text="Unknown"),
        AnimationEvent(time=1.0, text="end"),
    )
    mapped, warnings = EventMapper("fo3", "fo4").map_events(events)
    assert [e.text for e in mapped] == ["weaponDraw", "HitFrame", "SoundPlay.Reload", "Unknown"]
    assert len(warnings) == 1
    assert "Unknown" in warnings[0]


def test_fo76_batch_drops_and_renames():
    events = (
        AnimationEvent(time=0.0, text="SoundPlay.Attack1"),
        AnimationEvent(time=0.1, text="PathTweenerStart"),
        AnimationEvent(time=0.2, text="weaponFire.1"),
        AnimationEvent(time=0.3, text="weaponSwing"),
        AnimationEvent(time=0.4, text="bothSlam"),
        AnimationEvent(time=0.5, text="HitFrame"),
    )
    mapped, _warnings = EventMapper("fo76", "fo4").map_events(events)
    assert [e.text for e in mapped] == [
        "SoundPlay.Attack1",
        "weaponFire",
        "weaponSwing",
        "HitFrame",
    ]
