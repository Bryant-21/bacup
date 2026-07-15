"""Weapon-bone overlay filtering for FNV/FO3 animation clips."""
from __future__ import annotations

from dataclasses import replace

from bacup_lib.models import AnimationClip, BoneChannel


def filter_and_retarget(
    clip: AnimationClip,
    keep_bones: list[str],
    bone_remap: dict[str, str],
) -> AnimationClip:
    """Return a copy of *clip* with only the selected weapon bones."""
    keep = set(keep_bones)
    channels: list[BoneChannel] = []
    for channel in clip.channels:
        if channel.bone_name not in keep:
            continue
        channels.append(
            replace(
                channel,
                bone_name=bone_remap.get(channel.bone_name, channel.bone_name),
            )
        )
    return replace(clip, channels=tuple(channels), is_additive=bool(channels))
