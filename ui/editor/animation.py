"""NIF animation playback system.

Reads NiControllerSequence blocks from a loaded NIF and plays back
transform keyframe animations on the SceneNode tree using frame-based
updates. Supports linear interpolation of position, rotation, and scale
keys, with quadratic/TBC keys approximated as linear.
"""
from __future__ import annotations
import logging
import math
from collections.abc import Callable
from dataclasses import dataclass, field

import glm

from creation_lib.nif.nif_file import NifFile, NifBlock
from creation_lib.renderer.scene_renderer import Material, SceneNode
from ui.editor.particles.runtime import PARTICLE_PREVIEW_SEQUENCE
from ui.editor.sound_events import parse_sound_text_key

_log = logging.getLogger("nif_editor.animation")

_LEGACY_TRANSFORM_SEQUENCE = "[Legacy Transform Controllers]"


# -- Data structures for parsed keyframes --

@dataclass
class TransformKey:
    """A single transform keyframe at a given time."""
    time: float
    pos: tuple[float, float, float] | None = None
    rot: tuple[float, float, float, float] | None = None  # quaternion (w, x, y, z)
    scale: float | None = None


@dataclass
class FloatKey:
    """A single float keyframe at a given time."""
    time: float
    value: float = 0.0


@dataclass
class ColorKey:
    """A single RGB keyframe at a given time."""
    time: float
    value: tuple[float, float, float] = (1.0, 1.0, 1.0)


@dataclass
class SoundEvent:
    time: float
    cue: str
    raw_value: str


@dataclass
class ControlledChannel:
    """A single animated channel targeting a named node."""
    node_name: str
    transform_keys: list[TransformKey] = field(default_factory=list)
    float_keys: list[FloatKey] = field(default_factory=list)
    color_keys: list[ColorKey] = field(default_factory=list)
    # For standalone property controllers (BSEffectShaderPropertyFloatController etc.)
    material_var: str | None = None  # e.g. "U Offset", "V Offset", "EmissiveMultiple"
    material_color_var: str | None = None


@dataclass
class AnimSequence:
    """A parsed NiControllerSequence ready for playback."""
    name: str
    start_time: float
    stop_time: float
    cycle_type: int  # 0=loop, 1=reverse, 2=clamp
    channels: list[ControlledChannel] = field(default_factory=list)
    sound_events: list[SoundEvent] = field(default_factory=list)

    @property
    def duration(self) -> float:
        return max(self.stop_time - self.start_time, 0.0001)


# -- Material variable accessors for property controller playback --

# BSEffectShaderPropertyFloatController "Controlled Variable" enum → name
_EFFECT_CTRL_VAR_ENUM = {
    0: "EmissiveMultiple",
    1: "Falloff Start Angle",
    2: "Falloff Stop Angle",
    3: "Falloff Start Opacity",
    4: "Falloff Stop Opacity",
    5: "Alpha Transparency",
    6: "U Offset",
    7: "U Scale",
    8: "V Offset",
    9: "V Scale",
}

_EFFECT_COLOR_VAR_ENUM = {
    0: "Emissive Color",
}

_LIGHTING_CTRL_VAR_ENUM = {
    0: "Refraction Strength",
    1: "Emissive Multiple",
    2: "Environment Map Scale",
    3: "Glossiness",
    4: "Specular Strength",
    5: "Alpha",
    6: "U Offset",
    7: "U Scale",
    8: "V Offset",
    9: "V Scale",
}

_LIGHTING_COLOR_VAR_ENUM = {
    0: "Specular Color",
    1: "Emissive Color",
}

_MATERIAL_VAR_MAP = {
    "U Offset": ("uv_scale_offset", 2),  # .z
    "V Offset": ("uv_scale_offset", 3),  # .w
    "U Scale": ("uv_scale_offset", 0),   # .x
    "V Scale": ("uv_scale_offset", 1),   # .y
    "EmissiveMultiple": ("emissive_mult", None),
    "Emissive Multiple": ("glow_mult", None),
    "Alpha Transparency": ("emissive_color", 3),
    "Alpha": ("emissive_color", 3),
    "Environment Map Scale": ("env_map_scale", None),
    "Glossiness": ("glossiness", None),
    "Specular Strength": ("spec_strength", None),
}


def _get_material_var(mat: Material, var: str) -> float:
    info = _MATERIAL_VAR_MAP.get(var)
    if info is None:
        return 0.0
    attr, idx = info
    val = getattr(mat, attr, 0.0)
    if idx is not None:
        return float(val[idx])
    return float(val)


def _set_material_var(mat: Material, var: str, value: float) -> None:
    info = _MATERIAL_VAR_MAP.get(var)
    if info is None:
        return
    attr, idx = info
    if idx is not None:
        vec = getattr(mat, attr)
        vec[idx] = value
    else:
        setattr(mat, attr, value)


def _safe_int(value) -> int:
    if value is None:
        return -1
    try:
        return int(value)
    except (TypeError, ValueError):
        return -1


def _float_interpolator_data_ref(nif: NifFile, interp_ref: int) -> int:
    interp = nif.get_block(interp_ref) if interp_ref >= 0 else None
    if interp is None:
        return -1
    return _safe_int(interp.get_field("Data"))


def _property_type_for_controller(controller_type: str) -> str:
    if "EffectShaderProperty" in controller_type:
        return "BSEffectShaderProperty"
    if "LightingShaderProperty" in controller_type:
        return "BSLightingShaderProperty"
    return ""


@dataclass
class _PropertyController:
    """A standalone property controller (e.g. BSEffectShaderPropertyFloatController).

    Not part of any NiControllerSequence — these are always-on controllers
    attached directly to shader properties that drive UV scrolling, emissive, etc.
    """
    node_name: str  # The shape node that owns this property
    material_var: str  # "U Offset", "V Offset", "EmissiveMultiple", etc.
    float_keys: list[FloatKey] = field(default_factory=list)
    frequency: float = 1.0
    phase: float = 0.0
    start_time: float = 0.0
    stop_time: float = 0.0
    cycle_loop: bool = True  # derived from controller flags
    controller_block_id: int = -1
    interpolator_block_id: int = -1
    data_block_id: int = -1
    target_block_id: int = -1
    controller_type: str = ""
    property_type: str = ""

    @property
    def duration(self) -> float:
        return max(self.stop_time - self.start_time, 0.0001)


