from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


_SUPPORTED_EMITTERS = {
    "NiPSysBoxEmitter",
    "NiPSysCylinderEmitter",
    "NiPSysMeshEmitter",
    "NiPSysSphereEmitter",
}
_HELPER_REF_FIELDS = (
    "Emitter",
    "Emitter Object",
    "Gravity Object",
    "Drag Object",
    "Field Object",
)
_SHADER_FLAG_BITS = {
    "GreyscaleToPalette_Color": 4,
    "GreyscaleToPalette_Alpha": 5,
}


class ParticleSupportLevel(Enum):
    SUPPORTED = "supported"
    APPROXIMATE = "approximate"
    DIAGNOSTIC_ONLY = "diagnostic-only"
    UNSUPPORTED = "unsupported"


@dataclass(frozen=True)
class ParticleWarning:
    block_id: int
    message: str
    severity: str = "warning"


@dataclass(frozen=True)
class _ModifierEntry:
    block_id: int
    block: Any
    parent_block_id: int | None
    depth: int


@dataclass(frozen=True)
class ParticleSystemModel:
    nif_id: str
    system_block_id: int
    name: str
    data_block_id: int | None = None
    shader_property_block_id: int | None = None
    alpha_property_block_id: int | None = None
    controller_block_id: int | None = None
    emitter_block_id: int | None = None
    emitter_type: str | None = None
    modifier_block_ids: tuple[int, ...] = field(default_factory=tuple)
    modifier_types: tuple[str, ...] = field(default_factory=tuple)
    modifier_parent_block_ids: tuple[int | None, ...] = field(default_factory=tuple)
    modifier_depths: tuple[int, ...] = field(default_factory=tuple)
    helper_node_block_ids: tuple[int, ...] = field(default_factory=tuple)
    emitter_mesh_block_ids: tuple[int, ...] = field(default_factory=tuple)
    world_space: bool = False
    max_particles: int | None = None
    atlas_offsets: tuple[tuple[float, float, float, float], ...] = field(default_factory=tuple)
    source_texture: str | None = None
    greyscale_texture: str | None = None
    greyscale_color: bool = False
    greyscale_alpha: bool = False
    base_color: tuple[float, float, float, float] | None = None
    emitter_initial_color: tuple[float, float, float, float] | None = None
    emitter_speed: float | None = None
    emitter_speed_variation: float | None = None
    emitter_declination: float | None = None
    emitter_declination_variation: float | None = None
    emitter_planar_angle: float | None = None
    emitter_planar_angle_variation: float | None = None
    emitter_lifetime: float | None = None
    emitter_lifetime_variation: float | None = None
    emitter_initial_radius: float | None = None
    emitter_radius_variation: float | None = None
    emitter_radius: float | None = None
    emitter_object_block_id: int | None = None
    support_level: ParticleSupportLevel = ParticleSupportLevel.UNSUPPORTED
    warnings: tuple[ParticleWarning, ...] = field(default_factory=tuple)

    @property
    def block_ids(self) -> set[int]:
        return {
            block_id
            for block_id in (
                self.system_block_id,
                self.data_block_id,
                self.shader_property_block_id,
                self.alpha_property_block_id,
                self.controller_block_id,
                self.emitter_block_id,
                *self.modifier_block_ids,
                *self.helper_node_block_ids,
                *self.emitter_mesh_block_ids,
            )
            if block_id is not None and block_id >= 0
        }

    @property
    def warning_text(self) -> str:
        return "; ".join(warning.message for warning in self.warnings)


