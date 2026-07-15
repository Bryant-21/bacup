"""Tests for weapon overlay clip synthesis."""
from __future__ import annotations

from bacup_lib.animation.weapon_overlay_synth import filter_and_retarget
from bacup_lib.models import (
    AnimationClip,
    AnimationKeyframe,
    BoneChannel,
)


def _kf(time: float, value: tuple[float, float, float]) -> AnimationKeyframe:
    return AnimationKeyframe(time=time, value=value)


def _channel(bone_name: str) -> BoneChannel:
    return BoneChannel(
        bone_name=bone_name,
        rotations=(),
        translations=(_kf(0.0, (0, 0, 0)), _kf(1.0, (1, 0, 0))),
        scales=(),
    )


def test_filter_keeps_only_matching_bones_and_renames():
    clip = AnimationClip(
        name="Fire",
        duration=1.0,
        channels=(
            _channel("Bip01 Hand"),
            _channel("Bip01 Magazine"),
            _channel("Bip01 Spine"),
            _channel("Bip01 Slide"),
        ),
        events=(),
    )
    filtered = filter_and_retarget(
        clip,
        keep_bones=["Bip01 Magazine", "Bip01 Slide"],
        bone_remap={"Bip01 Magazine": "WeaponMagazine", "Bip01 Slide": "WeaponBolt"},
    )
    assert [channel.bone_name for channel in filtered.channels] == [
        "WeaponMagazine",
        "WeaponBolt",
    ]
    assert filtered.is_additive is True


def test_filter_emits_empty_clip_when_no_bones_match():
    clip = AnimationClip(
        name="Fire",
        duration=1.0,
        channels=(_channel("Bip01 Hand"), _channel("Bip01 Spine")),
        events=(),
    )
    filtered = filter_and_retarget(clip, keep_bones=["Bip01 Magazine"], bone_remap={})
    assert filtered.channels == ()
    assert filtered.is_additive is False