def _slerp(q0: glm.quat, q1: glm.quat, t: float) -> glm.quat:
    """Spherical linear interpolation between two quaternions."""
    dot = glm.dot(q0, q1)
    # Ensure shortest path
    if dot < 0:
        q1 = -q1
        dot = -dot
    # If very close, use nlerp
    if dot > 0.9995:
        result = q0 + t * (q1 - q0)
        return glm.normalize(result)
    theta = math.acos(min(dot, 1.0))
    sin_theta = math.sin(theta)
    a = math.sin((1.0 - t) * theta) / sin_theta
    b = math.sin(t * theta) / sin_theta
    return glm.normalize(a * q0 + b * q1)


def _decompose_mat4(mat: glm.mat4) -> tuple[glm.vec3, glm.quat, float]:
    """Decompose a mat4 into (translation, rotation, scale)."""
    pos = glm.vec3(mat[3])
    sx = glm.length(glm.vec3(mat[0]))
    sy = glm.length(glm.vec3(mat[1]))
    sz = glm.length(glm.vec3(mat[2]))
    scale = (sx + sy + sz) / 3.0  # uniform scale approximation
    rot_mat = glm.mat3(
        glm.vec3(mat[0]) / sx if sx > 0 else glm.vec3(1, 0, 0),
        glm.vec3(mat[1]) / sy if sy > 0 else glm.vec3(0, 1, 0),
        glm.vec3(mat[2]) / sz if sz > 0 else glm.vec3(0, 0, 1),
    )
    rot = glm.quat_cast(glm.mat4(rot_mat))
    return pos, rot, scale


def _euler_xyz_to_quat(rx: float, ry: float, rz: float) -> tuple[float, float, float, float]:
    cx, sx = math.cos(rx / 2), math.sin(rx / 2)
    cy, sy = math.cos(ry / 2), math.sin(ry / 2)
    cz, sz = math.cos(rz / 2), math.sin(rz / 2)

    w = cx * cy * cz + sx * sy * sz
    x = sx * cy * cz - cx * sy * sz
    y = cx * sy * cz + sx * cy * sz
    z = cx * cy * sz - sx * sy * cz
    return (w, x, y, z)


