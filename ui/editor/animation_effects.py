"""Plain-English grouping for editable NIF animation channels."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


_FRIENDLY_EFFECT_CONTROLLERS = {
    "",
    "BSEffectShaderPropertyFloatController",
    "BSEffectShaderPropertyColorController",
    "BSLightingShaderPropertyFloatController",
    "BSLightingShaderPropertyColorController",
    "NiLightDimmerController",
    "NiLightRadiusController",
    "NiLightColorController",
}


@dataclass(frozen=True)
class EffectChannelSummary:
    channel_index: int
    node_name: str
    component: str
    property_label: str
    controller_type: str
    value_min: float
    value_max: float
    key_count: int


@dataclass(frozen=True)
class EffectStack:
    effect_type: str
    driver: str
    target_label: str
    timing: str
    output_label: str
    channels: list[EffectChannelSummary] = field(default_factory=list)


def build_effect_stacks(sequence: Any) -> list[EffectStack]:
    """Return friendly effect stacks for an editable sequence-like object."""
    channels = list(getattr(sequence, "channels", []) or [])
    if not channels:
        return []

    summaries = [
        _summarize_channel(index, channel)
        for index, channel in enumerate(channels)
    ]
    if _looks_like_display_flicker(sequence, summaries):
        return [_make_stack("Display Flicker", sequence, summaries)]
    if _looks_like_digit_counter(sequence, summaries):
        return [_make_stack("Digit Counter", sequence, summaries)]

    stacks: list[EffectStack] = []
    consumed: set[int] = set()
    for target in _target_order(summaries):
        target_channels = [s for s in summaries if s.node_name == target]
        uv_channels = [s for s in target_channels if _is_uv_property(s.property_label)]
        if _looks_like_progress_bar(uv_channels):
            stacks.append(_make_stack("Progress Bar", sequence, uv_channels))
            consumed.update(s.channel_index for s in uv_channels)

    for target in _target_order(summaries):
        target_channels = [
            s for s in summaries
            if s.node_name == target and s.channel_index not in consumed
        ]
        if not target_channels:
            continue
        uv_channels = [s for s in target_channels if _is_uv_property(s.property_label)]
        if _looks_like_texture_scroll(uv_channels):
            stacks.append(_make_stack("Texture Scroll", sequence, uv_channels))
            consumed.update(s.channel_index for s in uv_channels)

        for summary in target_channels:
            if summary.channel_index in consumed:
                continue
            stacks.append(_make_stack(_effect_type_for_channel(summary), sequence, [summary]))

    return stacks


def _summarize_channel(index: int, channel: Any) -> EffectChannelSummary:
    values = [float(getattr(key, "value", 0.0)) for key in getattr(channel, "keys", [])]
    property_label = (
        str(getattr(channel, "target_property", "") or "").strip()
        or _component_label(str(getattr(channel, "component", "")))
    )
    return EffectChannelSummary(
        channel_index=index,
        node_name=str(getattr(channel, "node_name", "") or ""),
        component=str(getattr(channel, "component", "") or ""),
        property_label=property_label,
        controller_type=str(getattr(channel, "controller_type", "") or ""),
        value_min=min(values) if values else 0.0,
        value_max=max(values) if values else 0.0,
        key_count=len(values),
    )


def _make_stack(
    effect_type: str,
    sequence: Any,
    channels: list[EffectChannelSummary],
) -> EffectStack:
    targets = _target_order(channels)
    return EffectStack(
        effect_type=effect_type,
        driver=_driver_for(effect_type, sequence),
        target_label=", ".join(targets) if targets else "Unknown Target",
        timing=_timing_for(effect_type, channels),
        output_label=_output_label(channels),
        channels=channels,
    )


def _driver_for(effect_type: str, sequence: Any) -> str:
    name = str(getattr(sequence, "name", "") or "").lower()
    if effect_type == "Digit Counter" or "count" in name:
        return "Scrub By Value"
    if str(getattr(sequence, "name", "") or "") == "[Property Controllers]":
        return "Always On"
    return "Play Named Sequence"


def _timing_for(effect_type: str, channels: list[EffectChannelSummary]) -> str:
    if effect_type == "Display Flicker":
        return "Random-Looking Flicker"
    if effect_type in {"Texture Scroll", "Progress Bar"}:
        return "Looping Ramp" if any(c.key_count > 2 for c in channels) else "Two-Key Ramp"
    if effect_type == "Digit Counter":
        return "Digit Atlas Steps"
    if any(c.key_count > 2 for c in channels):
        return "Manual Keyframes"
    if any(c.key_count == 2 for c in channels):
        return "Two-Key Ramp"
    return "Constant"


def _output_label(channels: list[EffectChannelSummary]) -> str:
    target_count = len(set(c.node_name for c in channels))
    return (
        f"{len(channels)} {_plural('channel', len(channels))}, "
        f"{target_count} {_plural('target', target_count)}"
    )


def _plural(label: str, count: int) -> str:
    return label if count == 1 else f"{label}s"


def _target_order(channels: list[EffectChannelSummary]) -> list[str]:
    return sorted({c.node_name for c in channels if c.node_name})


def _looks_like_display_flicker(sequence: Any, channels: list[EffectChannelSummary]) -> bool:
    if not _uses_friendly_effect_controllers(channels):
        return False
    name = str(getattr(sequence, "name", "") or "").lower()
    has_color = any(c.component.startswith("color_") for c in channels)
    has_light = any(c.property_label.lower() in {"dimmer", "radius"} for c in channels)
    return "flicker" in name or (has_color and has_light)


def _looks_like_digit_counter(sequence: Any, channels: list[EffectChannelSummary]) -> bool:
    if not _uses_friendly_effect_controllers(channels):
        return False
    name = str(getattr(sequence, "name", "") or "").lower()
    if "count" not in name and not any("count" in c.node_name.lower() for c in channels):
        return False
    return bool(channels) and all(_is_uv_property(c.property_label) for c in channels)


def _looks_like_progress_bar(channels: list[EffectChannelSummary]) -> bool:
    if not channels:
        return False
    if not _uses_friendly_effect_controllers(channels):
        return False
    target_text = " ".join(c.node_name.lower() for c in channels)
    if "bar" not in target_text and "progress" not in target_text:
        return False
    return any(_is_uv_property(c.property_label) for c in channels)


def _looks_like_texture_scroll(channels: list[EffectChannelSummary]) -> bool:
    if not _uses_friendly_effect_controllers(channels):
        return False
    return bool(channels) and any(_is_uv_property(c.property_label) for c in channels)


def _uses_friendly_effect_controllers(channels: list[EffectChannelSummary]) -> bool:
    return all(channel.controller_type in _FRIENDLY_EFFECT_CONTROLLERS for channel in channels)


def _is_uv_property(property_label: str) -> bool:
    normalized = property_label.lower()
    return normalized in {"u offset", "v offset", "u scale", "v scale"}


def _advanced_type(channel: EffectChannelSummary) -> str:
    if channel.controller_type == "NiPSysEmitterCtlr":
        return "Particle Emitter"
    if "PSys" in channel.controller_type or channel.controller_type.startswith(("NiPS", "BSPSys")):
        return "Particle Effect"
    if channel.controller_type and channel.controller_type not in _FRIENDLY_EFFECT_CONTROLLERS:
        return "Advanced Controller"
    if channel.component.startswith("color_"):
        return "Advanced Color"
    if channel.component == "bool":
        return "Advanced Bool"
    if channel.component.startswith(("pos_", "rot_")) or channel.component == "scale":
        return "Advanced Transform"
    return "Advanced Scalar"


def _effect_type_for_channel(channel: EffectChannelSummary) -> str:
    normalized = channel.property_label.lower().replace(" ", "")
    if normalized in {"alphatransparency", "alpha"}:
        return "Alpha Flicker"
    if normalized in {"emissivemultiple", "emissivemultiplier"}:
        return "Glow Pulse"
    return _advanced_type(channel)


def _component_label(component: str) -> str:
    labels = {
        "float": "Float",
        "bool": "Bool",
        "color_r": "Color R",
        "color_g": "Color G",
        "color_b": "Color B",
        "pos_x": "Pos X",
        "pos_y": "Pos Y",
        "pos_z": "Pos Z",
        "rot_w": "Rot W",
        "rot_x": "Rot X",
        "rot_y": "Rot Y",
        "rot_z": "Rot Z",
        "scale": "Scale",
    }
    return labels.get(component, component or "Channel")
