"""Animation Editor — visual keyframe curve editor for NIF animations.

Opens from the scene tree context menu on animation blocks. Provides an ImPlot-based
curve view with per-component channels, draggable keyframes, and playhead scrubbing
that drives the 3D viewport in real-time.
"""
from __future__ import annotations

import copy
import logging
import math
from dataclasses import dataclass, field

import numpy as np
from creation_lib.nif.schema import get_schema
from imgui_bundle import hello_imgui, icons_fontawesome_6 as fa, imgui, implot

from ui.editor.animation_effects import EffectStack, build_effect_stacks
from ui.editor.animation_authoring import (
    AuthoringTarget,
    ControllerChainSpec,
    ControllerRegistryEntry,
    LinkContext,
    ValueKind,
    add_controller_chain,
    build_controller_registry,
    build_controller_templates,
    remove_sequence_controller,
    remove_standalone_controller,
)
from ui.editor.panels.properties import (
    enum_option_names_and_values,
    float_field_slider_bounds,
    is_nif_float_sentinel,
)
from ui.editor.sound_events import (
    format_sound_text_key,
    parse_sound_text_key,
    play_sound_cue,
)
from ui.editor.particles.runtime import PARTICLE_PREVIEW_SEQUENCE

_log = logging.getLogger("nif_editor.animation_editor")

# Key interpolation types used in NIF
KEY_LINEAR = 1
KEY_QUADRATIC = 2
KEY_TBC = 3
KEY_CONST = 5

_KEY_TYPE_NAMES = {
    KEY_LINEAR: "Linear",
    KEY_QUADRATIC: "Quadratic",
    KEY_TBC: "TBC",
    KEY_CONST: "Constant",
}


# -- Data model --

@dataclass
class EditableKey:
    time: float
    value: float
    key_type: int = KEY_LINEAR
    forward: float = 0.0
    backward: float = 0.0
    source_key_index: int = -1


@dataclass
class EditableChannel:
    label: str                # "NodeName : Pos X"
    node_name: str
    component: str            # "pos_x", "pos_y", etc.
    keys: list[EditableKey] = field(default_factory=list)
    data_block_id: int = -1
    interp_block_id: int = -1
    color: tuple = (1.0, 1.0, 1.0, 1.0)
    visible: bool = True
    controller_type: str = ""
    controller_block_id: int = -1
    controlled_field: str = ""
    property_type: str = ""
    target_property: str = ""
    interpolator_type: str = ""
    is_particle: bool = False
    start_time: float = 0.0
    stop_time: float = 0.0
    frequency: float = 1.0
    phase: float = 0.0


@dataclass
class EditableSoundEvent:
    time: float
    cue: str
    source_key_index: int = -1


@dataclass
class EditableSequence:
    name: str
    block_id: int
    start_time: float
    stop_time: float
    cycle_type: int
    channels: list[EditableChannel] = field(default_factory=list)
    text_keys_block_id: int = -1
    sound_events: list[EditableSoundEvent] = field(default_factory=list)


# Channel colors by component
_COLORS = {
    "pos_x": (1.0, 0.2, 0.2, 1.0),   # Red
    "pos_y": (0.2, 1.0, 0.2, 1.0),   # Green
    "pos_z": (0.3, 0.3, 1.0, 1.0),   # Blue
    "rot_w": (1.0, 1.0, 1.0, 1.0),   # White
    "rot_x": (1.0, 0.5, 0.5, 1.0),   # Light red
    "rot_y": (0.5, 1.0, 0.5, 1.0),   # Light green
    "rot_z": (0.5, 0.5, 1.0, 1.0),   # Light blue
    "scale": (1.0, 1.0, 0.2, 1.0),   # Yellow
    "float": (0.2, 1.0, 1.0, 1.0),   # Cyan
    "color_r": (1.0, 0.35, 0.35, 1.0),
    "color_g": (0.35, 1.0, 0.35, 1.0),
    "color_b": (0.45, 0.55, 1.0, 1.0),
    "point_x": (1.0, 0.35, 0.35, 1.0),
    "point_y": (0.35, 1.0, 0.35, 1.0),
    "point_z": (0.45, 0.55, 1.0, 1.0),
    "bool": (1.0, 0.7, 0.2, 1.0),
}

_COMPONENT_LABELS = {
    "pos_x": "Pos X", "pos_y": "Pos Y", "pos_z": "Pos Z",
    "rot_w": "Rot W", "rot_x": "Rot X", "rot_y": "Rot Y", "rot_z": "Rot Z",
    "scale": "Scale", "float": "Float",
    "color_r": "Color R", "color_g": "Color G", "color_b": "Color B",
    "point_x": "Point X", "point_y": "Point Y", "point_z": "Point Z",
    "bool": "Bool",
}

_IMGUI_SLIDER_FLOAT_ABS_LIMIT = 1.7e38
_DEFAULT_EFFECT_RANGE_SLIDER_BOUNDS = (-10.0, 10.0)
_EFFECT_RANGE_SLIDER_BOUNDS_BY_PROPERTY = {
    "U Offset": (-10.0, 10.0),
    "V Offset": (-10.0, 10.0),
    "U Scale": (0.0, 10.0),
    "V Scale": (0.0, 10.0),
    "Alpha": (0.0, 1.0),
    "Alpha Transparency": (0.0, 1.0),
    "EmissiveMultiple": (0.0, 10.0),
    "Emissive Multiple": (0.0, 10.0),
    "Dimmer": (0.0, 10.0),
    "Radius": (0.0, 10.0),
}


def _effect_channel_slider_bounds(
    current_min: float,
    current_max: float,
    property_label: str = "",
) -> tuple[float, float] | None:
    values = (float(current_min), float(current_max))
    if any(not math.isfinite(value) for value in values):
        return None
    if any(is_nif_float_sentinel(value) for value in values):
        return None
    if any(abs(value) >= _IMGUI_SLIDER_FLOAT_ABS_LIMIT for value in values):
        return None

    low = min(values)
    high = max(values)
    slider_min, slider_max = _EFFECT_RANGE_SLIDER_BOUNDS_BY_PROPERTY.get(
        property_label,
        _DEFAULT_EFFECT_RANGE_SLIDER_BOUNDS,
    )
    if low < slider_min or high > slider_max:
        return None
    return slider_min, slider_max


def _format_effect_channel_range_value(value: float) -> str:
    value = float(value)
    if math.isnan(value):
        return "NaN"
    if is_nif_float_sentinel(value):
        return "FLT_MAX" if value > 0 else "-FLT_MAX"
    return f"{value:.4g}"

_EFFECT_FLOAT_PROPERTIES = {
    "0": "EmissiveMultiple",
    "1": "Falloff Start Angle",
    "2": "Falloff Stop Angle",
    "3": "Falloff Start Opacity",
    "4": "Falloff Stop Opacity",
    "5": "Alpha Transparency",
    "6": "U Offset",
    "7": "U Scale",
    "8": "V Offset",
    "9": "V Scale",
}

_EFFECT_COLOR_PROPERTIES = {"0": "Emissive Color"}

_LIGHTING_FLOAT_PROPERTIES = {
    "0": "Refraction Strength",
    "1": "Emissive Multiple",
    "2": "Environment Map Scale",
    "3": "Glossiness",
    "4": "Specular Strength",
    "5": "Alpha",
    "6": "U Offset",
    "7": "U Scale",
    "8": "V Offset",
    "9": "V Scale",
}

_LIGHTING_COLOR_PROPERTIES = {
    "0": "Specular Color",
    "1": "Emissive Color",
}

_CONTROLLER_DEFAULT_PROPERTIES = {
    "NiAlphaController": "Alpha",
    "NiLightDimmerController": "Dimmer",
    "NiLightRadiusController": "Radius",
    "NiLightColorController": "Diffuse Color",
    "NiVisController": "Visibility",
    "BSMaterialEmittanceMultController": "Emissive Multiple",
    "BSRefractionStrengthController": "Refraction Strength",
}

_CONTROLLED_CONTROLLER_FIELDS = (
    "Controlled Variable",
    "Controlled Color",
    "Controlled Float",
    "Controlled U Short",
)
_CONTROLLER_TIMING_SPECS = (
    ("Start Time", "start_time", 0.0),
    ("Stop Time", "stop_time", 0.0),
    ("Frequency", "frequency", 1.0),
    ("Phase", "phase", 0.0),
)


def _controller_timing_kwargs(controller_block) -> dict[str, float]:
    values: dict[str, float] = {}
    for field_name, attr_name, default in _CONTROLLER_TIMING_SPECS:
        raw_value = controller_block.get_field(field_name) if controller_block is not None else None
        values[attr_name] = float(default if raw_value in (None, "") else raw_value)
    return values


# Number of samples for smooth curve rendering
_CURVE_SAMPLES = 200


def _advanced_effect_template_rejection(entry: ControllerRegistryEntry, link_context: LinkContext) -> str:
    if not entry.interpolator_type or not entry.data_type:
        return "Controller does not expose key data"
    if link_context not in entry.link_contexts:
        return "Controller cannot be added to this animation context"
    if link_context is LinkContext.STANDALONE and entry.target_kind not in ("effect_shader_property", "shader_property"):
        return "Controller does not target shader effects"
    return ""


def _get_ref_id(ref) -> int:
    """Extract block index from a reference value."""
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1