class AnimationManager:
    """Finds and plays NIF animations on a SceneNode tree.

    Call update(dt, scene_root) every frame from the main loop.
    """

    def __init__(self):
        self._sequences: dict[str, AnimSequence] = {}
        self._playing: bool = False
        self._paused: bool = False
        self._current_seq: AnimSequence | None = None
        self._elapsed: float = 0.0
        self._node_cache: dict[str, SceneNode] = {}
        self._rest_transforms: dict[str, tuple[glm.vec3, glm.quat, float]] = {}
        self._speed: float = 1.0
        self._loop: bool = True
        self._dirty: bool = False  # True when transforms were modified this frame
        self._sound_callback: Callable[[SoundEvent], None] | None = None
        self._sound_muted: bool = False
        # Standalone property controllers (always-on, not part of sequences)
        self._property_channels: list[_PropertyController] = []
        self._prop_elapsed: float = 0.0
        self._prop_playing: bool = False
        self._rest_materials: dict[str, dict[str, object]] = {}  # node_name → {var → value}

    # -- Public API --

    def set_sound_callback(self, callback: Callable[[SoundEvent], None] | None) -> None:
        self._sound_callback = callback

    @property
    def sound_muted(self) -> bool:
        return self._sound_muted

    @sound_muted.setter
    def sound_muted(self, value: bool) -> None:
        self._sound_muted = bool(value)

    def scan(self, nif: NifFile | None) -> None:
        """Scan the loaded NIF for animations.

        Finds both NiControllerSequence blocks and standalone property
        controller chains (BSEffectShaderPropertyFloatController etc.)
        that drive UV offset, emissive, and other material parameters.
        """
        current_name = self._current_seq.name if self._current_seq else None
        current_elapsed = self._elapsed
        was_playing = self._playing
        was_paused = self._paused
        self._sequences.clear()
        self._property_channels.clear()
        if nif is None:
            self._current_seq = None
            self._playing = False
            self._paused = False
            self._prop_playing = False
            self._elapsed = 0.0
            return

        for block in nif.find_blocks("NiControllerSequence"):
            seq = self._parse_sequence(nif, block)
            if seq:
                self._sequences[seq.name] = seq

        legacy_transform_seq = self._scan_legacy_transform_controllers(nif)
        if legacy_transform_seq is not None:
            self._sequences[legacy_transform_seq.name] = legacy_transform_seq

        # Scan for standalone property controller chains
        self._scan_property_controllers(nif)
        if self._property_channels:
            _log.info(
                "Found %d property controllers: %s",
                len(self._property_channels),
                ", ".join(f"{pc.node_name}.{pc.material_var}" for pc in self._property_channels),
            )

        if current_name == "[Property Controllers]" and self._property_channels:
            self._current_seq = self._make_property_sequence()
            self._elapsed = min(current_elapsed, self._current_seq.duration)
            self._playing = was_playing
            self._paused = was_paused
            self._prop_playing = was_playing
        elif current_name in self._sequences:
            self._current_seq = self._sequences[current_name]
            self._elapsed = min(current_elapsed, self._current_seq.duration)
            self._playing = was_playing
            self._paused = was_paused
            self._prop_playing = False
        else:
            self._current_seq = None
            self._playing = False
            self._paused = False
            self._prop_playing = False
            self._elapsed = 0.0

    def get_sequences(self) -> list[str]:
        names = list(self._sequences.keys())
        if self._property_channels:
            names.append("[Property Controllers]")
        return names

    def has_sequence(self, name: str) -> bool:
        """Check if a named sequence exists."""
        if name == PARTICLE_PREVIEW_SEQUENCE:
            return False
        if name == "[Property Controllers]":
            return bool(self._property_channels)
        return name in self._sequences

    def play(self, sequence_name: str) -> None:
        if sequence_name == PARTICLE_PREVIEW_SEQUENCE:
            return
        if sequence_name == "[Property Controllers]":
            self._start_property_playback()
            return
        if sequence_name not in self._sequences:
            return
        if (self._paused and self._current_seq is not None
                and self._current_seq.name == sequence_name):
            self.resume()
            return
        # Clear cache BEFORE stop() so stop() doesn't restore stale rest
        # transforms and undo any moves the user made since the last play.
        self._node_cache.clear()
        self._rest_transforms.clear()
        self._rest_materials.clear()
        self.stop()
        self._current_seq = self._sequences[sequence_name]
        self._elapsed = 0.0
        self._playing = True
        self._paused = False

    def pause(self) -> None:
        if self._playing:
            self._playing = False
            self._paused = True
            # Property controllers pause alongside sequences
            if self._prop_playing:
                self._prop_playing = False

    def resume(self) -> None:
        if self._paused and self._current_seq is not None:
            self._playing = True
            self._paused = False
            # Resume property controllers if they were active
            if self._property_channels and self._current_seq.name == "[Property Controllers]":
                self._prop_playing = True

    def stop(self) -> None:
        # Restore nodes to their original rest transforms
        if self._rest_transforms and self._node_cache:
            for name, (pos, rot, scl) in self._rest_transforms.items():
                node = self._node_cache.get(name)
                if node is None:
                    continue
                mat = glm.mat4(1.0)
                mat = glm.translate(mat, pos)
                mat = mat * glm.mat4_cast(rot)
                mat = glm.scale(mat, glm.vec3(scl))
                node.transform = mat
            self._dirty = True
        # Restore material properties
        self._stop_property_playback()
        self._restore_materials()
        self._playing = False
        self._paused = False
        self._current_seq = None
        self._elapsed = 0.0

    def select_sequence(self, sequence_name: str) -> None:
        if sequence_name == PARTICLE_PREVIEW_SEQUENCE:
            self._current_seq = None
            self._elapsed = 0.0
            self._paused = False
            self._playing = False
            return
        if sequence_name == "[Property Controllers]":
            # Select the synthetic property controller sequence
            if self._property_channels:
                self._current_seq = self._make_property_sequence()
                self._elapsed = 0.0
                self._paused = True
                self._playing = False
            return
        if sequence_name not in self._sequences:
            return
        self._current_seq = self._sequences[sequence_name]
        self._elapsed = 0.0
        self._paused = True
        self._playing = False

    def set_time(self, t: float, scene_root: SceneNode | None = None) -> None:
        if self._current_seq is None:
            return
        seq = self._current_seq
        t = max(seq.start_time, min(t, seq.stop_time))
        self._elapsed = t - seq.start_time
        # Apply the frame immediately so scrubbing updates the pose
        if not self._node_cache and scene_root:
            self._rebuild_node_cache(scene_root)
        if self._node_cache:
            self._apply_frame(t)
            if self._prop_playing or seq.name == "[Property Controllers]":
                self._prop_elapsed = t
                self._apply_property_controllers(t)
            self._dirty = True

    @property
    def speed(self) -> float:
        return self._speed

    @speed.setter
    def speed(self, value: float) -> None:
        self._speed = max(0.01, value)

    @property
    def loop(self) -> bool:
        return self._loop

    @loop.setter
    def loop(self, value: bool) -> None:
        self._loop = value

    @property
    def is_playing(self) -> bool:
        return self._playing

    @property
    def is_paused(self) -> bool:
        return self._paused

    @property
    def current_time(self) -> float:
        if self._current_seq is None:
            return 0.0
        return self._current_seq.start_time + self._elapsed

    @property
    def current_sequence(self) -> AnimSequence | None:
        return self._current_seq

    # -- Frame update --

    def update(self, dt: float, scene_root: SceneNode | None) -> None:
        """Advance animation by dt seconds. Called from gui() loop."""
        self._dirty = False

        # Rebuild node cache if needed (shared by sequences and property controllers)
        if not self._node_cache and scene_root:
            self._rebuild_node_cache(scene_root)

        # Advance standalone property controllers (always-on, independent of sequences)
        if self._prop_playing and self._property_channels and self._node_cache:
            self._prop_elapsed += dt * self._speed
            self._apply_property_controllers(self._prop_elapsed)
            self._dirty = True

        if not self._playing or self._current_seq is None:
            return

        seq = self._current_seq
        duration = seq.duration
        prev_t = seq.start_time + min(self._elapsed, duration)

        self._elapsed += dt * self._speed

        effective_cycle = 0 if self._loop else seq.cycle_type
        wrapped = False

        if effective_cycle == 0:
            while self._elapsed >= duration:
                self._elapsed -= duration
                wrapped = True
        elif effective_cycle == 1:
            period = duration * 2
            if self._elapsed >= period:
                wrapped = True
            self._elapsed = self._elapsed % period
        else:
            if self._elapsed >= duration:
                self._elapsed = duration
                self._apply_frame(seq.stop_time)
                self._trigger_sound_events(prev_t, seq.stop_time, wrapped=False)
                self._dirty = True
                self._playing = False
                self._paused = False
                return

        if effective_cycle == 1 and self._elapsed > duration:
            local_t = seq.start_time + (duration * 2 - self._elapsed)
        else:
            local_t = seq.start_time + self._elapsed

        self._apply_frame(local_t)
        self._trigger_sound_events(prev_t, local_t, wrapped=wrapped)
        self._dirty = True

    # -- Frame application --

    def _apply_frame(self, t: float) -> None:
        seq = self._current_seq
        if seq is None:
            return

        for channel in seq.channels:
            node = self._node_cache.get(channel.node_name)
            if node is None:
                continue

            if channel.transform_keys:
                self._apply_transform_keys(node, channel.transform_keys, t)
            if channel.float_keys and channel.material_var:
                self._apply_material_float_keys(
                    node, channel.material_var, channel.float_keys, t
                )
            if channel.color_keys and channel.material_color_var:
                self._apply_material_color_keys(
                    node, channel.material_color_var, channel.color_keys, t
                )

    def _trigger_sound_events(self, prev_t: float, current_t: float, *, wrapped: bool) -> None:
        seq = self._current_seq
        if seq is None or self._sound_muted or self._sound_callback is None or not seq.sound_events:
            return

        for event in seq.sound_events:
            should_fire = prev_t < event.time <= current_t
            if wrapped:
                should_fire = event.time > prev_t or event.time <= current_t
            if should_fire:
                self._sound_callback(event)

    def _apply_transform_keys(
        self, node: SceneNode, keys: list[TransformKey], t: float
    ) -> None:
        if not keys:
            return

        k0, k1, frac = self._find_bounding_keys(keys, t)

        # Start from the node's original rest transform (like NifSkope:
        # unkeyed components preserve their original NIF values)
        rest = self._rest_transforms.get(node.name)
        if rest:
            pos, rot, scl = rest[0], rest[1], rest[2]
        else:
            pos = glm.vec3(0, 0, 0)
            rot = glm.quat(1, 0, 0, 0)
            scl = 1.0

        # Only override components that have keyframe data
        if k0.pos is not None:
            p0 = glm.vec3(*k0.pos)
            if k1.pos is not None:
                p1 = glm.vec3(*k1.pos)
                pos = glm.mix(p0, p1, frac)
            else:
                pos = p0

        if k0.rot is not None:
            q0 = glm.quat(k0.rot[0], k0.rot[1], k0.rot[2], k0.rot[3])
            if k1.rot is not None:
                q1 = glm.quat(k1.rot[0], k1.rot[1], k1.rot[2], k1.rot[3])
                rot = _slerp(q0, q1, frac)
            else:
                rot = q0

        if k0.scale is not None:
            s0 = k0.scale
            if k1.scale is not None:
                scl = s0 + (k1.scale - s0) * frac
            else:
                scl = s0

        # Build transform matrix
        mat = glm.mat4(1.0)
        mat = glm.translate(mat, pos)
        mat = mat * glm.mat4_cast(rot)
        mat = glm.scale(mat, glm.vec3(scl))
        node.transform = mat

    def _apply_material_float_keys(
        self,
        node: SceneNode,
        material_var: str,
        keys: list[FloatKey],
        t: float,
    ) -> None:
        if node.mesh is None:
            return
        mat = node.mesh.material
        if material_var not in _MATERIAL_VAR_MAP:
            return
        self._remember_material_var(node.name, mat, material_var)
        value = self._interpolate_float_keys(keys, t)
        _set_material_var(mat, material_var, value)

    def _apply_material_color_keys(
        self,
        node: SceneNode,
        material_color_var: str,
        keys: list[ColorKey],
        t: float,
    ) -> None:
        if node.mesh is None:
            return
        mat = node.mesh.material
        if material_color_var != "Emissive Color":
            return
        self._remember_material_color_var(node.name, mat, material_color_var)
        r, g, b = self._interpolate_color_keys(keys, t)
        current = getattr(mat, "emissive_color", glm.vec4(1, 1, 1, 1))
        mat.emissive_color = glm.vec4(r, g, b, float(current.w))

    def _remember_material_var(self, node_name: str, mat: Material, var: str) -> None:
        rest = self._rest_materials.setdefault(node_name, {})
        if var not in rest:
            rest[var] = _get_material_var(mat, var)

    def _remember_material_color_var(
        self,
        node_name: str,
        mat: Material,
        var: str,
    ) -> None:
        rest = self._rest_materials.setdefault(node_name, {})
        if var not in rest:
            value = getattr(mat, "emissive_color", glm.vec4(1, 1, 1, 1))
            rest[var] = tuple(float(value[i]) for i in range(4))

    def _restore_materials(self) -> None:
        for node_name, vars_dict in self._rest_materials.items():
            node = self._node_cache.get(node_name)
            if node is None or node.mesh is None:
                continue
            mat = node.mesh.material
            for var, val in vars_dict.items():
                if var == "Emissive Color" and isinstance(val, tuple):
                    mat.emissive_color = glm.vec4(*val)
                elif isinstance(val, (int, float)):
                    _set_material_var(mat, var, float(val))
        self._rest_materials.clear()

    @staticmethod
    def _find_bounding_keys(
        keys: list[TransformKey | FloatKey], t: float
    ) -> tuple:
        if len(keys) == 1:
            return keys[0], keys[0], 0.0
        if t <= keys[0].time:
            return keys[0], keys[0], 0.0
        if t >= keys[-1].time:
            return keys[-1], keys[-1], 0.0

        lo, hi = 0, len(keys) - 1
        while lo < hi - 1:
            mid = (lo + hi) // 2
            if keys[mid].time <= t:
                lo = mid
            else:
                hi = mid

        k0 = keys[lo]
        k1 = keys[hi]
        dt = k1.time - k0.time
        frac = (t - k0.time) / dt if dt > 0 else 0.0
        return k0, k1, frac

    # -- Node cache --

    def _rebuild_node_cache(self, scene_root: SceneNode) -> None:
        self._node_cache.clear()
        self._rest_transforms.clear()
        self._rest_materials.clear()
        self._walk_nodes(scene_root)

    def _walk_nodes(self, node: SceneNode) -> None:
        if node.name and node.name != "nif_root":
            self._node_cache[node.name] = node
            self._rest_transforms[node.name] = _decompose_mat4(node.transform)
        for child in node.children:
            self._walk_nodes(child)

    # -- Standalone property controller support --

    def _scan_property_controllers(self, nif: NifFile) -> None:
        """Find BSEffectShaderPropertyFloatController chains on shapes."""
        schema = nif.schema
        for block in nif.blocks:
            if block is None:
                continue
            # Look for shapes (BSTriShape, NiTriShape, etc.)
            if not schema.is_subtype_of(block.type_name, "NiAVObject"):
                continue
            name = self._get_string(block, "Name") or ""
            if not name:
                continue
            # Check properties / shader property for controllers
            self._scan_shape_controllers(nif, block, name)

    def _scan_legacy_transform_controllers(self, nif: NifFile) -> AnimSequence | None:
        """Build a synthetic sequence from direct NiTransformController chains."""
        schema = nif.schema
        channels: list[ControlledChannel] = []
        start_time: float | None = None
        stop_time: float | None = None

        for block in nif.blocks:
            if block is None or not schema.is_subtype_of(block.type_name, "NiAVObject"):
                continue
            node_name = self._get_string(block, "Name") or ""
            if not node_name:
                continue

            ctrl_ref = self._ref_id(block.get_field("Controller"))
            visited: set[int] = set()
            while ctrl_ref >= 0 and ctrl_ref not in visited:
                visited.add(ctrl_ref)
                ctrl = nif.get_block(ctrl_ref)
                if ctrl is None:
                    break

                if schema.is_subtype_of(ctrl.type_name, "NiTransformController"):
                    channel = self._parse_legacy_transform_controller(
                        nif, block, node_name, ctrl
                    )
                    if channel is not None:
                        channels.append(channel)
                        start, stop = self._legacy_controller_time_range(ctrl, channel)
                        start_time = start if start_time is None else min(start_time, start)
                        stop_time = stop if stop_time is None else max(stop_time, stop)

                ctrl_ref = self._ref_id(ctrl.get_field("Next Controller"))

        if not channels:
            return None

        return AnimSequence(
            name=_LEGACY_TRANSFORM_SEQUENCE,
            start_time=start_time if start_time is not None else 0.0,
            stop_time=stop_time if stop_time is not None else 0.001,
            cycle_type=2,
            channels=channels,
        )

    def _parse_legacy_transform_controller(
        self,
        nif: NifFile,
        node_block: NifBlock,
        node_name: str,
        ctrl: NifBlock,
    ) -> ControlledChannel | None:
        target_block_id = _safe_int(ctrl.get_field("Target"))
        if target_block_id >= 0 and target_block_id != node_block.block_id:
            return None

        interp_ref = self._ref_id(ctrl.get_field("Interpolator"))
        if interp_ref < 0:
            return None

        interp_block = nif.get_block(interp_ref)
        if interp_block is None or not nif.schema.is_subtype_of(
            interp_block.type_name, "NiTransformInterpolator"
        ):
            return None

        transform_keys = self._parse_transform_interpolator(nif, interp_block)
        if not transform_keys:
            return None

        return ControlledChannel(node_name=node_name, transform_keys=transform_keys)

    @staticmethod
    def _legacy_controller_time_range(
        ctrl: NifBlock,
        channel: ControlledChannel,
    ) -> tuple[float, float]:
        key_start = channel.transform_keys[0].time
        key_stop = channel.transform_keys[-1].time
        raw_start = ctrl.get_field("Start Time")
        raw_stop = ctrl.get_field("Stop Time")
        start_time = float(raw_start) if raw_start is not None else key_start
        stop_time = float(raw_stop) if raw_stop is not None else key_stop
        return start_time, max(stop_time, start_time, key_stop)

    def _scan_shape_controllers(
        self, nif: NifFile, shape: NifBlock, shape_name: str
    ) -> None:
        """Walk a shape's shader property controller chain."""
        schema = nif.schema
        # Find the shader property — try "Shader Property" (BSTriShape) and
        # "Properties" list (NiTriShape)
        shader_ref = shape.get_field("Shader Property")
        if isinstance(shader_ref, dict):
            shader_ref = shader_ref.get("value", shader_ref.get("Value", -1))
        shader_ref = int(shader_ref) if shader_ref is not None else -1

        if shader_ref >= 0:
            shader = nif.get_block(shader_ref)
            if shader and schema.is_subtype_of(
                shader.type_name, "BSShaderProperty"
            ):
                self._walk_controller_chain(nif, shader, shape_name)
            return

        # NiTriShape: walk Properties list
        props = shape.get_field("Properties") or []
        if isinstance(props, list):
            for ref in props:
                bid = int(ref) if not isinstance(ref, dict) else int(
                    ref.get("value", ref.get("Value", -1))
                )
                if bid < 0:
                    continue
                prop = nif.get_block(bid)
                if prop and nif.schema.is_subtype_of(
                    prop.type_name, "BSShaderProperty"
                ):
                    self._walk_controller_chain(nif, prop, shape_name)

    def _walk_controller_chain(
        self, nif: NifFile, prop_block: NifBlock, shape_name: str
    ) -> None:
        """Walk the Controller linked-list on a shader property block."""
        ctrl_ref = prop_block.get_field("Controller")
        if isinstance(ctrl_ref, dict):
            ctrl_ref = ctrl_ref.get("value", ctrl_ref.get("Value", -1))
        ctrl_ref = int(ctrl_ref) if ctrl_ref is not None else -1

        visited = set()
        while ctrl_ref >= 0 and ctrl_ref not in visited:
            visited.add(ctrl_ref)
            ctrl = nif.get_block(ctrl_ref)
            if ctrl is None:
                break

            pc = self._parse_property_controller(nif, ctrl, shape_name)
            if pc and pc.float_keys:
                self._property_channels.append(pc)

            # Follow linked list
            next_ref = ctrl.get_field("Next Controller")
            if isinstance(next_ref, dict):
                next_ref = next_ref.get("value", next_ref.get("Value", -1))
            ctrl_ref = int(next_ref) if next_ref is not None else -1

    def _parse_property_controller(
        self, nif: NifFile, ctrl: NifBlock, shape_name: str
    ) -> _PropertyController | None:
        """Parse a single property float controller."""
        raw_var = ctrl.get_field("Controlled Variable")
        if raw_var is None:
            return None
        # Resolve integer enum to string name
        if isinstance(raw_var, int):
            var = _EFFECT_CTRL_VAR_ENUM.get(raw_var, "")
        else:
            var = str(raw_var)
        if not var or var not in _MATERIAL_VAR_MAP:
            return None

        flags = int(ctrl.get_field("Flags") or 0)
        frequency = float(ctrl.get_field("Frequency") or 1.0)
        phase = float(ctrl.get_field("Phase") or 0.0)
        start_time = float(ctrl.get_field("Start Time") or 0.0)
        stop_time = float(ctrl.get_field("Stop Time") or 0.0)
        # Flags bit 3 = active, bits 4-5 = cycle type
        cycle_loop = True  # Default to looping

        interp_ref = ctrl.get_field("Interpolator")
        if isinstance(interp_ref, dict):
            interp_ref = interp_ref.get("value", interp_ref.get("Value", -1))
        interp_ref = int(interp_ref) if interp_ref is not None else -1

        float_keys: list[FloatKey] = []
        if interp_ref >= 0:
            interp = nif.get_block(interp_ref)
            if interp and nif.schema.is_subtype_of(
                interp.type_name, "NiFloatInterpolator"
            ):
                float_keys = self._parse_float_interpolator(nif, interp)

        return _PropertyController(
            node_name=shape_name,
            material_var=var,
            float_keys=float_keys,
            frequency=frequency,
            phase=phase,
            start_time=start_time,
            stop_time=stop_time,
            cycle_loop=cycle_loop,
            controller_block_id=ctrl.block_id,
            interpolator_block_id=interp_ref,
            data_block_id=_float_interpolator_data_ref(nif, interp_ref),
            target_block_id=_safe_int(ctrl.get_field("Target")),
            controller_type=ctrl.type_name,
            property_type=_property_type_for_controller(ctrl.type_name),
        )

    def _make_property_sequence(self) -> AnimSequence:
        """Create a synthetic AnimSequence for UI display of property controllers."""
        max_stop = max(
            pc.stop_time for pc in self._property_channels
        ) if self._property_channels else 1.0
        return AnimSequence(
            name="[Property Controllers]",
            start_time=0.0,
            stop_time=max(max_stop, 0.001),
            cycle_type=0,
        )

    def _start_property_playback(self) -> None:
        """Start playing standalone property controllers."""
        if not self._property_channels:
            _log.warning("_start_property_playback: no property channels")
            return
        # Invalidate node cache so it rebuilds from current scene tree
        self._node_cache.clear()
        self._rest_transforms.clear()
        self._rest_materials.clear()
        self._prop_playing = True
        self._prop_elapsed = 0.0
        # Also set current_seq for the UI timeline
        self._current_seq = self._make_property_sequence()
        self._playing = True
        self._paused = False
        _log.info(
            "Property playback started: %d channels, stop=%.3f",
            len(self._property_channels), self._current_seq.stop_time,
        )

    def _stop_property_playback(self) -> None:
        """Stop property controllers and restore materials."""
        if not self._prop_playing:
            return
        self._prop_playing = False
        self._prop_elapsed = 0.0
        self._restore_materials()

    def _apply_property_controllers(self, elapsed: float) -> None:
        """Apply all standalone property controllers at the given time."""
        for pc in self._property_channels:
            node = self._node_cache.get(pc.node_name)
            if node is None or node.mesh is None:
                continue
            mat = node.mesh.material
            # Save rest value on first access
            self._remember_material_var(pc.node_name, mat, pc.material_var)

            # Compute effective time matching NifSkope's ctrlTime():
            #   time = frequency * elapsed + phase
            #   cyclic wrap: start + fmod(time - start, stop - start)
            effective_t = elapsed * pc.frequency + pc.phase
            delta = pc.stop_time - pc.start_time
            if effective_t >= pc.start_time and effective_t <= pc.stop_time:
                pass  # already in range
            elif delta > 0 and pc.cycle_loop:
                x = (effective_t - pc.start_time) / delta
                effective_t = pc.start_time + (x - math.floor(x)) * delta
            else:
                effective_t = max(pc.start_time, min(effective_t, pc.stop_time))

            # Interpolate float keys
            if pc.float_keys:
                value = self._interpolate_float_keys(pc.float_keys, effective_t)
                _set_material_var(mat, pc.material_var, value)

    def _interpolate_float_keys(self, keys: list[FloatKey], t: float) -> float:
        """Linearly interpolate float keys at time t."""
        if not keys:
            return 0.0
        k0, k1, frac = self._find_bounding_keys(keys, t)
        return k0.value + (k1.value - k0.value) * frac

    def _interpolate_color_keys(
        self,
        keys: list[ColorKey],
        t: float,
    ) -> tuple[float, float, float]:
        if not keys:
            return (1.0, 1.0, 1.0)
        k0, k1, frac = self._find_bounding_keys(keys, t)
        return tuple(
            float(k0.value[i]) + (float(k1.value[i]) - float(k0.value[i])) * frac
            for i in range(3)
        )

    # -- NIF parsing --

    def _parse_sequence(self, nif: NifFile, block: NifBlock) -> AnimSequence | None:
        name = self._get_string(block, "Name") or f"Sequence_{block.block_id}"
        start_time = float(block.get_field("Start Time") or 0.0)
        stop_time = float(block.get_field("Stop Time") or 0.0)
        cycle_type = int(block.get_field("Cycle Type") or 0)

        seq = AnimSequence(
            name=name, start_time=start_time,
            stop_time=stop_time, cycle_type=cycle_type,
        )

        controlled = block.get_field("Controlled Blocks")
        seq.sound_events = self._parse_sequence_sound_events(nif, block)
        if not controlled or not isinstance(controlled, list):
            return seq

        for cb in controlled:
            if not isinstance(cb, dict):
                continue
            channel = self._parse_controlled_block(nif, cb)
            if channel:
                seq.channels.append(channel)

        return seq

    def _parse_sequence_sound_events(self, nif: NifFile, block: NifBlock) -> list[SoundEvent]:
        text_ref = block.get_field("Text Keys")
        if isinstance(text_ref, dict):
            text_ref = text_ref.get("value", text_ref.get("Value", -1))
        text_ref = int(text_ref) if text_ref is not None else -1
        if text_ref < 0:
            return []

        text_block = nif.get_block(text_ref)
        if text_block is None or text_block.type_name != "NiTextKeyExtraData":
            return []

        result: list[SoundEvent] = []
        for entry in text_block.get_field("Text Keys") or []:
            if not isinstance(entry, dict):
                continue
            raw_value = str(entry.get("Value") or "")
            cue = parse_sound_text_key(raw_value)
            if not cue:
                continue
            result.append(SoundEvent(
                time=float(entry.get("Time", 0.0)),
                cue=cue,
                raw_value=raw_value,
            ))
        return sorted(result, key=lambda event: event.time)

    def _parse_controlled_block(
        self, nif: NifFile, cb: dict
    ) -> ControlledChannel | None:
        node_name = cb.get("Node Name") or cb.get("Target Name") or ""
        if isinstance(node_name, (int, float)):
            node_name = self._resolve_string_index(nif, int(node_name))
        if not node_name:
            return None

        channel = ControlledChannel(node_name=node_name)
        controller_ref = self._ref_id(cb.get("Controller", -1))
        controller_block = nif.get_block(controller_ref) if controller_ref >= 0 else None
        controller_type = str(
            getattr(controller_block, "type_name", "")
            or cb.get("Controller Type")
            or ""
        )
        target_property = self._resolve_controller_property(
            controller_type,
            cb.get("Controller ID"),
            controller_block,
        )
        if target_property in _MATERIAL_VAR_MAP:
            channel.material_var = target_property
        elif target_property == "Emissive Color":
            channel.material_color_var = target_property

        interp_ref = cb.get("Interpolator", -1)
        if isinstance(interp_ref, dict):
            interp_ref = interp_ref.get("value", interp_ref.get("Value", -1))
        interp_ref = int(interp_ref) if interp_ref is not None else -1

        if interp_ref < 0:
            return channel

        interp_block = nif.get_block(interp_ref)
        if interp_block is None:
            return channel

        schema = nif.schema

        if schema.is_subtype_of(interp_block.type_name, "NiTransformInterpolator"):
            channel.transform_keys = self._parse_transform_interpolator(
                nif, interp_block
            )
        elif schema.is_subtype_of(interp_block.type_name, "NiFloatInterpolator"):
            channel.float_keys = self._parse_float_interpolator(nif, interp_block)
        elif schema.is_subtype_of(interp_block.type_name, "NiPoint3Interpolator"):
            channel.color_keys = self._parse_point3_interpolator(nif, interp_block)

        return channel

    def _resolve_controller_property(
        self,
        controller_type: str,
        controller_id,
        controller_block: NifBlock | None,
    ) -> str:
        if controller_block is not None:
            for field_name in ("Controlled Variable", "Controlled Color"):
                raw_value = controller_block.get_field(field_name)
                if raw_value not in (None, ""):
                    return str(raw_value)

        if controller_type == "BSEffectShaderPropertyFloatController":
            return _EFFECT_CTRL_VAR_ENUM.get(_safe_int(controller_id), "")
        if controller_type == "BSEffectShaderPropertyColorController":
            return _EFFECT_COLOR_VAR_ENUM.get(_safe_int(controller_id), "")
        if controller_type == "BSLightingShaderPropertyFloatController":
            return _LIGHTING_CTRL_VAR_ENUM.get(_safe_int(controller_id), "")
        if controller_type == "BSLightingShaderPropertyColorController":
            return _LIGHTING_COLOR_VAR_ENUM.get(_safe_int(controller_id), "")
        if controller_type == "NiLightDimmerController":
            return "Dimmer"
        return ""

    def _parse_transform_interpolator(
        self, nif: NifFile, block: NifBlock
    ) -> list[TransformKey]:
        data_ref = block.get_field("Data")
        if isinstance(data_ref, dict):
            data_ref = data_ref.get("value", data_ref.get("Value", -1))
        data_ref = int(data_ref) if data_ref is not None else -1

        if data_ref < 0:
            return self._parse_static_transform(block)

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []

        keys: dict[float, TransformKey] = {}
        self._extract_position_keys(data_block, keys)
        self._extract_rotation_keys(data_block, keys)
        self._extract_scale_keys(data_block, keys)

        return sorted(keys.values(), key=lambda k: k.time)

    def _parse_static_transform(self, block: NifBlock) -> list[TransformKey]:
        trans = block.get_field("Translation")
        rot = block.get_field("Rotation")
        scale = block.get_field("Scale")

        has_data = False
        key = TransformKey(time=0.0)

        if trans and isinstance(trans, dict):
            x = float(trans.get("x", 0))
            y = float(trans.get("y", 0))
            z = float(trans.get("z", 0))
            if x != 0 or y != 0 or z != 0:
                key.pos = (x, y, z)
                has_data = True

        if rot and isinstance(rot, dict):
            w = float(rot.get("w", 1))
            x = float(rot.get("x", 0))
            y = float(rot.get("y", 0))
            z = float(rot.get("z", 0))
            key.rot = (w, x, y, z)
            has_data = True

        if scale is not None:
            s = float(scale)
            if s != 1.0:
                key.scale = s
                has_data = True

        return [key] if has_data else []

    def _extract_position_keys(
        self, data_block: NifBlock, keys: dict[float, TransformKey]
    ) -> None:
        translations = data_block.get_field("Translations")
        if not translations or not isinstance(translations, dict):
            return
        key_list = translations.get("Keys") or []
        for k in key_list:
            if not isinstance(k, dict):
                continue
            t = float(k.get("Time", 0))
            val = k.get("Value", {})
            if not isinstance(val, dict):
                continue
            x = float(val.get("x", 0))
            y = float(val.get("y", 0))
            z = float(val.get("z", 0))
            if t not in keys:
                keys[t] = TransformKey(time=t)
            keys[t].pos = (x, y, z)

    def _extract_rotation_keys(
        self, data_block: NifBlock, keys: dict[float, TransformKey]
    ) -> None:
        rotations = data_block.get_field("Rotations")
        if not rotations or not isinstance(rotations, dict):
            self._extract_xyz_rotation_keys(data_block, keys)
            return
        key_list = rotations.get("Keys") or rotations.get("Quaternion Keys") or []
        for k in key_list:
            if not isinstance(k, dict):
                continue
            t = float(k.get("Time", 0))
            val = k.get("Value", {})
            if not isinstance(val, dict):
                continue
            w = float(val.get("w", 1))
            x = float(val.get("x", 0))
            y = float(val.get("y", 0))
            z = float(val.get("z", 0))
            if t not in keys:
                keys[t] = TransformKey(time=t)
            keys[t].rot = (w, x, y, z)

    def _extract_xyz_rotation_keys(
        self, data_block: NifBlock, keys: dict[float, TransformKey]
    ) -> None:
        xyz_rotations = data_block.get_field("XYZ Rotations")
        if not xyz_rotations or not isinstance(xyz_rotations, list):
            return

        axis_keys: list[list[tuple[float, float]]] = []
        all_times: set[float] = set()
        for axis_group in xyz_rotations[:3]:
            pairs: list[tuple[float, float]] = []
            if isinstance(axis_group, dict):
                for key in axis_group.get("Keys") or []:
                    if not isinstance(key, dict):
                        continue
                    time = float(key.get("Time", 0.0))
                    value = float(key.get("Value", 0.0))
                    pairs.append((time, value))
                    all_times.add(time)
            axis_keys.append(sorted(pairs))

        while len(axis_keys) < 3:
            axis_keys.append([])
        for time in sorted(all_times):
            rx = self._sample_float_pairs(axis_keys[0], time)
            ry = self._sample_float_pairs(axis_keys[1], time)
            rz = self._sample_float_pairs(axis_keys[2], time)
            if time not in keys:
                keys[time] = TransformKey(time=time)
            keys[time].rot = _euler_xyz_to_quat(rx, ry, rz)

    @staticmethod
    def _sample_float_pairs(pairs: list[tuple[float, float]], t: float) -> float:
        if not pairs:
            return 0.0
        if len(pairs) == 1 or t <= pairs[0][0]:
            return pairs[0][1]
        if t >= pairs[-1][0]:
            return pairs[-1][1]
        for idx in range(len(pairs) - 1):
            t0, v0 = pairs[idx]
            t1, v1 = pairs[idx + 1]
            if t0 <= t <= t1:
                dt = t1 - t0
                frac = (t - t0) / dt if dt > 0 else 0.0
                return v0 + (v1 - v0) * frac
        return pairs[-1][1]

    def _extract_scale_keys(
        self, data_block: NifBlock, keys: dict[float, TransformKey]
    ) -> None:
        scales = data_block.get_field("Scales")
        if not scales or not isinstance(scales, dict):
            return
        key_list = scales.get("Keys") or []
        for k in key_list:
            if not isinstance(k, dict):
                continue
            t = float(k.get("Time", 0))
            val = k.get("Value", 1.0)
            if t not in keys:
                keys[t] = TransformKey(time=t)
            keys[t].scale = float(val)

    def _parse_float_interpolator(
        self, nif: NifFile, block: NifBlock
    ) -> list[FloatKey]:
        data_ref = block.get_field("Data")
        if isinstance(data_ref, dict):
            data_ref = data_ref.get("value", data_ref.get("Value", -1))
        data_ref = int(data_ref) if data_ref is not None else -1

        if data_ref < 0:
            val = block.get_field("Value")
            if val is not None:
                return [FloatKey(time=0.0, value=float(val))]
            return []

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []

        result: list[FloatKey] = []
        data = data_block.get_field("Data")
        if data and isinstance(data, dict):
            key_list = data.get("Keys") or []
            for k in key_list:
                if not isinstance(k, dict):
                    continue
                t = float(k.get("Time", 0))
                v = float(k.get("Value", 0))
                result.append(FloatKey(time=t, value=v))

        result.sort(key=lambda fk: fk.time)
        return result

    def _parse_point3_interpolator(
        self,
        nif: NifFile,
        block: NifBlock,
    ) -> list[ColorKey]:
        data_ref = block.get_field("Data")
        if isinstance(data_ref, dict):
            data_ref = data_ref.get("value", data_ref.get("Value", -1))
        data_ref = int(data_ref) if data_ref is not None else -1

        if data_ref < 0:
            value = block.get_field("Value")
            if isinstance(value, dict) and float(value.get("x", 0.0)) > -3.0e38:
                return [ColorKey(
                    time=0.0,
                    value=(
                        float(value.get("x", 1.0)),
                        float(value.get("y", 1.0)),
                        float(value.get("z", 1.0)),
                    ),
                )]
            return []

        data_block = nif.get_block(data_ref)
        if data_block is None:
            return []

        result: list[ColorKey] = []
        data = data_block.get_field("Data")
        if data and isinstance(data, dict):
            for key in data.get("Keys") or []:
                if not isinstance(key, dict):
                    continue
                value = key.get("Value")
                if not isinstance(value, dict):
                    continue
                result.append(ColorKey(
                    time=float(key.get("Time", 0.0)),
                    value=(
                        float(value.get("x", 1.0)),
                        float(value.get("y", 1.0)),
                        float(value.get("z", 1.0)),
                    ),
                ))

        result.sort(key=lambda ck: ck.time)
        return result

    @staticmethod
    def _get_string(block: NifBlock, field_name: str) -> str | None:
        val = block.get_field(field_name)
        if val is None:
            return None
        if isinstance(val, str):
            return val if val else None
        if isinstance(val, list):
            return "".join(str(c) for c in val) or None
        return str(val)

    @staticmethod
    def _resolve_string_index(nif: NifFile, index: int) -> str:
        strings = getattr(nif.header, "strings", None)
        if strings and 0 <= index < len(strings):
            return strings[index]
        return ""

    @staticmethod
    def _ref_id(ref) -> int:
        if isinstance(ref, dict):
            ref = ref.get("value", ref.get("Value", -1))
        return int(ref) if ref is not None else -1