def build_particle_models(nif: Any, nif_id: str = "main") -> list[ParticleSystemModel]:
    models = []

    for block in getattr(nif, "blocks", []):
        if not _is_particle_system(nif, block):
            continue

        data_block_id = _as_block_id(block.get_field("Data"))
        data_block = _get_block(nif, data_block_id)
        shader_property_block_id = _as_block_id(block.get_field("Shader Property"))
        shader_property_block = _get_block(nif, shader_property_block_id)
        alpha_property_block_id = _as_block_id(block.get_field("Alpha Property"))
        controller_block_id = _as_block_id(block.get_field("Controller"))
        modifier_block_ids = tuple(_as_block_ids(block.get_field("Modifiers")))
        modifier_entries = _collect_modifier_entries(nif, modifier_block_ids)
        modifier_block_ids = tuple(entry.block_id for entry in modifier_entries)
        modifier_blocks = [entry.block for entry in modifier_entries]
        emitter_block = _find_emitter(nif, modifier_blocks)
        helper_node_block_ids = _ordered_unique(
            ref_id
            for modifier_block in modifier_blocks
            for ref_id in _collect_refs_from_fields(modifier_block, _HELPER_REF_FIELDS)
        )
        emitter_mesh_block_ids = tuple(_as_block_ids(emitter_block.get_field("Emitter Meshes"))) if emitter_block else ()
        warnings, support_level = _classify_model(block, data_block, emitter_block)

        models.append(ParticleSystemModel(
            nif_id=nif_id,
            system_block_id=block.block_id,
            name=_get_name(block),
            data_block_id=data_block_id,
            shader_property_block_id=shader_property_block_id,
            alpha_property_block_id=alpha_property_block_id,
            controller_block_id=controller_block_id,
            emitter_block_id=emitter_block.block_id if emitter_block else None,
            emitter_type=emitter_block.type_name if emitter_block else None,
            modifier_block_ids=modifier_block_ids,
            modifier_types=tuple(modifier_block.type_name for modifier_block in modifier_blocks),
            modifier_parent_block_ids=tuple(entry.parent_block_id for entry in modifier_entries),
            modifier_depths=tuple(entry.depth for entry in modifier_entries),
            helper_node_block_ids=helper_node_block_ids,
            emitter_mesh_block_ids=emitter_mesh_block_ids,
            world_space=bool(block.get_field("World Space")),
            max_particles=_max_particles(data_block),
            atlas_offsets=_atlas_offsets(data_block),
            source_texture=_source_texture(shader_property_block),
            greyscale_texture=_greyscale_texture(shader_property_block),
            greyscale_color=_shader_flag(shader_property_block, "GreyscaleToPalette_Color"),
            greyscale_alpha=_shader_flag(shader_property_block, "GreyscaleToPalette_Alpha"),
            base_color=_color_field(shader_property_block, "Base Color"),
            emitter_initial_color=_color_field(emitter_block, "Initial Color"),
            emitter_speed=_float_field(emitter_block, "Speed"),
            emitter_speed_variation=_float_field(emitter_block, "Speed Variation"),
            emitter_declination=_float_field(emitter_block, "Declination"),
            emitter_declination_variation=_float_field(emitter_block, "Declination Variation"),
            emitter_planar_angle=_float_field(emitter_block, "Planar Angle"),
            emitter_planar_angle_variation=_float_field(emitter_block, "Planar Angle Variation"),
            emitter_lifetime=_float_field(emitter_block, "Life Span"),
            emitter_lifetime_variation=_float_field(emitter_block, "Life Span Variation"),
            emitter_initial_radius=_float_field(emitter_block, "Initial Radius"),
            emitter_radius_variation=_float_field(emitter_block, "Radius Variation"),
            emitter_radius=_float_field(emitter_block, "Radius"),
            emitter_object_block_id=_emitter_object_block_id(emitter_block),
            support_level=support_level,
            warnings=tuple(warnings),
        ))

    return models


def owner_system_for_block(models: list[ParticleSystemModel], block_id: int) -> ParticleSystemModel | None:
    for model in models:
        if block_id in model.block_ids:
            return model
    return None


def _is_particle_system(nif: Any, block: Any) -> bool:
    return _is_subtype(nif, block.type_name, "NiParticleSystem")


def _is_subtype(nif: Any, type_name: str, base_name: str) -> bool:
    schema = getattr(nif, "schema", None)
    if schema is None:
        if base_name == "NiParticleSystem":
            return type_name == "NiParticleSystem"
        if base_name == "NiPSysEmitter":
            return type_name in _SUPPORTED_EMITTERS or (
                type_name.startswith(("NiPSys", "BSPSys")) and type_name.endswith("Emitter")
            )
        if base_name == "NiPSysModifier":
            return (
                type_name == "BSWindModifier"
                or type_name.endswith("Emitter")
                or (
                    type_name.startswith(("NiPSys", "BSPSys"))
                    and "Modifier" in type_name
                )
            )
        return type_name == base_name
    return bool(schema.is_subtype_of(type_name, base_name))