def _default_template_value(value_kind: ValueKind):
    if value_kind is ValueKind.POINT3:
        return (1.0, 1.0, 1.0)
    if value_kind is ValueKind.TRANSFORM:
        return {"translation": (0.0, 0.0, 0.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0}
    if value_kind is ValueKind.BOOL:
        return True
    return 0.0


def _lerp_keys(keys: list[EditableKey], t: float) -> float:
    """Linearly interpolate value at time t from sorted keys."""
    if not keys:
        return 0.0
    if len(keys) == 1 or t <= keys[0].time:
        return keys[0].value
    if t >= keys[-1].time:
        return keys[-1].value
    for i in range(len(keys) - 1):
        if keys[i].time <= t <= keys[i + 1].time:
            k0, k1 = keys[i], keys[i + 1]
            dt = k1.time - k0.time
            if dt < 1e-9:
                return k0.value
            if k0.key_type == KEY_CONST:
                return k0.value
            frac = (t - k0.time) / dt
            return k0.value + frac * (k1.value - k0.value)
    return keys[-1].value


class AnimationEditorPanel:
    """Visual keyframe editor for NIF animation sequences."""

    def __init__(self, app):
        self.app = app
        self.window_name = "Animation Editor"
        self._visible = True
        self._dock_space = "RightDock"
        self._needs_dock = True
        self._focus_request = False  # Set True to focus/select the tab on next frame
        self._sequence: EditableSequence | None = None
        self._selected_channel_idx: int = -1
        self._selected_key_idx: int = -1
        self._playhead_time: float = 0.0
        self._plot_cache: dict[int, tuple[np.ndarray, np.ndarray]] = {}
        self._node_expanded: dict[str, bool] = {}
        self._show_legend: bool = False
        self._show_context_curves: bool = False
        self._scrub_realtime: bool = False
        self._scrub_active_this_frame: bool = False

    # -- Public API --

    def show(self):
        self._visible = True
        self._needs_dock = True

    def toggle(self):
        self._visible = not self._visible

    def open_for_block(self, block_id: int):
        """Open the editor for a given animation-related block."""
        _log.debug("open_for_block called with block_id=%d", block_id)
        nif = getattr(self.app, "nif", None)
        if nif is None:
            _log.warning("open_for_block: no active NIF (app.nif is None)")
            return

        _log.debug("open_for_block: active NIF has %d blocks", len(nif.blocks))
        seq_id = self._resolve_owning_sequence(nif, block_id)
        if seq_id is None:
            _log.warning("open_for_block: could not resolve owning NiControllerSequence for block %d", block_id)
            return

        _log.debug("open_for_block: resolved block %d -> sequence block %d", block_id, seq_id)
        self._load_sequence(seq_id)
        self._visible = True
        self._focus_request = True

        _log.info("Animation editor opened: visible=%s, sequence=%s, channels=%d",
                   self._visible,
                   self._sequence.name if self._sequence else "None",
                   len(self._sequence.channels) if self._sequence else 0)

        # Select in animation coordinator for cross-NIF live preview
        if self._sequence:
            self.app.anim_coordinator.play(self._sequence.name)

    def open_for_sequence_name(self, seq_name: str, *, focus: bool = False):
        """Open the editor for a sequence by name. Called when the animation panel selection changes."""
        # Skip if already showing this sequence
        if self._sequence and self._sequence.name == seq_name:
            return

        if seq_name == PARTICLE_PREVIEW_SEQUENCE:
            self._sequence = EditableSequence(
                name=PARTICLE_PREVIEW_SEQUENCE,
                block_id=-1,
                start_time=0.0,
                stop_time=5.0,
                cycle_type=0,
                channels=[],
            )
            self._visible = True
            if focus:
                self._focus_request = True
            return

        # Handle property controllers (standalone, not NiControllerSequence)
        if seq_name == "[Property Controllers]":
            self._load_property_controllers()
            self._visible = True
            if focus:
                self._focus_request = True
            return

        nif = getattr(self.app, "nif", None)
        if nif is None:
            return

        # Find the NiControllerSequence block with this name
        for b in nif.blocks:
            if b is None or b.type_name != "NiControllerSequence":
                continue
            name = self._get_string(b, "Name")
            if name == seq_name:
                self._load_sequence(b.block_id)
                self._visible = True
                if focus:
                    self._focus_request = True
                _log.debug("Animation editor synced to sequence '%s' (block %d)", seq_name, b.block_id)
                return

    # -- Sequence loading --

    def _load_sequence(self, seq_block_id: int):
        """Load an NiControllerSequence into the editor's data model."""
        nif = getattr(self.app, "nif", None)
        if nif is None:
            _log.warning("_load_sequence: app.nif is None, cannot load block %d", seq_block_id)
            return

        _log.debug("_load_sequence: loading sequence block %d", seq_block_id)
        seq = self._load_sequence_from_nif(nif, seq_block_id)
        if seq is None:
            _log.warning("_load_sequence: _load_sequence_from_nif returned None for block %d", seq_block_id)
            return

        _log.debug("_load_sequence: loaded '%s' with %d channels", seq.name, len(seq.channels))
        self._sequence = seq
        self._selected_channel_idx = 0 if seq.channels else -1
        self._selected_key_idx = -1
        self._playhead_time = seq.start_time
        self._node_expanded.clear()
        self._rebuild_plot_cache()
        self._sync_panel_selection(seq.name)

        # Auto-expand first node
        if seq.channels:
            first_node = seq.channels[0].node_name
            self._node_expanded[first_node] = True

    def _load_property_controllers(self):
        """Load standalone property controllers into the editor."""
        mgr = None
        registry = getattr(self.app, 'registry', None)
        if registry:
            try:
                mgr = registry.active_session.anim_manager
            except (KeyError, AttributeError):
                pass
        if mgr is None:
            mgr = getattr(self.app, 'animation_mgr', None)
        if mgr is None or not mgr._property_channels:
            return

        nif = getattr(self.app, "nif", None)

        # Build a synthetic EditableSequence from property controllers
        max_stop = max(
            pc.stop_time for pc in mgr._property_channels
        )
        seq = EditableSequence(
            name="[Property Controllers]",
            block_id=-1,
            start_time=0.0,
            stop_time=max(max_stop, 0.001),
            cycle_type=0,
        )

        _PROP_COLORS = {
            "U Offset": (0.2, 0.8, 1.0, 1.0),     # Cyan
            "V Offset": (1.0, 0.4, 0.8, 1.0),     # Pink
            "U Scale": (0.2, 1.0, 0.6, 1.0),      # Teal
            "V Scale": (1.0, 0.8, 0.2, 1.0),      # Gold
            "EmissiveMultiple": (1.0, 1.0, 0.4, 1.0),  # Yellow
        }

        for pc in mgr._property_channels:
            keys = []
            for i, fk in enumerate(pc.float_keys):
                keys.append(EditableKey(
                    time=fk.time, value=fk.value,
                    key_type=KEY_LINEAR, source_key_index=i,
                ))
            label = f"{pc.node_name} : {pc.material_var}"
            if pc.frequency != 1.0:
                label += f" (x{pc.frequency:.2f})"
            ch = EditableChannel(
                label=label,
                node_name=pc.node_name,
                component="float",
                keys=keys,
                data_block_id=getattr(pc, "data_block_id", -1),
                interp_block_id=getattr(pc, "interpolator_block_id", -1),
                color=_PROP_COLORS.get(pc.material_var, (0.8, 0.8, 0.8, 1.0)),
                controller_type=getattr(pc, "controller_type", "") or "BSEffectShaderPropertyFloatController",
                controller_block_id=getattr(pc, "controller_block_id", -1),
                controlled_field=self._controlled_field_for_controller(
                    getattr(pc, "controller_type", "") or "BSEffectShaderPropertyFloatController",
                    nif.get_block(getattr(pc, "controller_block_id", -1)) if nif is not None else None,
                ),
                property_type=getattr(pc, "property_type", "") or "BSEffectShaderProperty",
                target_property=pc.material_var,
                start_time=float(getattr(pc, "start_time", 0.0)),
                stop_time=float(getattr(pc, "stop_time", 0.0)),
                frequency=float(getattr(pc, "frequency", 1.0)),
                phase=float(getattr(pc, "phase", 0.0)),
            )
            seq.channels.append(ch)

        self._sequence = seq
        self._selected_channel_idx = 0 if seq.channels else -1
        self._selected_key_idx = -1
        self._playhead_time = seq.start_time
        self._node_expanded.clear()
        self._rebuild_plot_cache()
        self._sync_panel_selection(seq.name)

        if seq.channels:
            first_node = seq.channels[0].node_name
            self._node_expanded[first_node] = True

        _log.debug("Loaded property controllers: %d channels", len(seq.channels))

    def _load_sequence_from_nif(self, nif, seq_block_id: int) -> EditableSequence | None:
        """Parse an NiControllerSequence into EditableSequence with per-component channels."""
        block = nif.get_block(seq_block_id)
        if block is None:
            return None

        name = self._get_string(block, "Name") or f"Sequence_{seq_block_id}"
        start_time = float(block.get_field("Start Time") or 0.0)
        stop_time = float(block.get_field("Stop Time") or 0.0)
        cycle_type = int(block.get_field("Cycle Type") or 0)

        seq = EditableSequence(
            name=name, block_id=seq_block_id,
            start_time=start_time, stop_time=stop_time,
            cycle_type=cycle_type,
        )
        seq.text_keys_block_id = _get_ref_id(block.get_field("Text Keys"))
        seq.sound_events = self._parse_sound_events(nif, seq.text_keys_block_id)

        controlled = block.get_field("Controlled Blocks")
        if not controlled or not isinstance(controlled, list):
            return seq

        for cb in controlled:
            if not isinstance(cb, dict):
                continue
            channels = self._parse_controlled_block_to_channels(nif, cb)
            seq.channels.extend(channels)

        return seq

    def _parse_sound_events(self, nif, text_keys_block_id: int) -> list[EditableSoundEvent]:
        if text_keys_block_id < 0:
            return []
        text_block = nif.get_block(text_keys_block_id)
        if text_block is None or text_block.type_name != "NiTextKeyExtraData":
            return []

        result: list[EditableSoundEvent] = []
        for index, entry in enumerate(text_block.get_field("Text Keys") or []):
            if not isinstance(entry, dict):
                continue
            cue = parse_sound_text_key(entry.get("Value"))
            if not cue:
                continue
            result.append(EditableSoundEvent(
                time=float(entry.get("Time", 0.0)),
                cue=cue,
                source_key_index=index,
            ))
        return sorted(result, key=lambda event: event.time)

    def _parse_controlled_block_to_channels(
        self, nif, cb: dict
    ) -> list[EditableChannel]:
        """Parse a single controlled block into per-component EditableChannels."""
        node_name = cb.get("Node Name") or cb.get("Target Name") or ""
        if isinstance(node_name, (int, float)):
            node_name = self._resolve_string_index(nif, int(node_name))
        if not node_name:
            return []

        interp_ref = _get_ref_id(cb.get("Interpolator", -1))
        if interp_ref < 0:
            return []

        interp_block = nif.get_block(interp_ref)
        if interp_block is None:
            return []

        controller_ref = _get_ref_id(cb.get("Controller", -1))
        controller_block = nif.get_block(controller_ref) if controller_ref >= 0 else None
        controller_type = str(
            getattr(controller_block, "type_name", "")
            or cb.get("Controller Type")
            or ""
        )
        timing_kwargs = _controller_timing_kwargs(controller_block)
        property_type = str(cb.get("Property Type") or "")
        controlled_field = self._controlled_field_for_controller(controller_type, controller_block)
        target_property = self._resolve_controller_property(
            controller_type,
            cb.get("Controller ID"),
            controller_block,
        )

        schema = nif.schema
        channels = []

        if schema.is_subtype_of(interp_block.type_name, "NiTransformInterpolator"):
            channels = self._parse_transform_channels(nif, interp_block, node_name)
        elif schema.is_subtype_of(interp_block.type_name, "NiFloatInterpolator"):
            channels = self._parse_float_channels(nif, interp_block, node_name)
        elif schema.is_subtype_of(interp_block.type_name, "NiPoint3Interpolator"):
            channels = self._parse_point3_channels(
                nif, interp_block, node_name, target_property
            )
        elif schema.is_subtype_of(interp_block.type_name, "NiBoolInterpolator"):
            channels = self._parse_bool_channels(nif, interp_block, node_name)

        for channel in channels:
            self._apply_channel_metadata(
                channel,
                controller_type=controller_type,
                controller_block_id=controller_ref,
                controlled_field=controlled_field,
                property_type=property_type,
                target_property=target_property,
                interpolator_type=interp_block.type_name,
                **timing_kwargs,
            )

        return channels

    def _resolve_controller_property(
        self,
        controller_type: str,
        controller_id,
        controller_block,
    ) -> str:
        if controller_block is not None:
            for field_name in _CONTROLLED_CONTROLLER_FIELDS:
                raw_value = controller_block.get_field(field_name)
                if raw_value not in (None, ""):
                    return self._controller_field_display_value(
                        controller_block,
                        field_name,
                        raw_value,
                    )

        if controller_type in _CONTROLLER_DEFAULT_PROPERTIES:
            return _CONTROLLER_DEFAULT_PROPERTIES[controller_type]

        ctrl_id = "" if controller_id is None else str(controller_id)
        if controller_type == "BSEffectShaderPropertyFloatController":
            return _EFFECT_FLOAT_PROPERTIES.get(ctrl_id, f"Controller ID {ctrl_id}")
        if controller_type == "BSEffectShaderPropertyColorController":
            return _EFFECT_COLOR_PROPERTIES.get(ctrl_id, f"Controller ID {ctrl_id}")
        if controller_type == "BSLightingShaderPropertyFloatController":
            return _LIGHTING_FLOAT_PROPERTIES.get(ctrl_id, f"Controller ID {ctrl_id}")
        if controller_type == "BSLightingShaderPropertyColorController":
            return _LIGHTING_COLOR_PROPERTIES.get(ctrl_id, f"Controller ID {ctrl_id}")
        return ""

    def _controlled_field_for_controller(self, controller_type: str, controller_block) -> str:
        if controller_block is not None:
            for field_name in _CONTROLLED_CONTROLLER_FIELDS:
                if controller_block.get_field(field_name) is not None:
                    return field_name
        schema = getattr(self.app.nif, "schema", None) if getattr(self.app, "nif", None) is not None else None
        if schema is None or not controller_type:
            return ""
        field_names = {field.name for field in schema.get_all_fields(controller_type)}
        for field_name in _CONTROLLED_CONTROLLER_FIELDS:
            if field_name in field_names:
                return field_name
        return ""

    def _field_def_for_block(self, block, field_name: str):
        nif = getattr(self.app, "nif", None)
        schema = getattr(nif, "schema", None)
        if schema is None or block is None:
            return None
        for field in schema.get_all_fields(block.type_name):
            if field.name == field_name:
                return field
        return None

    def _controller_field_display_value(self, controller_block, field_name: str, raw_value) -> str:
        fdef = self._field_def_for_block(controller_block, field_name)
        schema = getattr(getattr(self.app, "nif", None), "schema", None)
        enum_def = schema.enums.get(fdef.type) if schema is not None and fdef is not None else None
        if enum_def is None:
            return str(raw_value)
        option_names, option_values = enum_option_names_and_values(enum_def)
        if raw_value in option_values:
            return option_names[option_values.index(raw_value)]
        if isinstance(raw_value, str):
            if raw_value in option_names:
                return raw_value
            try:
                value = int(raw_value)
            except ValueError:
                return raw_value
            if value in option_values:
                return option_names[option_values.index(value)]
        return str(raw_value)

    def _apply_channel_metadata(
        self,
        channel: EditableChannel,
        *,
        controller_type: str,
        controller_block_id: int,
        controlled_field: str,
        property_type: str,
        target_property: str,
        interpolator_type: str,
        start_time: float = 0.0,
        stop_time: float = 0.0,
        frequency: float = 1.0,
        phase: float = 0.0,
    ) -> None:
        channel.controller_type = controller_type
        channel.controller_block_id = controller_block_id
        channel.controlled_field = controlled_field
        channel.property_type = property_type
        channel.target_property = target_property
        channel.interpolator_type = interpolator_type
        channel.is_particle = controller_type.startswith(("NiPSys", "BSPSys", "NiPS"))
        channel.start_time = float(start_time)
        channel.stop_time = float(stop_time)
        channel.frequency = float(frequency)
        channel.phase = float(phase)
        channel.label = self._channel_label(channel)

    def _channel_label(self, channel: EditableChannel) -> str:
        label = channel.target_property or _COMPONENT_LABELS.get(
            channel.component, channel.component
        )
        if channel.target_property and channel.component in {
            "color_r",
            "color_g",
            "color_b",
            "point_x",
            "point_y",
            "point_z",
        }:
            suffix = _COMPONENT_LABELS.get(channel.component, channel.component).split()[-1]
            label = f"{channel.target_property} {suffix}"
        return f"{channel.node_name} : {label}"

    def _parse_transform_channels(
        self, nif, interp_block, node_name: str
    ) -> list[EditableChannel]:
        """Parse NiTransformInterpolator into position/rotation/scale channels."""
        data_ref = _get_ref_id(interp_block.get_field("Data"))
        interp_id = interp_block.block_id

        if data_ref < 0:
            # Static transform — create single-key channels
            return self._parse_static_transform_channels(interp_block, node_name, interp_id)

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []

        channels = []
        data_id = data_block.block_id

        # Position keys
        translations = data_block.get_field("Translations")
        if translations and isinstance(translations, dict):
            key_list = translations.get("Keys") or []
            for comp, axis in [("pos_x", "x"), ("pos_y", "y"), ("pos_z", "z")]:
                keys = []
                for i, k in enumerate(key_list):
                    if not isinstance(k, dict):
                        continue
                    t = float(k.get("Time", 0))
                    val = k.get("Value", {})
                    if isinstance(val, dict):
                        v = float(val.get(axis, 0))
                    else:
                        v = 0.0
                    key_type = int(k.get("Interpolation", KEY_LINEAR))
                    fwd = float(k.get("Forward", {}).get(axis, 0)) if isinstance(k.get("Forward"), dict) else 0.0
                    bwd = float(k.get("Backward", {}).get(axis, 0)) if isinstance(k.get("Backward"), dict) else 0.0
                    keys.append(EditableKey(
                        time=t, value=v, key_type=key_type,
                        forward=fwd, backward=bwd, source_key_index=i,
                    ))
                if keys:
                    label = f"{node_name} : {_COMPONENT_LABELS[comp]}"
                    channels.append(EditableChannel(
                        label=label, node_name=node_name, component=comp,
                        keys=keys, data_block_id=data_id, interp_block_id=interp_id,
                        color=_COLORS[comp],
                    ))

        # Rotation keys
        rotations = data_block.get_field("Rotations")
        if rotations and isinstance(rotations, dict):
            key_list = rotations.get("Keys") or rotations.get("Quaternion Keys") or []
            for comp, axis in [("rot_w", "w"), ("rot_x", "x"), ("rot_y", "y"), ("rot_z", "z")]:
                keys = []
                for i, k in enumerate(key_list):
                    if not isinstance(k, dict):
                        continue
                    t = float(k.get("Time", 0))
                    val = k.get("Value", {})
                    if isinstance(val, dict):
                        v = float(val.get(axis, 1.0 if axis == "w" else 0.0))
                    else:
                        v = 1.0 if axis == "w" else 0.0
                    key_type = int(k.get("Interpolation", KEY_LINEAR))
                    keys.append(EditableKey(
                        time=t, value=v, key_type=key_type, source_key_index=i,
                    ))
                if keys:
                    label = f"{node_name} : {_COMPONENT_LABELS[comp]}"
                    channels.append(EditableChannel(
                        label=label, node_name=node_name, component=comp,
                        keys=keys, data_block_id=data_id, interp_block_id=interp_id,
                        color=_COLORS[comp],
                    ))
        else:
            xyz_rotations = data_block.get_field("XYZ Rotations")
            if xyz_rotations and isinstance(xyz_rotations, list):
                for axis_index, comp in enumerate(("rot_x", "rot_y", "rot_z")):
                    if axis_index >= len(xyz_rotations):
                        continue
                    axis_group = xyz_rotations[axis_index]
                    if not isinstance(axis_group, dict):
                        continue
                    keys = []
                    for i, k in enumerate(axis_group.get("Keys") or []):
                        if not isinstance(k, dict):
                            continue
                        keys.append(EditableKey(
                            time=float(k.get("Time", 0)),
                            value=float(k.get("Value", 0)),
                            key_type=int(axis_group.get("Interpolation", KEY_LINEAR)),
                            forward=float(k.get("Forward", 0.0)),
                            backward=float(k.get("Backward", 0.0)),
                            source_key_index=i,
                        ))
                    if keys:
                        label = f"{node_name} : {_COMPONENT_LABELS[comp]}"
                        channels.append(EditableChannel(
                            label=label, node_name=node_name, component=comp,
                            keys=keys, data_block_id=data_id, interp_block_id=interp_id,
                            color=_COLORS[comp],
                        ))

        # Scale keys
        scales = data_block.get_field("Scales")
        if scales and isinstance(scales, dict):
            key_list = scales.get("Keys") or []
            keys = []
            for i, k in enumerate(key_list):
                if not isinstance(k, dict):
                    continue
                t = float(k.get("Time", 0))
                v = float(k.get("Value", 1.0))
                key_type = int(k.get("Interpolation", KEY_LINEAR))
                keys.append(EditableKey(
                    time=t, value=v, key_type=key_type, source_key_index=i,
                ))
            if keys:
                label = f"{node_name} : Scale"
                channels.append(EditableChannel(
                    label=label, node_name=node_name, component="scale",
                    keys=keys, data_block_id=data_id, interp_block_id=interp_id,
                    color=_COLORS["scale"],
                ))

        return channels

    def _parse_static_transform_channels(
        self, interp_block, node_name: str, interp_id: int
    ) -> list[EditableChannel]:
        """Create single-key channels from a static NiTransformInterpolator."""
        channels = []
        trans = interp_block.get_field("Translation")
        if trans and isinstance(trans, dict):
            for comp, axis in [("pos_x", "x"), ("pos_y", "y"), ("pos_z", "z")]:
                v = float(trans.get(axis, 0))
                if v != 0:
                    channels.append(EditableChannel(
                        label=f"{node_name} : {_COMPONENT_LABELS[comp]}",
                        node_name=node_name, component=comp,
                        keys=[EditableKey(time=0.0, value=v)],
                        data_block_id=-1, interp_block_id=interp_id,
                        color=_COLORS[comp],
                    ))

        rot = interp_block.get_field("Rotation")
        if rot and isinstance(rot, dict):
            for comp, axis in [("rot_w", "w"), ("rot_x", "x"), ("rot_y", "y"), ("rot_z", "z")]:
                v = float(rot.get(axis, 1.0 if axis == "w" else 0.0))
                channels.append(EditableChannel(
                    label=f"{node_name} : {_COMPONENT_LABELS[comp]}",
                    node_name=node_name, component=comp,
                    keys=[EditableKey(time=0.0, value=v)],
                    data_block_id=-1, interp_block_id=interp_id,
                    color=_COLORS[comp],
                ))

        scale = interp_block.get_field("Scale")
        if scale is not None and float(scale) != 1.0:
            channels.append(EditableChannel(
                label=f"{node_name} : Scale",
                node_name=node_name, component="scale",
                keys=[EditableKey(time=0.0, value=float(scale))],
                data_block_id=-1, interp_block_id=interp_id,
                color=_COLORS["scale"],
            ))

        return channels

    def _parse_float_channels(
        self, nif, interp_block, node_name: str
    ) -> list[EditableChannel]:
        """Parse NiFloatInterpolator into a single float channel."""
        data_ref = _get_ref_id(interp_block.get_field("Data"))
        interp_id = interp_block.block_id

        if data_ref < 0:
            val = interp_block.get_field("Value")
            if val is not None:
                return [EditableChannel(
                    label=f"{node_name} : Float",
                    node_name=node_name, component="float",
                    keys=[EditableKey(time=0.0, value=float(val))],
                    data_block_id=-1, interp_block_id=interp_id,
                    color=_COLORS["float"],
                )]
            return []

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []

        data_id = data_block.block_id
        data = data_block.get_field("Data")
        if not data or not isinstance(data, dict):
            return []

        key_list = data.get("Keys") or []
        keys = []
        for i, k in enumerate(key_list):
            if not isinstance(k, dict):
                continue
            t = float(k.get("Time", 0))
            v = float(k.get("Value", 0))
            key_type = int(k.get("Interpolation", KEY_LINEAR))
            fwd = float(k.get("Forward", 0)) if k.get("Forward") is not None else 0.0
            bwd = float(k.get("Backward", 0)) if k.get("Backward") is not None else 0.0
            keys.append(EditableKey(
                time=t, value=v, key_type=key_type,
                forward=fwd, backward=bwd, source_key_index=i,
            ))

        if not keys:
            return []

        return [EditableChannel(
            label=f"{node_name} : Float",
            node_name=node_name, component="float",
            keys=keys, data_block_id=data_id, interp_block_id=interp_id,
            color=_COLORS["float"],
        )]

    def _parse_point3_channels(
        self,
        nif,
        interp_block,
        node_name: str,
        target_property: str,
    ) -> list[EditableChannel]:
        """Parse NiPoint3Interpolator into three scalar channels."""
        data_ref = _get_ref_id(interp_block.get_field("Data"))
        interp_id = interp_block.block_id
        color_components = target_property.lower().endswith("color")
        components = (
            ("color_r", "x"), ("color_g", "y"), ("color_b", "z")
        ) if color_components else (
            ("point_x", "x"), ("point_y", "y"), ("point_z", "z")
        )

        if data_ref < 0:
            value = interp_block.get_field("Value")
            if not isinstance(value, dict):
                return []
            channels = []
            for component, axis in components:
                raw_value = float(value.get(axis, 0.0))
                if raw_value < -3.0e38:
                    continue
                channels.append(EditableChannel(
                    label=f"{node_name} : {_COMPONENT_LABELS[component]}",
                    node_name=node_name,
                    component=component,
                    keys=[EditableKey(time=0.0, value=raw_value)],
                    data_block_id=-1,
                    interp_block_id=interp_id,
                    color=_COLORS[component],
                ))
            return channels

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []

        data = data_block.get_field("Data")
        if not data or not isinstance(data, dict):
            return []

        key_list = data.get("Keys") or []
        channels = []
        for component, axis in components:
            keys = []
            for i, k in enumerate(key_list):
                if not isinstance(k, dict):
                    continue
                value = k.get("Value", {})
                if not isinstance(value, dict):
                    continue
                forward = k.get("Forward")
                backward = k.get("Backward")
                keys.append(EditableKey(
                    time=float(k.get("Time", 0.0)),
                    value=float(value.get(axis, 0.0)),
                    key_type=int(k.get("Interpolation", data.get("Interpolation", KEY_LINEAR))),
                    forward=float(forward.get(axis, 0.0)) if isinstance(forward, dict) else 0.0,
                    backward=float(backward.get(axis, 0.0)) if isinstance(backward, dict) else 0.0,
                    source_key_index=i,
                ))
            if keys:
                channels.append(EditableChannel(
                    label=f"{node_name} : {_COMPONENT_LABELS[component]}",
                    node_name=node_name,
                    component=component,
                    keys=keys,
                    data_block_id=data_block.block_id,
                    interp_block_id=interp_id,
                    color=_COLORS[component],
                ))
        return channels

    def _parse_bool_channels(
        self, nif, interp_block, node_name: str
    ) -> list[EditableChannel]:
        """Parse NiBoolInterpolator into a 0/1 editable channel."""
        data_ref = _get_ref_id(interp_block.get_field("Data"))
        interp_id = interp_block.block_id
        if data_ref < 0:
            value = interp_block.get_field("Value")
            if value is None:
                return []
            return [EditableChannel(
                label=f"{node_name} : Bool",
                node_name=node_name,
                component="bool",
                keys=[EditableKey(time=0.0, value=1.0 if bool(value) else 0.0)],
                data_block_id=-1,
                interp_block_id=interp_id,
                color=_COLORS["bool"],
            )]

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []
        data = data_block.get_field("Data")
        if not data or not isinstance(data, dict):
            return []

        keys = []
        for i, k in enumerate(data.get("Keys") or []):
            if not isinstance(k, dict):
                continue
            keys.append(EditableKey(
                time=float(k.get("Time", 0.0)),
                value=1.0 if bool(k.get("Value", False)) else 0.0,
                key_type=int(k.get("Interpolation", data.get("Interpolation", KEY_CONST))),
                source_key_index=i,
            ))

        if not keys:
            return []
        return [EditableChannel(
            label=f"{node_name} : Bool",
            node_name=node_name,
            component="bool",
            keys=keys,
            data_block_id=data_block.block_id,
            interp_block_id=interp_id,
            color=_COLORS["bool"],
        )]

    # -- Block resolution --

    def _resolve_owning_sequence(self, nif, block_id: int) -> int | None:
        """Find the NiControllerSequence that owns (or is) the given block."""
        block = nif.get_block(block_id)
        if block is None:
            return None

        type_name = block.type_name

        # Direct sequence
        if type_name == "NiControllerSequence":
            return block_id

        # NiControllerManager — return first sequence
        if type_name == "NiControllerManager":
            seq_refs = block.get_field("Controller Sequences")
            if isinstance(seq_refs, list):
                for ref in seq_refs:
                    rid = _get_ref_id(ref)
                    if rid >= 0:
                        return rid
            return None

        # Data or interpolator block — scan all sequences for a reference
        # First, for data blocks, find which interpolator refs this data
        interp_ids = set()
        if "Data" in type_name:
            for b in nif.blocks:
                if b is None:
                    continue
                data_ref = _get_ref_id(b.get_field("Data") if b.get_field("Data") is not None else -1)
                if data_ref == block_id:
                    interp_ids.add(b.block_id)
        else:
            interp_ids.add(block_id)

        # Now find which sequence references these interpolators
        for b in nif.blocks:
            if b is None or b.type_name != "NiControllerSequence":
                continue
            controlled = b.get_field("Controlled Blocks")
            if not controlled or not isinstance(controlled, list):
                continue
            for cb in controlled:
                if not isinstance(cb, dict):
                    continue
                interp_ref = _get_ref_id(cb.get("Interpolator", -1))
                if interp_ref in interp_ids:
                    return b.block_id

        return None

    # -- Plot cache --

    def _rebuild_plot_cache(self):
        """Build numpy arrays for curve rendering."""
        self._plot_cache.clear()
        if self._sequence is None:
            return

        seq = self._sequence
        t_range = seq.stop_time - seq.start_time
        if t_range <= 0:
            return

        for i, ch in enumerate(seq.channels):
            if not ch.keys:
                continue
            xs = np.linspace(seq.start_time, seq.stop_time, _CURVE_SAMPLES)
            ys = np.array([_lerp_keys(ch.keys, t) for t in xs], dtype=np.float64)
            self._plot_cache[i] = (xs, ys)

    # -- Draw --

    def _apply_dock(self):
        """Dock into assigned dock space on first render or re-show."""
        if self._needs_dock:
            dp = hello_imgui.get_runner_params().docking_params
            dock_id = dp.dock_space_id_from_name(self._dock_space)
            if dock_id is not None:
                imgui.set_next_window_dock_id(dock_id)
            self._needs_dock = False

    def draw(self):
        if not self._visible:
            return
        self._scrub_active_this_frame = False

        # Sync playhead from animation manager (follows playback + panel scrubber)
        mgr = self._get_active_mgr()
        if mgr and self._sequence and mgr.current_sequence is not None:
            if mgr.current_sequence.name != self._sequence.name:
                self.open_for_sequence_name(mgr.current_sequence.name)
            else:
                self._playhead_time = mgr.current_time

        seq_name = self._sequence.name if self._sequence else None
        # Use window_name as stable ID (toolkit overrides it to "Animation Editor##nif")
        title = f"Anim: {seq_name}###{self.window_name}" if seq_name else self.window_name

        # Dock into RightDock on first render or re-show
        self._apply_dock()

        # Focus/select this tab when a new sequence is loaded
        if self._focus_request:
            imgui.set_next_window_focus()
            self._focus_request = False

        expanded, opened = imgui.begin(title, True)
        if not opened:
            _log.debug("Animation Editor: window closed by user")
            self._visible = False
            self._set_scrub_realtime(False)
            imgui.end()
            return

        seq_names = self._get_sequence_names()
        if self._sequence is not None and self._sequence.name not in seq_names:
            if seq_names:
                self.open_for_sequence_name(seq_names[0])
            else:
                self._sequence = None

        if self._sequence is None:
            # Auto-load the first available sequence after a NIF is loaded
            if seq_names:
                self.open_for_sequence_name(seq_names[0])
            else:
                imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                                   "No animations found.")
                self._set_scrub_realtime(False)
                imgui.end()
                return

        self._draw_sequence_nav()
        self._draw_transport_controls()
        self._draw_sound_events()

        if expanded:
            if imgui.begin_tab_bar("##animation_editor_tabs"):
                selected, _ = imgui.begin_tab_item("Effects")
                if selected:
                    self._draw_effects_view()
                    imgui.end_tab_item()

                selected, _ = imgui.begin_tab_item("Animation Curves")
                if selected:
                    self._draw_curve_editor_body()
                    imgui.end_tab_item()
                imgui.end_tab_bar()

        self._set_scrub_realtime(self._scrub_active_this_frame)
        imgui.end()

    def _draw_curve_editor_body(self):
        if self._sequence is None:
            return

        self._draw_curve_options()

        avail = imgui.get_content_region_avail()
        channel_w = min(220.0, max(150.0, avail.x * 0.34))
        detail_h = 86.0
        plot_h = max(avail.y - detail_h - 8, 100)

        # --- Channel list (left) ---
        if channel_w > 0:
            imgui.begin_child("##channels", imgui.ImVec2(channel_w, plot_h), child_flags=imgui.ChildFlags_.borders)
            self._draw_channel_list()
            imgui.end_child()

            imgui.same_line()

        # --- Curve view (center) ---
        imgui.begin_child("##curves", imgui.ImVec2(0, plot_h))
        self._draw_curve_view()
        imgui.end_child()

        # --- Detail bar (bottom) ---
        imgui.separator()
        self._draw_detail_bar()

    def _add_effect_for_target(self, target_block_id: int, template_id: str, link_context: LinkContext) -> None:
        nif = getattr(self.app, "nif", None)
        undo_manager = getattr(self.app, "undo_manager", None)
        nif_id = self._active_nif_id()
        if nif is None or undo_manager is None or nif_id == "":
            return
        try:
            target = self._authoring_target_for_template(target_block_id, template_id)
            spec = self._template_to_chain_spec(target, template_id, link_context)
        except ValueError:
            return

        from ui.editor.undo import SnapshotAction

        cmd = SnapshotAction(_description=f"Add animation effect: {template_id}")
        cmd.capture_before(nif)
        try:
            add_controller_chain(nif, spec)
        except ValueError:
            return
        cmd.capture_after(nif)
        undo_manager.push(nif_id, cmd)
        self._after_authoring_mutation()

    def _remove_standalone_effect(self, target_block_id: int, controller_block_id: int) -> None:
        nif = getattr(self.app, "nif", None)
        undo_manager = getattr(self.app, "undo_manager", None)
        nif_id = self._active_nif_id()
        if nif is None or undo_manager is None or nif_id == "":
            return
        from ui.editor.undo import SnapshotAction

        cmd = SnapshotAction(_description=f"Remove animation effect: {controller_block_id}")
        cmd.capture_before(nif)
        remove_standalone_controller(nif, target_block_id, controller_block_id)
        cmd.capture_after(nif)
        undo_manager.push(nif_id, cmd)
        self._after_authoring_mutation()

    def _remove_sequence_effect(self, sequence_block_id: int, controller_block_id: int) -> None:
        nif = getattr(self.app, "nif", None)
        undo_manager = getattr(self.app, "undo_manager", None)
        nif_id = self._active_nif_id()
        if nif is None or undo_manager is None or nif_id == "":
            return
        from ui.editor.undo import SnapshotAction

        cmd = SnapshotAction(_description=f"Remove animation effect: {controller_block_id}")
        cmd.capture_before(nif)
        remove_sequence_controller(nif, sequence_block_id, controller_block_id)
        cmd.capture_after(nif)
        undo_manager.push(nif_id, cmd)
        self._after_authoring_mutation()

    def _after_authoring_mutation(self) -> None:
        registry = getattr(self.app, "registry", None)
        session = getattr(registry, "active_session", None) if registry else None
        manager = getattr(session, "anim_manager", None) if session else None
        if manager is not None:
            manager.scan(self.app.nif)
        if self._sequence and self._sequence.name == "[Property Controllers]":
            if manager is None or not getattr(manager, "_property_channels", []):
                self._sequence = EditableSequence(
                    name="[Property Controllers]",
                    block_id=-1,
                    start_time=0.0,
                    stop_time=0.001,
                    cycle_type=0,
                )
                self._selected_channel_idx = -1
                self._selected_key_idx = -1
                self._plot_cache.clear()
            else:
                self._load_property_controllers()
        elif self._sequence and self._sequence.block_id >= 0:
            self._load_sequence(self._sequence.block_id)

    def _active_nif_id(self) -> str:
        registry = getattr(self.app, "registry", None)
        return str(getattr(registry, "active_id", "") or "")

    def _target_display_name(self, target_block_id: int) -> str:
        nif = getattr(self.app, "nif", None)
        if nif is None:
            return ""
        for block in nif.blocks:
            if block.get_field("Shader Property") == target_block_id:
                return str(block.get_field("Name") or f"Block {block.block_id}")
        block = nif.get_block(target_block_id)
        return str(block.get_field("Name") or f"Block {target_block_id}") if block else ""

    def _authoring_target_for_template(self, target_block_id: int, template_id: str) -> AuthoringTarget:
        if template_id.startswith("advanced:"):
            registry = build_controller_registry(get_schema())
            templates = {template.template_id: template for template in build_controller_templates(registry)}
            template = templates.get(template_id)
            if template is None:
                raise ValueError(f"Unknown animation controller template: {template_id}")
            entry = registry[template.chain_specs[0].controller_type]
            target = self._authoring_target_for_kind(target_block_id, entry.target_kind)
            if target is None:
                raise ValueError(f"Selected block is not a valid {entry.target_kind} target")
            return target

        target = self._authoring_target_for_kind(target_block_id, "effect_shader_property")
        if target is None:
            raise ValueError("Friendly effects require a BSEffectShaderProperty target")
        return target

    def _authoring_target_for_kind(self, target_block_id: int, target_kind: str) -> AuthoringTarget | None:
        nif = getattr(self.app, "nif", None)
        block = nif.get_block(target_block_id) if nif is not None and target_block_id >= 0 else None
        if block is None or not self._block_matches_target_kind(block, target_kind):
            return None
        property_type = block.type_name if "shader_property" in target_kind else ""
        return AuthoringTarget(
            block_id=target_block_id,
            display_name=self._target_display_name(target_block_id),
            target_kind=target_kind,
            property_type=property_type,
        )

    def _template_to_chain_spec(
        self,
        target: AuthoringTarget,
        template_id: str,
        link_context: LinkContext,
    ) -> ControllerChainSpec:
        if template_id.startswith("advanced:"):
            registry = build_controller_registry(get_schema())
            templates = {template.template_id: template for template in build_controller_templates(registry)}
            template = templates.get(template_id)
            if template is None:
                raise ValueError(f"Unknown animation controller template: {template_id}")
            if not template.authorable:
                raise ValueError(template.unsupported_reason)
            entry = registry[template.chain_specs[0].controller_type]
            rejection = _advanced_effect_template_rejection(entry, link_context)
            if rejection:
                raise ValueError(rejection)
            value = _default_template_value(entry.value_kind)
            return ControllerChainSpec(
                controller_type=entry.controller_type,
                target=target,
                value_kind=entry.value_kind,
                interpolator_type=entry.interpolator_type,
                data_type=entry.data_type,
                link_context=link_context,
                keys=[(0.0, value), (1.0, value)],
                controlled_fields={},
                sequence_block_id=self._sequence.block_id if self._sequence else -1,
                node_name=target.display_name,
            )

        variable_by_template = {
            "texture_scroll_u": "U Offset",
            "texture_scroll_v": "V Offset",
            "glow_pulse": "EmissiveMultiple",
            "alpha_flicker": "Alpha Transparency",
        }
        if template_id not in variable_by_template:
            raise ValueError(f"Unknown animation effect template: {template_id}")
        variable = variable_by_template[template_id]
        return ControllerChainSpec(
            controller_type="BSEffectShaderPropertyFloatController",
            target=target,
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_context=link_context,
            keys=[(0.0, 0.0), (1.0, 1.0)],
            controlled_fields={"Controlled Variable": variable},
            start_time=0.0,
            stop_time=1.0,
            sequence_block_id=self._sequence.block_id if self._sequence else -1,
            node_name=target.display_name,
        )

    def _current_effect_link_context(self) -> LinkContext:
        if self._sequence is not None and self._sequence.block_id >= 0:
            return LinkContext.SEQUENCE
        return LinkContext.STANDALONE

    def _draw_add_effect_menu(self) -> None:
        link_context = self._current_effect_link_context()
        effect_target_id = self._selected_effect_target_id()
        if effect_target_id < 0:
            imgui.text_disabled("Select a BSEffectShaderProperty or shape for shader effects.")
        else:
            for label, template_id in (
                ("Texture Scroll U", "texture_scroll_u"),
                ("Texture Scroll V", "texture_scroll_v"),
                ("Glow Pulse", "glow_pulse"),
                ("Alpha Flicker", "alpha_flicker"),
            ):
                if imgui.menu_item(label, "", False)[0]:
                    self._add_effect_for_target(effect_target_id, template_id, link_context)
        if imgui.begin_menu("Advanced Controller"):
            registry = build_controller_registry(get_schema())
            for template in build_controller_templates(registry):
                if template.friendly or not template.authorable:
                    continue
                entry = registry[template.chain_specs[0].controller_type]
                if _advanced_effect_template_rejection(entry, link_context):
                    continue
                if entry.target_kind in ("effect_shader_property", "shader_property") and effect_target_id >= 0:
                    target_id = effect_target_id
                else:
                    target_id = self._selected_controller_target_id(entry.target_kind)
                if target_id < 0:
                    continue
                if imgui.menu_item(template.display_name, "", False)[0]:
                    self._add_effect_for_target(target_id, template.template_id, link_context)
            imgui.end_menu()

    def _selected_effect_target_id(self) -> int:
        seq = self._sequence
        nif = getattr(self.app, "nif", None)
        if seq is None or nif is None:
            return -1
        if 0 <= self._selected_channel_idx < len(seq.channels):
            target_id = self._effect_target_id_for_channel(seq.channels[self._selected_channel_idx])
            if target_id >= 0:
                return target_id
        selected_target_id = self._effect_target_id_for_selected_block()
        if selected_target_id >= 0:
            return selected_target_id
        for block in nif.blocks:
            if block.type_name == "BSEffectShaderProperty":
                return block.block_id
        return -1

    def _selected_controller_target_id(self, target_kind: str) -> int:
        seq = self._sequence
        nif = getattr(self.app, "nif", None)
        if seq is None or nif is None:
            return -1
        selected_id = self._selected_block_id()
        target_id = self._controller_target_id_for_block(selected_id, target_kind)
        if target_id >= 0:
            return target_id
        if 0 <= self._selected_channel_idx < len(seq.channels):
            return self._controller_target_id_for_channel(seq.channels[self._selected_channel_idx], target_kind)
        return -1

    def _effect_target_id_for_channel(self, channel: EditableChannel) -> int:
        nif = getattr(self.app, "nif", None)
        if nif is None:
            return -1
        if channel.controller_block_id >= 0:
            controller = nif.get_block(channel.controller_block_id)
            if controller is not None:
                target_id = self._effect_shader_property_target_id(
                    _get_ref_id(controller.get_field("Target"))
                )
                if target_id >= 0:
                    return target_id
        return self._effect_target_id_for_channel_metadata(channel)

    def _controller_target_id_for_channel(self, channel: EditableChannel, target_kind: str) -> int:
        nif = getattr(self.app, "nif", None)
        if nif is None:
            return -1
        if channel.controller_block_id >= 0:
            controller = nif.get_block(channel.controller_block_id)
            if controller is not None:
                target_id = self._controller_target_id_for_block(
                    _get_ref_id(controller.get_field("Target")),
                    target_kind,
                )
                if target_id >= 0:
                    return target_id
        if target_kind == "node" and channel.node_name:
            for block in nif.blocks:
                if str(block.get_field("Name") or "") == channel.node_name:
                    return self._controller_target_id_for_block(block.block_id, target_kind)
        if target_kind in ("effect_shader_property", "shader_property"):
            return self._effect_target_id_for_channel_metadata(channel)
        return -1

    def _effect_target_id_for_channel_metadata(self, channel: EditableChannel) -> int:
        if channel.property_type not in ("", "BSEffectShaderProperty"):
            return -1
        nif = getattr(self.app, "nif", None)
        if nif is None or not channel.node_name:
            return -1
        for block in nif.blocks:
            if str(block.get_field("Name") or "") != channel.node_name:
                continue
            target_id = self._effect_shader_property_target_id(
                _get_ref_id(block.get_field("Shader Property"))
            )
            if target_id >= 0:
                return target_id
        return -1

    def _effect_target_id_for_selected_block(self) -> int:
        return self._controller_target_id_for_block(self._selected_block_id(), "effect_shader_property")

    def _selected_block_id(self) -> int:
        selection_mgr = getattr(self.app, "selection_mgr", None)
        selected_id = getattr(selection_mgr, "selected_block_id", None)
        return int(selected_id) if selected_id is not None else -1

    def _controller_target_id_for_block(self, block_id: int, target_kind: str) -> int:
        nif = getattr(self.app, "nif", None)
        block = nif.get_block(block_id) if nif is not None and block_id >= 0 else None
        if block is None:
            return -1
        if self._block_matches_target_kind(block, target_kind):
            return block.block_id
        if target_kind in ("effect_shader_property", "shader_property"):
            shader_id = _get_ref_id(block.get_field("Shader Property"))
            shader = nif.get_block(shader_id) if shader_id >= 0 else None
            if shader is not None and self._block_matches_target_kind(shader, target_kind):
                return shader.block_id
        if target_kind == "node":
            for candidate in nif.blocks:
                if _get_ref_id(candidate.get_field("Shader Property")) == block.block_id:
                    if self._block_matches_target_kind(candidate, target_kind):
                        return candidate.block_id
        return -1

    def _block_matches_target_kind(self, block, target_kind: str) -> bool:
        nif = getattr(self.app, "nif", None)
        schema = getattr(nif, "schema", None)

        def is_subtype(base_type: str) -> bool:
            if schema is None:
                return False
            try:
                return bool(schema.is_subtype_of(block.type_name, base_type))
            except Exception:
                return False

        if target_kind == "effect_shader_property":
            return block.type_name == "BSEffectShaderProperty"
        if target_kind == "lighting_shader_property":
            return block.type_name == "BSLightingShaderProperty"
        if target_kind == "shader_property":
            return block.type_name in ("BSEffectShaderProperty", "BSLightingShaderProperty") or is_subtype("NiProperty")
        if target_kind == "node":
            return block.type_name in ("NiNode", "BSFadeNode", "BSTriShape", "BSSubIndexTriShape") or is_subtype("NiAVObject")
        if target_kind == "light":
            return "Light" in block.type_name or is_subtype("NiLight")
        if target_kind == "particle_system":
            return "PSys" in block.type_name or block.type_name.startswith(("NiPS", "BSPSys"))
        if target_kind == "geometry":
            return block.type_name in ("BSTriShape", "BSSubIndexTriShape") or is_subtype("NiGeometry")
        if target_kind == "extra_data":
            return "ExtraData" in block.type_name
        return False

    def _effect_shader_property_target_id(self, target_block_id: int) -> int:
        nif = getattr(self.app, "nif", None)
        block = nif.get_block(target_block_id) if nif is not None and target_block_id >= 0 else None
        if block is not None and block.type_name == "BSEffectShaderProperty":
            return target_block_id
        return -1

    def _draw_effects_view(self):
        seq = self._sequence
        if seq is None:
            return

        if imgui.button("+ Effect"):
            imgui.open_popup("##add_effect_popup")
        if imgui.begin_popup("##add_effect_popup"):
            self._draw_add_effect_menu()
            imgui.end_popup()

        stacks = build_effect_stacks(seq)
        if not stacks:
            imgui.text_disabled("No editable non-particle effect channels in this sequence.")
            return

        imgui.text_disabled("Driver / Target / Effect / Timing / Output")
        imgui.separator()
        for stack_index, stack in enumerate(stacks):
            self._draw_effect_stack(stack_index, stack)

    def _draw_effect_stack(self, stack_index: int, stack: EffectStack):
        flags = imgui.TreeNodeFlags_.default_open if stack_index == 0 else 0
        opened = imgui.tree_node_ex(
            f"{stack.effect_type}##effect_stack_{stack_index}",
            flags,
        )
        if not opened:
            return

        if imgui.begin_table(
            f"##effect_stack_meta_{stack_index}",
            2,
            imgui.TableFlags_.sizing_stretch_prop,
        ):
            for label, value in (
                ("Driver", stack.driver),
                ("Target", stack.target_label),
                ("Timing", stack.timing),
                ("Output", stack.output_label),
            ):
                imgui.table_next_row()
                imgui.table_next_column()
                imgui.text_disabled(label)
                imgui.table_next_column()
                imgui.text(value)
            imgui.end_table()

        self._draw_effect_channel_controls(stack_index, stack)
        self._draw_effect_stack_actions(stack)
        imgui.tree_pop()

    def _draw_effect_stack_actions(self, stack: EffectStack) -> None:
        seq = self._sequence
        if seq is None or not stack.channels:
            return
        first_summary = stack.channels[0]
        if not (0 <= first_summary.channel_index < len(seq.channels)):
            return
        channel = seq.channels[first_summary.channel_index]
        if channel.controller_block_id < 0:
            return
        if imgui.small_button(f"Remove##remove_effect_{channel.controller_block_id}"):
            nif = getattr(self.app, "nif", None)
            controller = nif.get_block(channel.controller_block_id) if nif is not None else None
            target_id = _get_ref_id(controller.get_field("Target")) if controller is not None else -1
            if seq.name == "[Property Controllers]" and target_id >= 0:
                self._remove_standalone_effect(target_id, channel.controller_block_id)
            elif seq.block_id >= 0:
                self._remove_sequence_effect(seq.block_id, channel.controller_block_id)

    def _draw_effect_channel_controls(self, stack_index: int, stack: EffectStack):
        seq = self._sequence
        if seq is None:
            return

        flags = (
            imgui.TableFlags_.borders_inner_h
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.sizing_stretch_prop
        )
        if not imgui.begin_table(
            f"##effect_channels_{stack_index}",
            4,
            flags,
        ):
            return

        imgui.table_setup_column("Target", imgui.TableColumnFlags_.none, 1.1)
        imgui.table_setup_column("Control", imgui.TableColumnFlags_.none, 1.0)
        imgui.table_setup_column("Value", imgui.TableColumnFlags_.none, 1.4)
        imgui.table_setup_column("Action", imgui.TableColumnFlags_.width_fixed, 68.0)
        imgui.table_headers_row()

        for summary in stack.channels:
            if not (0 <= summary.channel_index < len(seq.channels)):
                continue
            channel = seq.channels[summary.channel_index]
            imgui.push_id(f"effect_channel_{summary.channel_index}")

            current_min, current_max = self._channel_range(channel)
            slider_bounds = _effect_channel_slider_bounds(current_min, current_max, summary.property_label)

            if channel.controller_block_id >= 0 and channel.controlled_field:
                self._draw_controller_variable_row(summary.channel_index, summary, channel)

            changed_min, new_min = self._draw_effect_slider_row(
                summary.channel_index,
                summary,
                f"{summary.property_label} Min",
                current_min,
                slider_bounds,
                show_curve_button=True,
            )
            changed_max, new_max = self._draw_effect_slider_row(
                summary.channel_index,
                summary,
                f"{summary.property_label} Max",
                current_max,
                slider_bounds,
                show_curve_button=False,
            )

            if changed_min or changed_max:
                self._set_channel_range(channel, float(new_min), float(new_max))
                self._rebuild_plot_cache()
                self._write_back_channel(summary.channel_index)

            if channel.component == "float" and channel.interp_block_id >= 0:
                self._draw_effect_float_shape_row(summary.channel_index, summary, channel)

            if channel.controller_block_id >= 0:
                self._draw_effect_timing_rows(summary.channel_index, channel)
            imgui.pop_id()

        imgui.end_table()

    def _draw_controller_variable_row(
        self,
        channel_index: int,
        summary,
        channel: EditableChannel,
    ) -> None:
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.text(summary.node_name)
        imgui.table_next_column()
        imgui.text_disabled("Controller Variable")
        imgui.table_next_column()
        self._draw_controller_variable_control(channel_index, channel, summary.property_label)
        imgui.table_next_column()
        imgui.text_disabled("")

    def _draw_effect_slider_row(
        self,
        channel_index: int,
        summary,
        label: str,
        current_value: float,
        slider_bounds: tuple[float, float] | None,
        *,
        show_curve_button: bool,
    ) -> tuple[bool, float]:
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.text(summary.node_name if show_curve_button else "")
        imgui.table_next_column()
        imgui.text_disabled(label)
        imgui.table_next_column()
        if slider_bounds is None:
            imgui.text_disabled(_format_effect_channel_range_value(current_value))
            changed_value, new_value = False, current_value
        else:
            slider_min, slider_max = slider_bounds
            imgui.set_next_item_width(-1)
            changed_value, new_value = imgui.slider_float(
                f"##range_{label.lower().replace(' ', '_')}",
                current_value,
                slider_min,
                slider_max,
                "%.4f",
            )

        imgui.table_next_column()
        if show_curve_button:
            if imgui.small_button("Curve"):
                self._select_channel_for_advanced_edit(channel_index)
        else:
            imgui.text_disabled("")

        return changed_value, new_value

    def _draw_effect_float_shape_row(self, channel_index: int, summary, channel: EditableChannel) -> None:
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.text("")
        imgui.table_next_column()
        imgui.text_disabled("NiFloatData")
        imgui.table_next_column()
        data_label = f"{len(channel.keys)} keys" if channel.data_block_id >= 0 else "Inline value"
        imgui.text_disabled(data_label)
        imgui.same_line(0, 12)
        if imgui.small_button("Ramp"):
            self._apply_effect_channel_shape(channel_index, "ramp")
        imgui.same_line(0, 6)
        if imgui.small_button("Pulse"):
            self._apply_effect_channel_shape(channel_index, "pulse")
        imgui.table_next_column()
        imgui.text_disabled("")

    def _draw_effect_timing_rows(self, channel_index: int, channel: EditableChannel) -> None:
        changed_any = False
        values: dict[str, float] = {}
        for field_name, attr_name, _default in _CONTROLLER_TIMING_SPECS:
            changed, new_value = self._draw_controller_float_slider_row(field_name, getattr(channel, attr_name))
            changed_any = changed_any or changed
            values[attr_name] = float(new_value)

        if changed_any:
            start_time = values["start_time"]
            stop_time = max(start_time, values["stop_time"])
            self._write_back_channel_timing(
                channel_index,
                start_time=start_time,
                stop_time=stop_time,
                frequency=max(0.0, values["frequency"]),
                phase=values["phase"],
            )

    def _draw_controller_float_slider_row(self, field_name: str, value: float) -> tuple[bool, float]:
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.text("")
        imgui.table_next_column()
        imgui.text_disabled(field_name)
        imgui.table_next_column()
        bounds = float_field_slider_bounds(field_name, float(value))
        if bounds is None:
            imgui.text_disabled(_format_effect_channel_range_value(value))
            changed, new_value = False, value
        else:
            imgui.set_next_item_width(-1)
            label = f"##{field_name.lower().replace(' ', '_')}"
            changed, new_value = imgui.slider_float(label, float(value), bounds[0], bounds[1], "%.4f")
        imgui.table_next_column()
        imgui.text_disabled("")
        return changed, new_value

    def _draw_controller_variable_control(
        self,
        channel_index: int,
        channel: EditableChannel,
        fallback_label: str,
    ) -> None:
        nif = getattr(self.app, "nif", None)
        controller = nif.get_block(channel.controller_block_id) if nif is not None and channel.controller_block_id >= 0 else None
        if controller is None or not channel.controlled_field:
            imgui.text(fallback_label)
            return
        fdef = self._field_def_for_block(controller, channel.controlled_field)
        schema = getattr(nif, "schema", None)
        enum_def = schema.enums.get(fdef.type) if schema is not None and fdef is not None else None
        if enum_def is None:
            imgui.text(fallback_label)
            return

        option_names, option_values = enum_option_names_and_values(enum_def)
        current_value = controller.get_field(channel.controlled_field)
        current_label = self._controller_field_display_value(controller, channel.controlled_field, current_value)
        if imgui.begin_combo("##controlled_variable", current_label):
            for option_name, option_value in zip(option_names, option_values):
                selected = option_name == current_label or option_value == current_value
                clicked, _ = imgui.selectable(option_name, selected)
                if clicked and not selected:
                    self._write_back_controller_field(channel_index, channel.controlled_field, option_value)
                if selected:
                    imgui.set_item_default_focus()
            imgui.end_combo()

    def _channel_range(self, channel: EditableChannel) -> tuple[float, float]:
        if not channel.keys:
            return 0.0, 0.0
        values = [float(key.value) for key in channel.keys]
        return min(values), max(values)

    def _effect_channel_time_bounds(self, channel: EditableChannel) -> tuple[float, float]:
        start = float(channel.start_time)
        stop = float(channel.stop_time)
        if stop <= start and self._sequence is not None:
            start = float(self._sequence.start_time)
            stop = float(self._sequence.stop_time)
        if stop <= start and channel.keys:
            times = [float(key.time) for key in channel.keys]
            start = min(times)
            stop = max(times)
        if stop <= start:
            stop = start + 1.0
        return start, stop

    def _effect_shape_range(self, channel: EditableChannel) -> tuple[float, float]:
        low, high = self._channel_range(channel)
        if abs(high - low) >= 1.0e-9:
            return low, high
        bounds = _effect_channel_slider_bounds(low, high, channel.target_property)
        if bounds is None:
            return low, high
        slider_min, slider_max = bounds
        if low < slider_max:
            return low, min(slider_max, low + 1.0)
        return max(slider_min, high - 1.0), high

    def _apply_effect_channel_shape(self, channel_index: int, shape: str) -> None:
        if self._sequence is None or not (0 <= channel_index < len(self._sequence.channels)):
            return
        channel = self._sequence.channels[channel_index]
        if channel.component != "float":
            return

        start, stop = self._effect_channel_time_bounds(channel)
        low, high = self._effect_shape_range(channel)
        if shape == "ramp":
            key_specs = ((start, low), (stop, high))
        elif shape == "pulse":
            key_specs = ((start, low), ((start + stop) * 0.5, high), (stop, low))
        else:
            return

        channel.keys = [
            EditableKey(time=float(time), value=float(value), key_type=KEY_LINEAR)
            for time, value in key_specs
        ]
        self._selected_channel_idx = channel_index
        self._selected_key_idx = -1
        self._rebuild_plot_cache()
        self._write_back_channel(channel_index)

    def _set_channel_range(
        self,
        channel: EditableChannel,
        new_min: float,
        new_max: float,
    ) -> None:
        if not channel.keys:
            return
        old_min, old_max = self._channel_range(channel)
        old_span = old_max - old_min
        if abs(old_span) < 1.0e-9:
            new_span = float(new_max) - float(new_min)
            if abs(new_span) < 1.0e-9:
                for key in channel.keys:
                    key.value = float(new_min)
            elif len(channel.keys) == 1:
                start, stop = self._effect_channel_time_bounds(channel)
                channel.keys = [
                    EditableKey(time=start, value=float(new_min), key_type=channel.keys[0].key_type),
                    EditableKey(time=stop, value=float(new_max), key_type=channel.keys[0].key_type),
                ]
            else:
                last_index = len(channel.keys) - 1
                for index, key in enumerate(channel.keys):
                    ratio = index / last_index
                    key.value = float(new_min) + ratio * new_span
            return
        for key in channel.keys:
            ratio = (float(key.value) - old_min) / old_span
            key.value = float(new_min) + ratio * (float(new_max) - float(new_min))

    def _select_channel_for_advanced_edit(self, channel_index: int) -> None:
        if self._sequence is None:
            return
        if 0 <= channel_index < len(self._sequence.channels):
            self._selected_channel_idx = channel_index
            self._selected_key_idx = -1

    def _draw_transport_controls(self):
        """Draw the compact scrub timeline."""
        coord = self._coordinator
        mgr = self._get_active_mgr()
        seq = self._sequence
        if seq is None or (coord is None and mgr is None):
            return

        is_playing, _ = self._get_transport_state(seq.name, mgr)
        start_t = seq.start_time
        stop_t = seq.stop_time
        duration = stop_t - start_t
        has_safe_bounds = (
            math.isfinite(start_t)
            and math.isfinite(stop_t)
            and not is_nif_float_sentinel(start_t)
            and not is_nif_float_sentinel(stop_t)
            and abs(start_t) < _IMGUI_SLIDER_FLOAT_ABS_LIMIT
            and abs(stop_t) < _IMGUI_SLIDER_FLOAT_ABS_LIMIT
        )

        imgui.text_disabled("Time")
        imgui.same_line()
        if not has_safe_bounds:
            imgui.text_disabled(
                f"{_format_effect_channel_range_value(self._playhead_time)}/"
                f"{_format_effect_channel_range_value(stop_t)}s"
            )
            imgui.separator()
            return
        scrub_time = max(start_t, min(self._playhead_time, stop_t))
        if duration <= 0:
            imgui.begin_disabled()
        imgui.set_next_item_width(max(120.0, imgui.get_content_region_avail().x - 110.0))
        changed, scrub_time = imgui.slider_float(
            "##anim_time",
            scrub_time,
            start_t,
            stop_t,
            "%.3f",
        )
        if imgui.is_item_active():
            self._scrub_active_this_frame = True
        if duration <= 0:
            imgui.end_disabled()
        if changed:
            if is_playing:
                if coord:
                    coord.pause()
                elif mgr:
                    mgr.pause()
            self._apply_playhead_time(scrub_time)

        imgui.same_line()
        imgui.text_disabled(f"{self._playhead_time:.3f}/{stop_t:.3f}s")
        imgui.separator()

    def _draw_sound_events(self):
        seq = self._sequence
        if seq is None or seq.name == "[Property Controllers]":
            return

        label = f"Sound Events ({len(seq.sound_events)})"
        expanded = imgui.tree_node_ex(
            label,
            imgui.TreeNodeFlags_.default_open,
        )
        if not expanded:
            return

        has_text_keys = seq.text_keys_block_id >= 0
        if not has_text_keys:
            imgui.text_disabled("No NiTextKeyExtraData block on this sequence.")

        imgui.same_line()
        if not has_text_keys:
            imgui.begin_disabled()
        if imgui.small_button("Add @ Playhead##add_sound"):
            seq.sound_events.append(EditableSoundEvent(
                time=max(seq.start_time, min(self._playhead_time, seq.stop_time)),
                cue="NewSoundDescriptor",
            ))
            seq.sound_events.sort(key=lambda sound_event: sound_event.time)
            self._write_back_sound_events()
        if not has_text_keys:
            imgui.end_disabled()

        if has_text_keys and not seq.sound_events:
            imgui.text_disabled("No sound cues.")
            imgui.tree_pop()
            imgui.separator()
            return

        flags = (
            imgui.TableFlags_.borders_inner_h
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.sizing_stretch_prop
        )
        delete_index = -1
        changed_any = False
        if imgui.begin_table("##sound_events", 3, flags, imgui.ImVec2(0, 0)):
            imgui.table_setup_column("Time", imgui.TableColumnFlags_.width_fixed, 128.0)
            imgui.table_setup_column("Cue", imgui.TableColumnFlags_.none, 1.0)
            imgui.table_setup_column("Actions", imgui.TableColumnFlags_.width_fixed, 78.0)
            imgui.table_headers_row()

            for idx, event in enumerate(seq.sound_events):
                imgui.push_id(f"sound_event_{idx}")
                imgui.table_next_row()

                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed_t, new_time = imgui.input_float(
                    "##time", event.time, 0.01, 0.1, "%.5f"
                )

                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed_cue, new_cue = imgui.input_text("##cue", event.cue, 256)

                imgui.table_next_column()
                if imgui.small_button(">##play"):
                    result = play_sound_cue(event.cue, self.app)
                    if result.error:
                        self.app.status_text = f"Sound unavailable: {event.cue} ({result.error})"
                    else:
                        self.app.status_text = f"Playing sound: {event.cue}"
                imgui.set_item_tooltip("Preview sound")
                imgui.same_line()
                if imgui.small_button("X##remove"):
                    delete_index = idx
                imgui.set_item_tooltip("Remove sound event")

                if changed_t or changed_cue:
                    event.time = max(seq.start_time, min(float(new_time), seq.stop_time))
                    event.cue = str(new_cue).strip()
                    changed_any = True
                imgui.pop_id()

            imgui.end_table()

        if changed_any:
            seq.sound_events.sort(key=lambda sound_event: sound_event.time)
            self._write_back_sound_events()

        if delete_index >= 0:
            seq.sound_events.pop(delete_index)
            if has_text_keys:
                self._write_back_sound_events()

        imgui.tree_pop()
        imgui.separator()

    def _draw_sequence_nav(self):
        """Draw a compact animation toolbar: sequence, transport, loop, speed."""
        names = self._get_sequence_names()
        if not names:
            return

        cur_name = self._sequence.name if self._sequence else names[0]
        cur_idx = names.index(cur_name) if cur_name in names else 0
        mgr = self._get_active_mgr()
        coord = self._coordinator
        is_playing, is_paused = self._get_transport_state(cur_name, mgr)

        def _icon(name: str, fallback: str) -> str:
            return getattr(fa, name, fallback)

        def _btn(label: str, tooltip: str, *, enabled: bool = True, active: bool = False) -> bool:
            visible, sep, ident = label.partition("##")
            draw_label = label
            if sep and any(ord(ch) > 127 for ch in visible):
                draw_label = f"{visible} ##{ident}"
            if active:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.18, 0.38, 0.58, 1.0)
                )
            if not enabled:
                imgui.begin_disabled()
            clicked = imgui.button(draw_label, imgui.ImVec2(30, 0))
            if not enabled:
                imgui.end_disabled()
            if active:
                imgui.pop_style_color()
            imgui.set_item_tooltip(tooltip)
            return clicked and enabled

        if _btn("<##seq_prev", "Previous animation", enabled=cur_idx > 0):
            self._select_sequence_by_name(names[cur_idx - 1])
        imgui.same_line()

        combo_w = min(240.0, max(120.0, imgui.get_content_region_avail().x - 285.0))
        imgui.set_next_item_width(combo_w)
        changed, new_idx = imgui.combo("##seq_nav", cur_idx, names)
        if changed and 0 <= new_idx < len(names):
            self._select_sequence_by_name(names[new_idx])
        imgui.same_line()

        if _btn(">##seq_next", "Next animation", enabled=cur_idx < len(names) - 1):
            self._select_sequence_by_name(names[cur_idx + 1])
        imgui.same_line()

        imgui.text_disabled(f"{cur_idx + 1}/{len(names)}")
        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        play_icon = _icon("ICON_FA_PAUSE", "||") if is_playing else _icon("ICON_FA_PLAY", ">")
        if _btn(f"{play_icon}##anim_play_pause", "Play / pause animation", enabled=cur_name != ""):
            if is_playing:
                if coord:
                    coord.pause()
                elif mgr:
                    mgr.pause()
            elif is_paused:
                if coord:
                    coord.resume()
                elif mgr:
                    mgr.resume()
            else:
                if coord:
                    coord.play(cur_name)
                elif mgr:
                    mgr.play(cur_name)
        imgui.same_line()

        if _btn(f"{_icon('ICON_FA_STOP', 'Stop')}##anim_stop", "Stop animation"):
            if coord:
                coord.stop()
            elif mgr:
                mgr.stop()
            if self._sequence:
                self._playhead_time = self._sequence.start_time
        imgui.same_line()

        mute_on = bool(mgr and mgr.sound_muted)
        mute_icon = _icon("ICON_FA_VOLUME_XMARK", "M") if mute_on else _icon("ICON_FA_VOLUME_HIGH", "S")
        if _btn(f"{mute_icon}##anim_mute", "Mute animation sounds", enabled=mgr is not None, active=mute_on):
            self._set_sound_muted(not mute_on)
        imgui.same_line()

        loop_on = bool(mgr and mgr.loop)
        if _btn(f"{_icon('ICON_FA_REPEAT', 'Loop')}##anim_loop", "Toggle loop", active=loop_on):
            self._set_loop(not loop_on)
        imgui.same_line()

        speed = mgr.speed if mgr else 1.0
        if _btn("-##anim_speed_down", "Slow down", enabled=mgr is not None):
            self._set_speed(speed - 0.25)
        imgui.same_line()
        if imgui.button(f"{speed:.2f}x##anim_speed_reset", imgui.ImVec2(54, 0)):
            self._set_speed(1.0)
        imgui.set_item_tooltip("Reset speed to 1.00x")
        imgui.same_line()
        if _btn("+##anim_speed_up", "Speed up", enabled=mgr is not None):
            self._set_speed(speed + 0.25)

        if is_playing or is_paused:
            imgui.same_line()
            color = imgui.ImVec4(0.4, 1.0, 0.4, 1.0) if is_playing else imgui.ImVec4(1.0, 0.8, 0.3, 1.0)
            imgui.text_colored(color, "Playing" if is_playing else "Paused")

    def _draw_curve_options(self):
        seq = self._sequence
        if seq is None:
            return

        imgui.text_disabled("Graph")
        imgui.same_line()
        _, self._show_context_curves = imgui.checkbox("Context curves", self._show_context_curves)
        imgui.same_line()
        _, self._show_legend = imgui.checkbox("Legend", self._show_legend)

        if 0 <= self._selected_channel_idx < len(seq.channels):
            ch = seq.channels[self._selected_channel_idx]
            imgui.same_line()
            imgui.text_colored(imgui.ImVec4(*ch.color), f"Editing: {ch.label}")
        else:
            imgui.same_line()
            imgui.text_disabled("Select a channel to edit")

        imgui.separator()

    def _get_sequence_names(self) -> list[str]:
        coord = self._coordinator
        if coord:
            return list(coord.get_all_sequences().keys())

        mgr = self._get_active_mgr()
        if mgr:
            names = mgr.get_sequences()
        else:
            nif = getattr(self.app, "nif", None)
            if nif is None:
                names = []
            else:
                names = [
                    self._get_string(b, "Name") or f"Sequence_{b.block_id}"
                    for b in nif.blocks
                    if b is not None and b.type_name == "NiControllerSequence"
                ]

        session = getattr(getattr(self.app, "registry", None), "active_session", None)
        particle_runtime = getattr(session, "particle_runtime", None) if session else None
        if particle_runtime is not None and getattr(particle_runtime, "has_particles", False):
            names = list(names) + [PARTICLE_PREVIEW_SEQUENCE]
        return names

    def _get_particle_runtimes(self):
        registry = getattr(self.app, "registry", None)
        if registry is None:
            return []
        try:
            sessions = registry.all_sessions()
        except (KeyError, AttributeError):
            return []
        runtimes = []
        for session in sessions:
            runtime = getattr(session, "particle_runtime", None)
            if runtime is not None and getattr(runtime, "has_particles", False):
                runtimes.append(runtime)
        return runtimes

    def _get_transport_state(self, sequence_name: str, mgr) -> tuple[bool, bool]:
        if sequence_name == PARTICLE_PREVIEW_SEQUENCE:
            runtimes = self._get_particle_runtimes()
            if any(bool(getattr(runtime, "is_paused", False)) for runtime in runtimes):
                return False, True
            if any(bool(getattr(runtime, "is_playing", False)) for runtime in runtimes):
                return True, False
            if runtimes:
                return False, False
        return (
            bool(mgr and mgr.is_playing),
            bool(mgr and mgr.is_paused and mgr.current_sequence is not None),
        )

    def _select_sequence_by_name(self, seq_name: str):
        self.open_for_sequence_name(seq_name)
        if self._sequence and self._sequence.name == seq_name:
            self._sync_panel_selection(seq_name)
            self._apply_playhead_time(self._sequence.start_time)

    def _apply_playhead_time(self, t: float):
        seq = self._sequence
        if seq is None:
            return

        t = max(seq.start_time, min(float(t), seq.stop_time))
        self._playhead_time = t
        self._sync_panel_selection(seq.name)

        coord = self._coordinator
        mgr = self._get_active_mgr()
        if coord:
            coord.set_time(t)
        elif mgr:
            mgr.set_time(t, getattr(self.app, 'nif_root', None))
        self._flush_animation_pose_to_viewport()

    def _flush_animation_pose_to_viewport(self):
        try:
            import glm
            from creation_lib.renderer.nif_loader import _update_world_transforms
        except Exception:
            return

        registry = getattr(self.app, 'registry', None)
        root = None
        if registry and getattr(registry, 'sessions', None):
            main = registry.sessions.get(getattr(registry, 'main_id', 'main'))
            if main is not None:
                root = main.scene_root
            else:
                try:
                    root = registry.active_session.scene_root
                except (KeyError, AttributeError):
                    root = None
        else:
            root = getattr(self.app, 'nif_root', None)

        if root is not None:
            _update_world_transforms(root, glm.mat4(1.0))
            renderer = getattr(self.app, 'renderer', None)
            if renderer is not None:
                renderer._collision_dirty = True

    def _set_scrub_realtime(self, enabled: bool):
        if self._scrub_realtime == enabled:
            return
        try:
            params = hello_imgui.get_runner_params()
        except Exception:
            return

        if enabled:
            params.fps_idling.enable_idling = False
        else:
            mgr = self._get_active_mgr()
            if not (mgr and mgr.is_playing):
                params.fps_idling.enable_idling = True
        self._scrub_realtime = enabled

    def _set_speed(self, speed: float):
        speed = max(0.1, min(float(speed), 5.0))
        registry = getattr(self.app, 'registry', None)
        if registry:
            for s in registry.all_sessions():
                s.anim_manager.speed = speed
            return

        mgr = self._get_active_mgr()
        if mgr:
            mgr.speed = speed

    def _set_loop(self, enabled: bool):
        registry = getattr(self.app, 'registry', None)
        if registry:
            for s in registry.all_sessions():
                s.anim_manager.loop = enabled
            return

        mgr = self._get_active_mgr()
        if mgr:
            mgr.loop = enabled

    def _set_sound_muted(self, enabled: bool):
        registry = getattr(self.app, 'registry', None)
        if registry:
            for s in registry.all_sessions():
                s.anim_manager.sound_muted = enabled
            return

        mgr = self._get_active_mgr()
        if mgr:
            mgr.sound_muted = enabled

    def _draw_channel_list(self):
        """Draw the channel tree grouped by node name."""
        if self._sequence is None:
            return

        # Group channels by node name
        node_channels: dict[str, list[tuple[int, EditableChannel]]] = {}
        for i, ch in enumerate(self._sequence.channels):
            node_channels.setdefault(ch.node_name, []).append((i, ch))

        for node_name, ch_list in node_channels.items():
            expanded = self._node_expanded.get(node_name, False)
            flags = imgui.TreeNodeFlags_.default_open if expanded else 0
            is_open = imgui.tree_node_ex(f"{node_name}##node", flags)
            self._node_expanded[node_name] = is_open

            if is_open:
                for idx, ch in ch_list:
                    imgui.push_id(f"channel_{idx}")
                    changed, new_val = imgui.checkbox(
                        "##visible",
                        ch.visible,
                    )
                    if changed:
                        ch.visible = new_val
                        self._rebuild_plot_cache()

                    imgui.same_line()
                    imgui.text_colored(imgui.ImVec4(*ch.color), "*")
                    imgui.same_line()
                    label = f"{_COMPONENT_LABELS.get(ch.component, ch.component)} ({len(ch.keys)})"
                    clicked, _ = imgui.selectable(label, idx == self._selected_channel_idx)
                    if clicked:
                        self._selected_channel_idx = idx
                        self._selected_key_idx = -1
                    imgui.pop_id()

                imgui.tree_pop()

    def _draw_curve_view(self):
        """Draw the ImPlot curve area with channels and playhead."""
        if self._sequence is None:
            return

        seq = self._sequence
        avail = imgui.get_content_region_avail()
        if not (0 <= self._selected_channel_idx < len(seq.channels)) and seq.channels:
            self._selected_channel_idx = 0

        plot_flags = 0 if self._show_legend else implot.Flags_.no_legend
        if implot.begin_plot("##curves_plot", imgui.ImVec2(avail.x, avail.y), flags=plot_flags):
            implot.setup_axes("Time (s)", "Value")
            implot.setup_axis_limits(implot.ImAxis_.x1, seq.start_time, seq.stop_time, implot.Cond_.once)

            for i, ch in enumerate(seq.channels):
                if not ch.visible:
                    continue
                is_selected = i == self._selected_channel_idx
                if not is_selected and not self._show_context_curves:
                    continue
                cache = self._plot_cache.get(i)
                if cache is None:
                    continue
                xs, ys = cache

                alpha = 1.0 if is_selected else 0.28
                color = imgui.ImVec4(ch.color[0], ch.color[1], ch.color[2], alpha)
                line_spec = implot.Spec()
                line_spec.line_color = color
                line_spec.line_weight = 2.4 if is_selected else 1.0
                label = ch.label if (self._show_legend or is_selected) else f"##context_{i}"
                implot.plot_line(label, xs, ys, line_spec)

                if self._show_context_curves and not is_selected and ch.keys:
                    key_times = np.array([k.time for k in ch.keys], dtype=np.float64)
                    key_vals = np.array([k.value for k in ch.keys], dtype=np.float64)
                    scatter_spec = implot.Spec()
                    scatter_spec.marker = implot.Marker_.diamond
                    scatter_spec.marker_size = 4.0
                    scatter_spec.marker_fill_color = color
                    scatter_spec.marker_line_color = color
                    implot.plot_scatter(f"##{ch.label}_keys", key_times, key_vals, scatter_spec)

            # Draggable key handles for the selected channel
            if (0 <= self._selected_channel_idx < len(seq.channels)):
                sel_ch = seq.channels[self._selected_channel_idx]
                if sel_ch.visible:
                    sel_color = imgui.ImVec4(*sel_ch.color)
                    highlight = imgui.ImVec4(1.0, 1.0, 1.0, 1.0)
                    for ki, key in enumerate(sel_ch.keys):
                        kx, ky = float(key.time), float(key.value)
                        pt_color = highlight if ki == self._selected_key_idx else sel_color
                        drag_changed, kx, ky, clicked, _, _ = implot.drag_point(
                            ki, kx, ky, pt_color, size=5.0,
                        )
                        if clicked:
                            self._selected_key_idx = ki
                        if drag_changed:
                            self._selected_key_idx = ki
                            key.time = max(seq.start_time, min(float(kx), seq.stop_time))
                            key.value = float(ky)
                            sel_ch.keys.sort(key=lambda k: k.time)
                            for new_ki, k in enumerate(sel_ch.keys):
                                if k is key:
                                    self._selected_key_idx = new_ki
                                    break
                            self._rebuild_plot_cache()
                            self._write_back_channel(self._selected_channel_idx)

            # Draggable playhead
            playhead_color = imgui.ImVec4(1.0, 0.8, 0.0, 0.9)
            changed, new_time, _, _, _ = implot.drag_line_x(
                0, self._playhead_time, playhead_color,
                thickness=2.0,
            )
            if changed:
                self._scrub_active_this_frame = True
                self._apply_playhead_time(new_time)

            # Click to select nearest keyframe
            if implot.is_plot_hovered() and imgui.is_mouse_clicked(imgui.MouseButton_.left):
                mouse_pos = implot.get_plot_mouse_pos()
                self._pick_nearest_key(mouse_pos.x, mouse_pos.y)

            implot.end_plot()

    def _pick_nearest_key(self, mouse_t: float, mouse_v: float):
        """Find the nearest keyframe to the mouse position in plot coords."""
        if self._sequence is None:
            return

        seq = self._sequence
        best_dist = float('inf')
        best_ch = -1
        best_k = -1

        # Normalize distances — time range vs value range
        t_range = max(seq.stop_time - seq.start_time, 1e-6)

        for i, ch in enumerate(seq.channels):
            if not ch.visible or not ch.keys:
                continue
            for j, k in enumerate(ch.keys):
                # Simple normalized distance
                dt = (k.time - mouse_t) / t_range
                dv = k.value - mouse_v
                dist = dt * dt + dv * dv * 0.01  # weight time more
                if dist < best_dist:
                    best_dist = dist
                    best_ch = i
                    best_k = j

        # Only select if reasonably close
        if best_dist < 0.05:
            self._selected_channel_idx = best_ch
            self._selected_key_idx = best_k

    def _draw_detail_bar(self):
        """Draw the bottom detail bar with key editing controls."""
        seq = self._sequence
        if seq is None:
            return

        has_selection = (
            0 <= self._selected_channel_idx < len(seq.channels)
            and 0 <= self._selected_key_idx < len(seq.channels[self._selected_channel_idx].keys)
        )

        if has_selection:
            ch = seq.channels[self._selected_channel_idx]
            key = ch.keys[self._selected_key_idx]

            imgui.text_colored(imgui.ImVec4(*ch.color), f"Editing {ch.label}")
            imgui.same_line(0, 20)

            imgui.set_next_item_width(80)
            changed_t, new_t = imgui.input_float("Time##key_t", key.time, 0.001, 0.01, "%.4f")

            imgui.same_line()
            imgui.set_next_item_width(100)
            changed_v, new_v = imgui.input_float("Value##key_v", key.value, 0.01, 0.1, "%.4f")

            imgui.same_line()
            imgui.set_next_item_width(100)
            current_label = _KEY_TYPE_NAMES.get(key.key_type, "Unknown")
            if imgui.begin_combo("Type##key_type", current_label):
                for kt, kt_name in _KEY_TYPE_NAMES.items():
                    selected = kt == key.key_type
                    if imgui.selectable(kt_name, selected)[0]:
                        key.key_type = kt
                    if selected:
                        imgui.set_item_default_focus()
                imgui.end_combo()

            if changed_t or changed_v:
                if changed_t:
                    key.time = max(seq.start_time, min(float(new_t), seq.stop_time))
                    ch.keys.sort(key=lambda k: k.time)
                    for idx, k in enumerate(ch.keys):
                        if k is key:
                            self._selected_key_idx = idx
                            break
                if changed_v:
                    key.value = new_v
                self._write_back_channel(self._selected_channel_idx)
        else:
            if 0 <= self._selected_channel_idx < len(seq.channels):
                ch = seq.channels[self._selected_channel_idx]
                imgui.text_colored(imgui.ImVec4(*ch.color), f"Editing {ch.label}")
                imgui.same_line()
                imgui.text_disabled("Click or drag a key to edit its time/value.")
            else:
                imgui.text_disabled("Select a channel to edit.")

        if not (0 <= self._selected_channel_idx < len(seq.channels)):
            imgui.begin_disabled()
        if imgui.button("Add Key"):
            self._add_key_at_playhead()
        if not (0 <= self._selected_channel_idx < len(seq.channels)):
            imgui.end_disabled()

        imgui.same_line()
        if not has_selection:
            imgui.begin_disabled()
        if imgui.button("Delete Key"):
            self._delete_selected_key()
        if not has_selection:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button("Normalize Quat"):
            self._normalize_quaternion()

    # -- Keyframe operations --

    def _add_key_at_playhead(self):
        """Add a keyframe at the current playhead time for the selected channel."""
        if self._sequence is None or self._selected_channel_idx < 0:
            return

        ch = self._sequence.channels[self._selected_channel_idx]
        t = self._playhead_time

        # Check if key already exists at this time
        for k in ch.keys:
            if abs(k.time - t) < 1e-6:
                return

        # Interpolate value
        value = _lerp_keys(ch.keys, t)
        new_key = EditableKey(time=t, value=value, key_type=KEY_LINEAR)
        ch.keys.append(new_key)
        ch.keys.sort(key=lambda k: k.time)

        # Select the new key
        for idx, k in enumerate(ch.keys):
            if k is new_key:
                self._selected_key_idx = idx
                break

        self._write_back_channel(self._selected_channel_idx)

    def _delete_selected_key(self):
        """Delete the currently selected keyframe."""
        if self._sequence is None:
            return
        if not (0 <= self._selected_channel_idx < len(self._sequence.channels)):
            return

        ch = self._sequence.channels[self._selected_channel_idx]
        if not (0 <= self._selected_key_idx < len(ch.keys)):
            return

        # Don't delete the last key
        if len(ch.keys) <= 1:
            return

        ch.keys.pop(self._selected_key_idx)
        self._selected_key_idx = min(self._selected_key_idx, len(ch.keys) - 1)
        self._write_back_channel(self._selected_channel_idx)

    def _normalize_quaternion(self):
        """Normalize quaternion channels for the selected channel's node."""
        if self._sequence is None or self._selected_channel_idx < 0:
            return

        ch = self._sequence.channels[self._selected_channel_idx]
        if not ch.component.startswith("rot_"):
            return

        # Find all rot channels for this node + data block
        rot_channels = {}
        for i, c in enumerate(self._sequence.channels):
            if c.node_name == ch.node_name and c.data_block_id == ch.data_block_id and c.component.startswith("rot_"):
                rot_channels[c.component] = c

        if len(rot_channels) < 4:
            return

        # Normalize at each key time
        rw = rot_channels.get("rot_w")
        rx = rot_channels.get("rot_x")
        ry = rot_channels.get("rot_y")
        rz = rot_channels.get("rot_z")
        if not (rw and rx and ry and rz):
            return

        # All rot channels should have same number of keys
        for j in range(len(rw.keys)):
            if j >= len(rx.keys) or j >= len(ry.keys) or j >= len(rz.keys):
                break
            w, x, y, z = rw.keys[j].value, rx.keys[j].value, ry.keys[j].value, rz.keys[j].value
            mag = math.sqrt(w*w + x*x + y*y + z*z)
            if mag > 1e-9:
                rw.keys[j].value = w / mag
                rx.keys[j].value = x / mag
                ry.keys[j].value = y / mag
                rz.keys[j].value = z / mag

        # Write back all rot channels
        for i, c in enumerate(self._sequence.channels):
            if c.node_name == ch.node_name and c.data_block_id == ch.data_block_id and c.component.startswith("rot_"):
                self._write_back_channel(i)
                break  # Only need to write once since they share the same data block

    # -- Write-back to NIF --

    def _write_back_controller_field(self, channel_idx: int, field_name: str, new_value) -> None:
        if self._sequence is None or not (0 <= channel_idx < len(self._sequence.channels)):
            return
        ch = self._sequence.channels[channel_idx]
        nif = getattr(self.app, "nif", None)
        undo_manager = getattr(self.app, "undo_manager", None)
        nif_id = self._active_nif_id()
        if nif is None or undo_manager is None or nif_id == "" or ch.controller_block_id < 0:
            return
        controller = nif.get_block(ch.controller_block_id)
        if controller is None:
            return

        old_value = controller.get_field(field_name)
        if old_value == new_value:
            return

        from ui.editor.undo import SetFieldAction

        action = SetFieldAction(
            block_id=controller.block_id,
            field_name=field_name,
            old_value=old_value,
            new_value=new_value,
            _description=f"Edit controller field: {field_name}",
        )
        action.execute(nif)
        undo_manager.push(nif_id, action)

        if field_name in _CONTROLLED_CONTROLLER_FIELDS:
            ch.controlled_field = field_name
            ch.target_property = self._resolve_controller_property(ch.controller_type, None, controller)
            ch.label = self._channel_label(ch)
            self._rebuild_plot_cache()

        registry = getattr(self.app, "registry", None)
        session = getattr(registry, "active_session", None) if registry else None
        manager = getattr(session, "anim_manager", None) if session else None
        if manager is not None:
            manager.scan(nif)

    def _write_back_channel_timing(
        self,
        channel_idx: int,
        *,
        start_time: float,
        stop_time: float,
        frequency: float,
        phase: float,
    ) -> None:
        if self._sequence is None or not (0 <= channel_idx < len(self._sequence.channels)):
            return
        ch = self._sequence.channels[channel_idx]
        nif = getattr(self.app, "nif", None)
        undo_manager = getattr(self.app, "undo_manager", None)
        nif_id = self._active_nif_id()
        if nif is None or undo_manager is None or nif_id == "" or ch.controller_block_id < 0:
            return
        controller = nif.get_block(ch.controller_block_id)
        if controller is None:
            return

        from ui.editor.undo import SetFieldAction, CompositeAction

        new_timing = {
            "start_time": start_time,
            "stop_time": stop_time,
            "frequency": frequency,
            "phase": phase,
        }
        timing_values = {
            field_name: float(new_timing[attr_name])
            for field_name, attr_name, _default in _CONTROLLER_TIMING_SPECS
        }
        actions = [
            SetFieldAction(
                block_id=controller.block_id,
                field_name=field_name,
                old_value=controller.get_field(field_name),
                new_value=new_value,
            )
            for field_name, new_value in timing_values.items()
            if controller.get_field(field_name) != new_value
        ]

        seq = self._sequence
        seq_block = nif.get_block(seq.block_id) if seq.block_id >= 0 else None
        if seq_block is not None:
            expanded_start = min(float(seq_block.get_field("Start Time") or 0.0), float(start_time))
            expanded_stop = max(float(seq_block.get_field("Stop Time") or 0.0), float(stop_time))
            if seq_block.get_field("Start Time") != expanded_start:
                actions.append(SetFieldAction(seq.block_id, "Start Time", seq_block.get_field("Start Time"), expanded_start))
            if seq_block.get_field("Stop Time") != expanded_stop:
                actions.append(SetFieldAction(seq.block_id, "Stop Time", seq_block.get_field("Stop Time"), expanded_stop))

        if not actions:
            return

        composite = CompositeAction(
            children=actions,
            _description=f"Edit animation timing: {ch.label}",
        )
        composite.execute(nif)
        undo_manager.push(nif_id, composite)

        for _field_name, attr_name, _default in _CONTROLLER_TIMING_SPECS:
            setattr(ch, attr_name, float(new_timing[attr_name]))
        if seq_block is not None:
            seq.start_time = float(seq_block.get_field("Start Time") or seq.start_time)
            seq.stop_time = float(seq_block.get_field("Stop Time") or seq.stop_time)

        registry = getattr(self.app, "registry", None)
        session = getattr(registry, "active_session", None) if registry else None
        manager = getattr(session, "anim_manager", None) if session else None
        if manager is not None:
            manager.scan(nif)
        self._rebuild_plot_cache()

    def _write_back_channel(self, channel_idx: int):
        """Write modified channel data back to the NIF via the undo system."""
        if self._sequence is None:
            return
        if not (0 <= channel_idx < len(self._sequence.channels)):
            return

        ch = self._sequence.channels[channel_idx]
        nif = getattr(self.app, "nif", None)
        if nif is None:
            return

        data_block = nif.get_block(ch.data_block_id) if ch.data_block_id >= 0 else None
        if data_block is None:
            if ch.component == "float":
                self._write_back_materialized_float_channel(channel_idx)
            return

        from ui.editor.undo import SetFieldAction, CompositeAction

        actions = []

        if ch.component.startswith("pos_"):
            actions = self._build_position_write_back(data_block, ch)
        elif ch.component.startswith("rot_"):
            actions = self._build_rotation_write_back(data_block, ch)
        elif ch.component == "scale":
            actions = self._build_scale_write_back(data_block, ch)
        elif ch.component == "float":
            actions = self._build_float_write_back(data_block, ch)
        elif ch.component == "bool":
            actions = self._build_bool_write_back(data_block, ch)

        if not actions:
            return

        composite = CompositeAction(
            children=actions,
            _description=f"Edit animation keyframes: {ch.label}",
        )
        composite.execute(nif)
        self.app.undo_manager.push(self.app.registry.active_id, composite)

        # Mark dirty and rescan the active session's animation manager
        session = self.app.registry.active_session
        if session:
            session.anim_manager.scan(nif)

        # Rebuild editor data
        self._rebuild_plot_cache()

    def _write_back_materialized_float_channel(self, channel_idx: int) -> None:
        if self._sequence is None or not (0 <= channel_idx < len(self._sequence.channels)):
            return
        ch = self._sequence.channels[channel_idx]
        nif = getattr(self.app, "nif", None)
        undo_manager = getattr(self.app, "undo_manager", None)
        nif_id = self._active_nif_id()
        if nif is None or undo_manager is None or nif_id == "" or ch.interp_block_id < 0:
            return
        interp_block = nif.get_block(ch.interp_block_id)
        if interp_block is None or "FloatInterpolator" not in interp_block.type_name:
            return

        from ui.editor.undo import SnapshotAction

        cmd = SnapshotAction(_description=f"Create NiFloatData: {ch.label}")
        cmd.capture_before(nif)
        data_block = nif.add_block("NiFloatData", {"Data": self._float_data_from_channel(ch)})
        interp_block.set_field("Data", data_block.block_id)
        cmd.capture_after(nif)
        undo_manager.push(nif_id, cmd)

        ch.data_block_id = data_block.block_id
        registry = getattr(self.app, "registry", None)
        session = getattr(registry, "active_session", None) if registry else None
        manager = getattr(session, "anim_manager", None) if session else None
        if manager is not None:
            manager.scan(nif)
        self._rebuild_plot_cache()

    def _write_back_sound_events(self):
        if self._sequence is None:
            return
        seq = self._sequence
        nif = getattr(self.app, "nif", None)
        if nif is None or seq.text_keys_block_id < 0:
            return

        text_block = nif.get_block(seq.text_keys_block_id)
        if text_block is None or text_block.type_name != "NiTextKeyExtraData":
            return

        old_keys = copy.deepcopy(text_block.get_field("Text Keys") or [])
        preserved = [
            entry for entry in old_keys
            if not (isinstance(entry, dict) and parse_sound_text_key(entry.get("Value")))
        ]
        sound_keys = [
            {
                "Time": float(event.time),
                "Value": format_sound_text_key(event.cue),
            }
            for event in seq.sound_events
            if event.cue.strip()
        ]
        new_keys = sorted(
            [*preserved, *sound_keys],
            key=lambda entry: float(entry.get("Time", 0.0)) if isinstance(entry, dict) else 0.0,
        )

        from ui.editor.undo import SetFieldAction, CompositeAction

        actions = [
            SetFieldAction(
                block_id=text_block.block_id,
                field_name="Num Text Keys",
                old_value=text_block.get_field("Num Text Keys"),
                new_value=len(new_keys),
            ),
            SetFieldAction(
                block_id=text_block.block_id,
                field_name="Text Keys",
                old_value=old_keys,
                new_value=new_keys,
            ),
        ]
        composite = CompositeAction(
            children=actions,
            _description=f"Edit animation sound events: {seq.name}",
        )
        composite.execute(nif)
        self.app.undo_manager.push(self.app.registry.active_id, composite)

        session = self.app.registry.active_session
        if session:
            session.anim_manager.scan(nif)

        reloaded = self._load_sequence_from_nif(nif, seq.block_id)
        if reloaded is not None:
            self._sequence = reloaded
            self._rebuild_plot_cache()

    def _build_position_write_back(self, data_block, changed_ch: EditableChannel) -> list:
        """Rebuild the Translations key array from all pos_x/y/z channels."""
        from ui.editor.undo import SetFieldAction

        old_translations = copy.deepcopy(data_block.get_field("Translations"))
        if not old_translations or not isinstance(old_translations, dict):
            return []

        # Collect all position channels for this data block
        pos_channels = {}
        for c in self._sequence.channels:
            if c.data_block_id == changed_ch.data_block_id and c.component.startswith("pos_"):
                pos_channels[c.component] = c

        # Build new keys from the channel data
        # All pos channels share the same times (they came from Vector3 keys)
        ref_ch = pos_channels.get("pos_x") or pos_channels.get("pos_y") or pos_channels.get("pos_z")
        if ref_ch is None:
            return []

        new_keys = []
        for i in range(len(ref_ch.keys)):
            x = pos_channels["pos_x"].keys[i].value if "pos_x" in pos_channels and i < len(pos_channels["pos_x"].keys) else 0.0
            y = pos_channels["pos_y"].keys[i].value if "pos_y" in pos_channels and i < len(pos_channels["pos_y"].keys) else 0.0
            z = pos_channels["pos_z"].keys[i].value if "pos_z" in pos_channels and i < len(pos_channels["pos_z"].keys) else 0.0
            k = ref_ch.keys[i]

            new_key = {
                "Time": k.time,
                "Value": {"x": x, "y": y, "z": z},
                "Interpolation": k.key_type,
            }
            # Preserve tangent data for quadratic keys
            if k.key_type == KEY_QUADRATIC:
                fx = pos_channels["pos_x"].keys[i].forward if "pos_x" in pos_channels and i < len(pos_channels["pos_x"].keys) else 0.0
                fy = pos_channels["pos_y"].keys[i].forward if "pos_y" in pos_channels and i < len(pos_channels["pos_y"].keys) else 0.0
                fz = pos_channels["pos_z"].keys[i].forward if "pos_z" in pos_channels and i < len(pos_channels["pos_z"].keys) else 0.0
                bx = pos_channels["pos_x"].keys[i].backward if "pos_x" in pos_channels and i < len(pos_channels["pos_x"].keys) else 0.0
                by = pos_channels["pos_y"].keys[i].backward if "pos_y" in pos_channels and i < len(pos_channels["pos_y"].keys) else 0.0
                bz = pos_channels["pos_z"].keys[i].backward if "pos_z" in pos_channels and i < len(pos_channels["pos_z"].keys) else 0.0
                new_key["Forward"] = {"x": fx, "y": fy, "z": fz}
                new_key["Backward"] = {"x": bx, "y": by, "z": bz}
            new_keys.append(new_key)

        new_translations = copy.deepcopy(old_translations)
        new_translations["Num Keys"] = len(new_keys)
        new_translations["Keys"] = new_keys

        return [SetFieldAction(
            block_id=data_block.block_id,
            field_name="Translations",
            old_value=old_translations,
            new_value=new_translations,
        )]

    def _build_rotation_write_back(self, data_block, changed_ch: EditableChannel) -> list:
        """Rebuild the rotation key array from the visible rotation channels."""
        from ui.editor.undo import SetFieldAction

        old_xyz_rotations = copy.deepcopy(data_block.get_field("XYZ Rotations"))
        if old_xyz_rotations and isinstance(old_xyz_rotations, list):
            rot_channels = {}
            for c in self._sequence.channels:
                if c.data_block_id == changed_ch.data_block_id and c.component in {"rot_x", "rot_y", "rot_z"}:
                    rot_channels[c.component] = c

            if not rot_channels:
                return []

            new_xyz_rotations = copy.deepcopy(old_xyz_rotations)
            for axis_index, comp in enumerate(("rot_x", "rot_y", "rot_z")):
                if axis_index >= len(new_xyz_rotations):
                    continue
                axis_group = new_xyz_rotations[axis_index]
                if not isinstance(axis_group, dict):
                    continue
                ch = rot_channels.get(comp)
                if ch is None:
                    continue
                new_keys = []
                for key in ch.keys:
                    new_key = {
                        "Time": key.time,
                        "Value": key.value,
                    }
                    key_type = int(axis_group.get("Interpolation", key.key_type))
                    if key_type == KEY_QUADRATIC:
                        new_key["Forward"] = key.forward
                        new_key["Backward"] = key.backward
                    new_keys.append(new_key)
                axis_group["Num Keys"] = len(new_keys)
                axis_group["Keys"] = new_keys

            return [SetFieldAction(
                block_id=data_block.block_id,
                field_name="XYZ Rotations",
                old_value=old_xyz_rotations,
                new_value=new_xyz_rotations,
            )]

        old_rotations = copy.deepcopy(data_block.get_field("Rotations"))
        if not old_rotations or not isinstance(old_rotations, dict):
            return []

        rot_channels = {}
        for c in self._sequence.channels:
            if c.data_block_id == changed_ch.data_block_id and c.component.startswith("rot_"):
                rot_channels[c.component] = c

        ref_ch = rot_channels.get("rot_w") or rot_channels.get("rot_x")
        if ref_ch is None:
            return []

        new_keys = []
        for i in range(len(ref_ch.keys)):
            w = rot_channels["rot_w"].keys[i].value if "rot_w" in rot_channels and i < len(rot_channels["rot_w"].keys) else 1.0
            x = rot_channels["rot_x"].keys[i].value if "rot_x" in rot_channels and i < len(rot_channels["rot_x"].keys) else 0.0
            y = rot_channels["rot_y"].keys[i].value if "rot_y" in rot_channels and i < len(rot_channels["rot_y"].keys) else 0.0
            z = rot_channels["rot_z"].keys[i].value if "rot_z" in rot_channels and i < len(rot_channels["rot_z"].keys) else 0.0
            k = ref_ch.keys[i]
            new_keys.append({
                "Time": k.time,
                "Value": {"w": w, "x": x, "y": y, "z": z},
                "Interpolation": k.key_type,
            })

        # Determine which key in old data
        old_key_field = "Keys" if "Keys" in old_rotations else "Quaternion Keys"
        new_rotations = copy.deepcopy(old_rotations)
        new_rotations["Num Keys"] = len(new_keys)
        new_rotations[old_key_field] = new_keys

        return [SetFieldAction(
            block_id=data_block.block_id,
            field_name="Rotations",
            old_value=old_rotations,
            new_value=new_rotations,
        )]

    def _build_scale_write_back(self, data_block, changed_ch: EditableChannel) -> list:
        """Rebuild the Scales key array."""
        from ui.editor.undo import SetFieldAction

        old_scales = copy.deepcopy(data_block.get_field("Scales"))
        if not old_scales or not isinstance(old_scales, dict):
            return []

        new_keys = []
        for k in changed_ch.keys:
            new_keys.append({
                "Time": k.time,
                "Value": k.value,
                "Interpolation": k.key_type,
            })

        new_scales = copy.deepcopy(old_scales)
        new_scales["Num Keys"] = len(new_keys)
        new_scales["Keys"] = new_keys

        return [SetFieldAction(
            block_id=data_block.block_id,
            field_name="Scales",
            old_value=old_scales,
            new_value=new_scales,
        )]

    def _build_float_write_back(self, data_block, changed_ch: EditableChannel) -> list:
        """Rebuild the Data key array for NiFloatData."""
        from ui.editor.undo import SetFieldAction

        old_data = copy.deepcopy(data_block.get_field("Data"))
        if not old_data or not isinstance(old_data, dict):
            return []

        new_data = copy.deepcopy(old_data)
        new_data.update(self._float_data_from_channel(changed_ch, new_data))

        return [SetFieldAction(
            block_id=data_block.block_id,
            field_name="Data",
            old_value=old_data,
            new_value=new_data,
        )]

    def _float_data_from_channel(
        self,
        changed_ch: EditableChannel,
        template: dict | None = None,
    ) -> dict:
        new_keys = []
        for key in changed_ch.keys:
            key_dict = {
                "Time": key.time,
                "Value": key.value,
                "Interpolation": key.key_type,
            }
            if key.key_type == KEY_QUADRATIC:
                key_dict["Forward"] = key.forward
                key_dict["Backward"] = key.backward
            new_keys.append(key_dict)

        data = copy.deepcopy(template) if isinstance(template, dict) else {}
        data.setdefault("Interpolation", KEY_LINEAR)
        data["Num Keys"] = len(new_keys)
        data["Keys"] = new_keys
        return data

    def _build_bool_write_back(self, data_block, changed_ch: EditableChannel) -> list:
        from ui.editor.undo import SetFieldAction

        old_data = copy.deepcopy(data_block.get_field("Data"))
        if not old_data or not isinstance(old_data, dict):
            return []

        new_data = copy.deepcopy(old_data)
        new_data["Num Keys"] = len(changed_ch.keys)
        new_data["Keys"] = [
            {
                "Time": key.time,
                "Value": bool(key.value),
                "Interpolation": key.key_type,
            }
            for key in changed_ch.keys
        ]

        return [SetFieldAction(
            block_id=data_block.block_id,
            field_name="Data",
            old_value=old_data,
            new_value=new_data,
        )]

    # -- Sync --

    @property
    def _coordinator(self):
        """Return the animation coordinator if available."""
        return getattr(self.app, 'anim_coordinator', None)

    def _get_active_mgr(self):
        """Return the active session's animation manager."""
        registry = getattr(self.app, 'registry', None)
        if registry:
            try:
                return registry.active_session.anim_manager
            except (KeyError, AttributeError):
                pass
        return getattr(self.app, 'animation_mgr', None)

    def _sync_panel_selection(self, seq_name: str):
        """Sync the animation manager's current sequence to match the editor."""
        coord = self._coordinator
        if coord:
            coord.select(seq_name)
            return

        mgr = self._get_active_mgr()
        if mgr and mgr.has_sequence(seq_name):
            mgr.select_sequence(seq_name)

    # -- Helpers --

    @staticmethod
    def _get_string(block, field_name: str) -> str | None:
        val = block.get_field(field_name)
        if val is None:
            return None
        if isinstance(val, str):
            return val if val else None
        if isinstance(val, list):
            return "".join(str(c) for c in val) or None
        return str(val)

    @staticmethod
    def _resolve_string_index(nif, index: int) -> str:
        strings = getattr(nif.header, "strings", None)
        if strings and 0 <= index < len(strings):
            return strings[index]
        return ""