def _get_block(nif: Any, block_id: int | None) -> Any | None:
    if block_id is None:
        return None
    return nif.get_block(block_id)


def _as_block_id(value: Any) -> int | None:
    if value is None:
        return None
    if isinstance(value, dict):
        value = next(
            (value[key] for key in ("block_id", "value", "Value", "Block ID", "Ref") if value.get(key) is not None),
            None,
        )
    if hasattr(value, "block_id"):
        value = value.block_id
    try:
        block_id = int(value)
    except (TypeError, ValueError):
        return None
    return block_id if block_id >= 0 else None


def _as_block_ids(value: Any) -> list[int]:
    if value is None:
        return []
    if isinstance(value, (dict, str, bytes)):
        block_id = _as_block_id(value)
        return [block_id] if block_id is not None else []

    try:
        values = list(value)
    except TypeError:
        values = [value]

    return [block_id for item in values if (block_id := _as_block_id(item)) is not None]


def _get_name(block: Any) -> str:
    return str(block.get_field("Name") or f"{block.type_name} {block.block_id}")


def _vec4_tuple(value: Any) -> tuple[float, float, float, float]:
    if isinstance(value, dict):
        return (
            float(value["x"]),
            float(value["y"]),
            float(value["z"]),
            float(value["w"]),
        )
    if isinstance(value, (list, tuple)):
        x, y, z, w = value
        return (float(x), float(y), float(z), float(w))
    return (
        float(value.x),
        float(value.y),
        float(value.z),
        float(value.w),
    )


def _atlas_offsets(data_block: Any | None) -> tuple[tuple[float, float, float, float], ...]:
    if data_block is None or not data_block.get_field("Has Texture Indices"):
        return ()

    offsets = data_block.get_field("Subtexture Offsets") or []
    expected_count = data_block.get_field("Num Subtexture Offsets")
    if expected_count is not None:
        offsets = list(offsets)[:int(expected_count)]

    return tuple(_atlas_rect_tuple(offset) for offset in offsets)


def _atlas_rect_tuple(value: Any) -> tuple[float, float, float, float]:
    u, width, v, height = _vec4_tuple(value)
    return (u, u + width, v, v + height)


def _source_texture(shader_property_block: Any | None) -> str | None:
    return _string_field(shader_property_block, "Source Texture")


def _greyscale_texture(shader_property_block: Any | None) -> str | None:
    return _string_field(shader_property_block, "Greyscale Texture")


def _string_field(block: Any | None, field_name: str) -> str | None:
    if block is None:
        return None
    value = block.get_field(field_name)
    if isinstance(value, list):
        value = "".join(str(part) for part in value)
    if value is None:
        return None
    text = str(value).strip().rstrip("\x00")
    return text or None


def _shader_flag(shader_property_block: Any | None, flag_name: str) -> bool:
    if shader_property_block is None:
        return False
    flags = shader_property_block.get_field("Shader Flags 1") or []
    if isinstance(flags, list):
        return flag_name in flags
    bit = _SHADER_FLAG_BITS.get(flag_name)
    if bit is None:
        return False
    try:
        return (int(flags) & (1 << bit)) != 0
    except (TypeError, ValueError):
        return False


def _color_field(block: Any | None, field_name: str) -> tuple[float, float, float, float] | None:
    if block is None:
        return None
    value = block.get_field(field_name)
    if value is None:
        return None
    if isinstance(value, dict):
        return (
            float(value.get("r", value.get("R", value.get("x", value.get("X", 1.0))))),
            float(value.get("g", value.get("G", value.get("y", value.get("Y", 1.0))))),
            float(value.get("b", value.get("B", value.get("z", value.get("Z", 1.0))))),
            float(value.get("a", value.get("A", value.get("w", value.get("W", 1.0))))),
        )
    if isinstance(value, (list, tuple)):
        r, g, b, a = value
        return (float(r), float(g), float(b), float(a))
    return (
        float(value.r),
        float(value.g),
        float(value.b),
        float(value.a),
    )


def _float_field(block: Any | None, field_name: str) -> float | None:
    if block is None:
        return None
    value = block.get_field(field_name)
    return float(value) if value is not None else None


def _emitter_object_block_id(emitter_block: Any | None) -> int | None:
    if emitter_block is None:
        return None
    block_id = _as_block_id(emitter_block.get_field("Emitter Object"))
    if block_id is not None:
        return block_id
    return _as_block_id(emitter_block.get_field("Emitter"))


def _max_particles(data_block: Any | None) -> int | None:
    if data_block is None:
        return None
    value = data_block.get_field("BS Max Vertices")
    return int(value) if value is not None else None


def _collect_refs_from_fields(block: Any, field_names: tuple[str, ...]) -> tuple[int, ...]:
    return tuple(
        ref_id
        for field_name in field_names
        for ref_id in _as_block_ids(block.get_field(field_name))
    )


def _classify_model(
    system_block: Any,
    data_block: Any | None,
    emitter_block: Any | None,
) -> tuple[list[ParticleWarning], ParticleSupportLevel]:
    warnings = []
    if data_block is None:
        warnings.append(ParticleWarning(system_block.block_id, "missing NiPSysData"))
    if emitter_block is None:
        warnings.append(ParticleWarning(system_block.block_id, "missing emitter"))

    if warnings:
        return warnings, ParticleSupportLevel.UNSUPPORTED

    if emitter_block.type_name not in _SUPPORTED_EMITTERS:
        warnings.append(ParticleWarning(
            emitter_block.block_id,
            f"unsupported emitter {emitter_block.type_name}",
        ))
        return warnings, ParticleSupportLevel.DIAGNOSTIC_ONLY

    return warnings, ParticleSupportLevel.SUPPORTED


def _find_emitter(nif: Any, modifier_blocks: list[Any]) -> Any | None:
    for modifier_block in modifier_blocks:
        if _is_subtype(nif, modifier_block.type_name, "NiPSysEmitter"):
            return modifier_block
    return None


def _collect_modifier_entries(
    nif: Any,
    root_block_ids: tuple[int, ...],
) -> tuple[_ModifierEntry, ...]:
    entries: list[_ModifierEntry] = []
    visited: set[int] = set()

    def visit(block_id: int, parent_block_id: int | None, depth: int, *, direct: bool) -> None:
        if block_id in visited:
            return
        block = _get_block(nif, block_id)
        if block is None:
            return
        if not direct and not _is_particle_modifier_block(nif, block):
            return

        visited.add(block_id)
        entries.append(_ModifierEntry(block_id, block, parent_block_id, depth))

        for child_block_id in _child_modifier_refs(nif, block):
            visit(child_block_id, block_id, depth + 1, direct=False)

    for root_block_id in root_block_ids:
        visit(root_block_id, None, 0, direct=True)

    return tuple(entries)


def _child_modifier_refs(nif: Any, block: Any) -> tuple[int, ...]:
    return _ordered_unique(
        ref_id
        for _, ref_ids in _iter_ref_fields(nif, block)
        for ref_id in ref_ids
        if ref_id != getattr(block, "block_id", None)
        and (ref_block := _get_block(nif, ref_id)) is not None
        and _is_particle_modifier_block(nif, ref_block)
    )


def _iter_ref_fields(nif: Any, block: Any) -> tuple[tuple[str, tuple[int, ...]], ...]:
    schema = getattr(nif, "schema", None)
    get_all_ref_fields = getattr(block, "get_all_ref_fields", None)
    if schema is not None and callable(get_all_ref_fields):
        return tuple(
            (field_name, tuple(ref_ids))
            for field_name, ref_ids in get_all_ref_fields(schema)
            if field_name != "Target"
        )

    fields = getattr(block, "fields", None)
    if fields is None:
        fields = getattr(block, "_fields", {}).items()
    return tuple(
        (name, tuple(_as_block_ids(value)))
        for name, value in fields
        if name != "Target" and "Modifier" in name
    )


def _is_particle_modifier_block(nif: Any, block: Any) -> bool:
    return (
        _is_subtype(nif, block.type_name, "NiPSysModifier")
        or _is_subtype(nif, block.type_name, "NiPSysEmitter")
    )


def _ordered_unique(block_ids: Any) -> tuple[int, ...]:
    seen = set()
    unique_ids = []
    for block_id in block_ids:
        if block_id in seen:
            continue
        seen.add(block_id)
        unique_ids.append(block_id)
    return tuple(unique_ids)
