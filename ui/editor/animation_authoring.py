"""Universal NIF animation controller authoring helpers."""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Iterable


class ValueKind(str, Enum):
    FLOAT = "float"
    BOOL = "bool"
    POINT3 = "point3"
    TRANSFORM = "transform"
    PARTICLE = "particle"
    MORPH = "morph"
    EXTRA_DATA = "extra_data"
    MANAGER = "manager"
    CUSTOM = "custom"


class LinkContext(str, Enum):
    STANDALONE = "standalone"
    SEQUENCE = "sequence"
    MANAGER = "manager"


class SupportTier(str, Enum):
    FRIENDLY = "friendly"
    ADVANCED = "advanced"
    READ_ONLY = "read_only"
    BLOCKED = "blocked"


@dataclass(frozen=True)
class AuthoringTarget:
    block_id: int
    display_name: str
    target_kind: str
    property_type: str = ""


@dataclass(frozen=True)
class ControllerChainSpec:
    controller_type: str
    target: AuthoringTarget
    value_kind: ValueKind
    interpolator_type: str
    data_type: str
    link_context: LinkContext
    keys: list[tuple[float, Any]]
    controlled_fields: dict[str, Any] = field(default_factory=dict)
    start_time: float = 0.0
    stop_time: float = 1.0
    frequency: float = 1.0
    phase: float = 0.0
    flags: int = 72
    sequence_block_id: int = -1
    node_name: str = ""


@dataclass(frozen=True)
class ControllerChainResult:
    controller_id: int
    interpolator_id: int
    data_id: int
    sequence_block_id: int = -1


@dataclass(frozen=True)
class RemoveResult:
    removed_block_ids: list[int]
    detached_controller_id: int


@dataclass(frozen=True)
class ControllerRegistryEntry:
    controller_type: str
    target_kind: str
    value_kind: ValueKind
    interpolator_type: str = ""
    data_type: str = ""
    controlled_field: str = ""
    link_contexts: tuple[LinkContext, ...] = (LinkContext.SEQUENCE,)
    support_tier: SupportTier = SupportTier.ADVANCED
    friendly_name: str = ""
    default_flags: int = 72
    default_frequency: float = 1.0
    default_phase: float = 0.0
    unsupported_reason: str = ""


@dataclass(frozen=True)
class ControllerTemplate:
    template_id: str
    display_name: str
    chain_specs: tuple[ControllerChainSpec, ...]
    friendly: bool = False
    authorable: bool = True
    unsupported_reason: str = ""


_CONTROLLED_FIELDS = (
    "Controlled Variable",
    "Controlled Color",
    "Controlled Float",
    "Controlled U Short",
)

_NUMERIC_FLOAT_TOKENS = (
    "Float",
    "Dimmer",
    "Radius",
    "Strength",
    "FOV",
    "Period",
    "Intensity",
)

_DEFAULT_TYPES = {
    ValueKind.FLOAT: ("NiFloatInterpolator", "NiFloatData"),
    ValueKind.BOOL: ("NiBoolInterpolator", "NiBoolData"),
    ValueKind.POINT3: ("NiPoint3Interpolator", "NiPosData"),
    ValueKind.TRANSFORM: ("NiTransformInterpolator", "NiTransformData"),
    ValueKind.PARTICLE: ("NiFloatInterpolator", "NiFloatData"),
}


def iter_schema_controller_types(schema: Any) -> Iterable[str]:
    for type_name, obj in sorted(schema.niobjects.items()):
        if getattr(obj, "abstract", getattr(obj, "is_abstract", False)):
            continue
        if _is_schema_controller_type(schema, type_name):
            yield type_name


def build_controller_registry(schema: Any) -> dict[str, ControllerRegistryEntry]:
    registry = {
        controller_type: _build_inferred_entry(schema, controller_type)
        for controller_type in iter_schema_controller_types(schema)
    }
    registry.update(
        {
            controller_type: entry
            for controller_type, entry in _friendly_overrides().items()
            if controller_type in registry
        }
    )
    return registry


def build_controller_templates(registry: dict[str, ControllerRegistryEntry]) -> list[ControllerTemplate]:
    dummy = AuthoringTarget(-1, "", "", "")
    templates: list[ControllerTemplate] = [
        ControllerTemplate(
            template_id="texture_scroll",
            display_name="Texture Scroll",
            friendly=True,
            chain_specs=(
                _template_chain(
                    registry["BSEffectShaderPropertyFloatController"],
                    dummy,
                    {"Controlled Variable": "U Offset"},
                ),
                _template_chain(
                    registry["BSEffectShaderPropertyFloatController"],
                    dummy,
                    {"Controlled Variable": "V Offset"},
                ),
            ),
        ),
        ControllerTemplate(
            template_id="glow_pulse",
            display_name="Glow Pulse",
            friendly=True,
            chain_specs=(
                _template_chain(
                    registry["BSEffectShaderPropertyFloatController"],
                    dummy,
                    {"Controlled Variable": "EmissiveMultiple"},
                ),
            ),
        ),
        ControllerTemplate(
            template_id="alpha_flicker",
            display_name="Alpha Flicker",
            friendly=True,
            chain_specs=(
                _template_chain(
                    registry["BSEffectShaderPropertyFloatController"],
                    dummy,
                    {"Controlled Variable": "Alpha Transparency"},
                ),
            ),
        ),
    ]
    covered = {chain.controller_type for template in templates for chain in template.chain_specs}
    for controller_type, entry in sorted(registry.items()):
        if controller_type in covered or entry.value_kind is ValueKind.MANAGER:
            continue
        authorable, unsupported_reason = _template_authorability(entry)
        templates.append(
            ControllerTemplate(
                template_id=f"advanced:{controller_type}",
                display_name=entry.friendly_name or controller_type,
                chain_specs=(_template_chain(entry, dummy, {}),),
                friendly=False,
                authorable=authorable,
                unsupported_reason=unsupported_reason,
            )
        )
    return templates


def add_controller_chain(nif: Any, spec: ControllerChainSpec) -> ControllerChainResult:
    _validate_controller_chain_spec(spec)
    if spec.link_context is LinkContext.STANDALONE:
        return _add_standalone_chain(nif, spec)
    if spec.link_context is LinkContext.SEQUENCE:
        return _add_sequence_chain(nif, spec)
    raise ValueError(f"Unsupported link context: {spec.link_context}")


def remove_standalone_controller(nif: Any, target_block_id: int, controller_id: int) -> RemoveResult:
    target = nif.get_block(target_block_id)
    if target is None:
        raise ValueError(f"Target block not found: {target_block_id}")

    _unlink_controller_from_target_chain(nif, target, controller_id)
    controller = nif.get_block(controller_id)
    if controller is None:
        return RemoveResult([], controller_id)

    candidate_ids = [controller_id]
    interp_id = _ref_id(controller.get_field("Interpolator"))
    if interp_id >= 0:
        candidate_ids.append(interp_id)
        interp = nif.get_block(interp_id)
        if interp is not None:
            data_id = _ref_id(interp.get_field("Data"))
            if data_id >= 0:
                candidate_ids.append(data_id)

    removable = _removable_owned_blocks(nif, candidate_ids)
    nif.remove_blocks(removable)
    return RemoveResult(removable, controller_id)


def remove_sequence_controller(nif: Any, sequence_block_id: int, controller_id: int) -> RemoveResult:
    seq = nif.get_block(sequence_block_id)
    if seq is None:
        raise ValueError(f"Sequence block not found: {sequence_block_id}")
    controlled = list(seq.get_field("Controlled Blocks") or [])
    kept = []
    removed_entry = None
    for entry in controlled:
        if _ref_id(entry.get("Controller")) == controller_id:
            removed_entry = entry
        else:
            kept.append(entry)
    if removed_entry is None:
        raise ValueError(f"Controller {controller_id} is not linked from sequence {sequence_block_id}")
    seq.set_field("Controlled Blocks", kept)
    seq.set_field("Num Controlled Blocks", len(kept))

    candidate_ids = [controller_id]
    controller = nif.get_block(controller_id)
    interp_id = _ref_id(removed_entry.get("Interpolator"))
    if interp_id < 0 and controller is not None:
        interp_id = _ref_id(controller.get_field("Interpolator"))
    if interp_id >= 0:
        candidate_ids.append(interp_id)
        interp = nif.get_block(interp_id)
        if interp is not None:
            data_id = _ref_id(interp.get_field("Data"))
            if data_id >= 0:
                candidate_ids.append(data_id)

    removable = _removable_owned_blocks(nif, candidate_ids)
    nif.remove_blocks(removable)
    return RemoveResult(removable, controller_id)


def _template_chain(
    entry: ControllerRegistryEntry,
    target: AuthoringTarget,
    controlled_fields: dict[str, Any],
) -> ControllerChainSpec:
    value = _default_value_for_kind(entry.value_kind)
    return ControllerChainSpec(
        controller_type=entry.controller_type,
        target=target,
        value_kind=entry.value_kind,
        interpolator_type=entry.interpolator_type,
        data_type=entry.data_type,
        link_context=entry.link_contexts[0],
        keys=[(0.0, value), (1.0, value)],
        controlled_fields=controlled_fields,
        start_time=0.0,
        stop_time=1.0,
        flags=entry.default_flags,
        frequency=entry.default_frequency,
        phase=entry.default_phase,
    )


def _template_authorability(entry: ControllerRegistryEntry) -> tuple[bool, str]:
    if not entry.interpolator_type or not entry.data_type:
        return False, "Controller does not expose key data"
    if entry.support_tier is SupportTier.READ_ONLY:
        return False, entry.unsupported_reason or "Controller is read-only"
    if entry.support_tier is SupportTier.BLOCKED:
        return False, entry.unsupported_reason or "Controller authoring is blocked"
    return True, ""


def _default_value_for_kind(value_kind: ValueKind) -> Any:
    if value_kind is ValueKind.POINT3:
        return (1.0, 1.0, 1.0)
    if value_kind is ValueKind.TRANSFORM:
        return {"translation": (0.0, 0.0, 0.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0}
    if value_kind is ValueKind.BOOL:
        return True
    return 0.0


def _add_standalone_chain(nif: Any, spec: ControllerChainSpec) -> ControllerChainResult:
    target_block = nif.get_block(spec.target.block_id)
    if target_block is None:
        raise ValueError(f"Target block not found: {spec.target.block_id}")
    data_fields = _make_data_fields(spec.value_kind, spec.keys)
    _ensure_controller_chain_appendable(nif, target_block)

    data = nif.add_block(spec.data_type)
    for field_name, value in data_fields.items():
        data.set_field(field_name, value)
    interp = nif.add_block(spec.interpolator_type, {"Data": data.block_id})
    controller = nif.add_block(spec.controller_type)
    controller.set_field("Next Controller", -1)
    controller.set_field("Flags", int(spec.flags))
    controller.set_field("Frequency", float(spec.frequency))
    controller.set_field("Phase", float(spec.phase))
    controller.set_field("Start Time", float(spec.start_time))
    controller.set_field("Stop Time", float(spec.stop_time))
    controller.set_field("Target", int(spec.target.block_id))
    controller.set_field("Interpolator", int(interp.block_id))
    for field_name, value in spec.controlled_fields.items():
        controller.set_field(field_name, value)

    _append_controller_to_chain(nif, target_block, controller.block_id)
    return ControllerChainResult(controller.block_id, interp.block_id, data.block_id)


def _add_sequence_chain(nif: Any, spec: ControllerChainSpec) -> ControllerChainResult:
    seq = nif.get_block(spec.sequence_block_id)
    if seq is None:
        raise ValueError(f"Sequence block not found: {spec.sequence_block_id}")
    target = nif.get_block(spec.target.block_id)
    if target is None:
        raise ValueError(f"Target block not found: {spec.target.block_id}")
    data_fields = _make_data_fields(spec.value_kind, spec.keys)

    data = nif.add_block(spec.data_type)
    for field_name, value in data_fields.items():
        data.set_field(field_name, value)
    interp = nif.add_block(spec.interpolator_type, {"Data": data.block_id})
    controller = nif.add_block(spec.controller_type)
    controller.set_field("Next Controller", -1)
    controller.set_field("Flags", int(spec.flags))
    controller.set_field("Frequency", float(spec.frequency))
    controller.set_field("Phase", float(spec.phase))
    controller.set_field("Start Time", float(spec.start_time))
    controller.set_field("Stop Time", float(spec.stop_time))
    controller.set_field("Target", int(spec.target.block_id))
    controller.set_field("Interpolator", int(interp.block_id))
    for field_name, value in spec.controlled_fields.items():
        controller.set_field(field_name, value)

    controlled = list(seq.get_field("Controlled Blocks") or [])
    controlled.append(_controlled_block_entry(spec, controller.block_id, interp.block_id))
    seq.set_field("Controlled Blocks", controlled)
    seq.set_field("Num Controlled Blocks", len(controlled))
    seq.set_field("Start Time", _expanded_start_time(seq.get_field("Start Time"), spec.start_time))
    seq.set_field("Stop Time", _expanded_stop_time(seq.get_field("Stop Time"), spec.stop_time))
    return ControllerChainResult(controller.block_id, interp.block_id, data.block_id, seq.block_id)


def _validate_controller_chain_spec(spec: ControllerChainSpec) -> None:
    if not spec.controller_type:
        raise ValueError("Controller chain spec is missing a controller type")
    if not spec.interpolator_type or not spec.data_type:
        raise ValueError(f"{spec.controller_type} does not expose key data")
    if spec.link_context is LinkContext.SEQUENCE and spec.sequence_block_id < 0:
        raise ValueError("Sequence controller chains require a sequence block")


def _controlled_block_entry(spec: ControllerChainSpec, controller_id: int, interpolator_id: int) -> dict[str, Any]:
    controller_id_value = next(iter(spec.controlled_fields.values()), "")
    return {
        "Interpolator": interpolator_id,
        "Controller": controller_id,
        "Priority": 0,
        "Node Name": spec.node_name or spec.target.display_name,
        "Property Type": spec.target.property_type,
        "Controller Type": spec.controller_type,
        "Controller ID": str(controller_id_value),
        "Interpolator ID": "",
    }


def _expanded_start_time(current: Any, start_time: float) -> float:
    if current is None:
        return float(start_time)
    return min(float(current), float(start_time))


def _expanded_stop_time(current: Any, stop_time: float) -> float:
    if current is None:
        return float(stop_time)
    return max(float(current), float(stop_time))


def _append_controller_to_chain(nif: Any, target_block: Any, controller_id: int) -> None:
    first = _ref_id(target_block.get_field("Controller"))
    if first < 0:
        target_block.set_field("Controller", controller_id)
        return
    current_id = first
    visited: set[int] = set()
    while current_id >= 0 and current_id not in visited:
        visited.add(current_id)
        current = nif.get_block(current_id)
        if current is None:
            break
        next_id = _ref_id(current.get_field("Next Controller"))
        if next_id < 0:
            current.set_field("Next Controller", controller_id)
            return
        current_id = next_id
    raise ValueError("Controller chain contains a cycle")


def _unlink_controller_from_target_chain(nif: Any, target: Any, controller_id: int) -> tuple[int, int]:
    first_id = _ref_id(target.get_field("Controller"))
    if first_id == controller_id:
        controller = nif.get_block(controller_id)
        next_id = _ref_id(controller.get_field("Next Controller")) if controller is not None else -1
        target.set_field("Controller", next_id)
        return -1, next_id

    current_id = first_id
    visited: set[int] = set()
    while current_id >= 0 and current_id not in visited:
        visited.add(current_id)
        current = nif.get_block(current_id)
        if current is None:
            break
        next_id = _ref_id(current.get_field("Next Controller"))
        if next_id == controller_id:
            removed = nif.get_block(controller_id)
            removed_next = _ref_id(removed.get_field("Next Controller")) if removed is not None else -1
            current.set_field("Next Controller", removed_next)
            return current_id, removed_next
        current_id = next_id
    raise ValueError(f"Controller {controller_id} is not linked from target {target.block_id}")


def _removable_owned_blocks(nif: Any, candidate_ids: list[int]) -> list[int]:
    removable = set(candidate_ids)
    changed = True
    while changed:
        changed = False
        for block_id in candidate_ids:
            if block_id not in removable:
                continue
            if _ref_count(nif, block_id, ignore=removable) > 0:
                removable.remove(block_id)
                changed = True
    return [block_id for block_id in candidate_ids if block_id in removable]


def _ref_count(nif: Any, block_id: int, *, ignore: set[int] | None = None) -> int:
    ignored = ignore or set()
    count = 0
    for block in nif.blocks:
        if block.block_id in ignored:
            continue
        count += sum(1 for ref in block.get_refs(nif.schema) if ref == block_id)
    return count


def _ensure_controller_chain_appendable(nif: Any, target_block: Any) -> None:
    current_id = _ref_id(target_block.get_field("Controller"))
    visited: set[int] = set()
    while current_id >= 0:
        if current_id in visited:
            raise ValueError("Controller chain contains a cycle")
        visited.add(current_id)
        current = nif.get_block(current_id)
        if current is None:
            raise ValueError(f"Controller chain references missing block: {current_id}")
        current_id = _ref_id(current.get_field("Next Controller"))


def _make_key_data(value_kind: ValueKind, keys: list[tuple[float, Any]]) -> dict[str, Any]:
    return {
        "Interpolation": 1,
        "Num Keys": len(keys),
        "Keys": [_make_key(value_kind, time, value) for time, value in keys],
    }


def _make_data_fields(value_kind: ValueKind, keys: list[tuple[float, Any]]) -> dict[str, Any]:
    if value_kind is ValueKind.TRANSFORM:
        return _make_transform_data_fields(keys)
    return {_data_field_name(value_kind): _make_key_data(value_kind, keys)}


def _make_transform_data_fields(keys: list[tuple[float, Any]]) -> dict[str, Any]:
    translations: list[tuple[float, Any]] = []
    scales: list[tuple[float, Any]] = []
    rotations: list[list[tuple[float, float]]] = [[], [], []]
    for time, value in keys:
        transform = _coerce_transform_value(value)
        translations.append((time, transform["translation"]))
        scales.append((time, transform["scale"]))
        for axis_index, axis_value in enumerate(transform["rotation"]):
            rotations[axis_index].append((time, axis_value))

    return {
        "Num Rotation Keys": 1 if keys else 0,
        "Rotation Type": 4 if keys else 1,
        "Quaternion Keys": [],
        "XYZ Rotations": [_make_float_key_group(axis_keys) for axis_keys in rotations],
        "Translations": _make_key_data(ValueKind.POINT3, translations),
        "Scales": _make_key_data(ValueKind.FLOAT, scales),
    }


def _make_float_key_group(keys: list[tuple[float, float]]) -> dict[str, Any]:
    return {
        "Interpolation": 1,
        "Num Keys": len(keys),
        "Keys": [{"Time": float(time), "Value": float(value)} for time, value in keys],
    }


def _coerce_transform_value(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("Transform keys must be dictionaries")
    return {
        "translation": _coerce_point3(value.get("translation", (0.0, 0.0, 0.0))),
        "rotation": _coerce_point3(value.get("rotation", (0.0, 0.0, 0.0))),
        "scale": float(value.get("scale", 1.0)),
    }


def _coerce_point3(value: Any) -> tuple[float, float, float]:
    if isinstance(value, dict):
        return (float(value.get("x", 0.0)), float(value.get("y", 0.0)), float(value.get("z", 0.0)))
    x, y, z = value
    return (float(x), float(y), float(z))


def _make_key(value_kind: ValueKind, time: float, value: Any) -> dict[str, Any]:
    if value_kind is ValueKind.POINT3:
        x, y, z = _coerce_point3(value)
        encoded_value = {"x": float(x), "y": float(y), "z": float(z)}
    elif value_kind is ValueKind.BOOL:
        encoded_value = bool(value)
    else:
        encoded_value = float(value)
    return {"Time": float(time), "Value": encoded_value, "Interpolation": 1}


def _data_field_name(value_kind: ValueKind) -> str:
    return "Data"


def _ref_id(value: Any) -> int:
    if isinstance(value, dict):
        value = value.get("value", value.get("Value", -1))
    try:
        return int(value)
    except (TypeError, ValueError):
        return -1


def _is_schema_controller_type(schema: Any, type_name: str) -> bool:
    return (
        type_name.endswith("Controller")
        or type_name.endswith("Ctlr")
        or type_name in ("NiControllerManager", "NiControllerSequence")
        or schema.is_subtype_of(type_name, "NiTimeController")
    )


def _build_inferred_entry(schema: Any, controller_type: str) -> ControllerRegistryEntry:
    value_kind = _infer_value_kind(schema, controller_type)
    field_names = _field_names(schema, controller_type)
    interpolator_type, data_type = _default_types(field_names, value_kind)
    return ControllerRegistryEntry(
        controller_type=controller_type,
        target_kind=_infer_target_kind(controller_type, value_kind),
        value_kind=value_kind,
        interpolator_type=interpolator_type,
        data_type=data_type,
        controlled_field=_controlled_field(schema, controller_type),
        link_contexts=_link_contexts(schema, controller_type, value_kind),
        support_tier=SupportTier.READ_ONLY if value_kind is ValueKind.MANAGER else SupportTier.ADVANCED,
    )


def _default_types(field_names: set[str], value_kind: ValueKind) -> tuple[str, str]:
    if value_kind is ValueKind.PARTICLE and "Interpolator" not in field_names:
        return "", ""
    return _DEFAULT_TYPES.get(value_kind, ("", ""))


def _infer_value_kind(schema: Any, controller_type: str) -> ValueKind:
    if controller_type in ("NiControllerManager", "NiControllerSequence"):
        return ValueKind.MANAGER
    if _is_particle_controller(controller_type):
        return ValueKind.PARTICLE
    if "Morph" in controller_type:
        return ValueKind.MORPH
    if "ExtraData" in controller_type:
        return ValueKind.EXTRA_DATA
    if schema.is_subtype_of(controller_type, "NiBoolInterpController") or "VisController" in controller_type:
        return ValueKind.BOOL
    if schema.is_subtype_of(controller_type, "NiPoint3InterpController") or "ColorController" in controller_type:
        return ValueKind.POINT3
    if any(token in controller_type for token in ("Transform", "Keyframe", "Path")):
        return ValueKind.TRANSFORM
    if schema.is_subtype_of(controller_type, "NiFloatInterpController") or any(
        token in controller_type for token in _NUMERIC_FLOAT_TOKENS
    ):
        return ValueKind.FLOAT
    return ValueKind.CUSTOM


def _infer_target_kind(controller_type: str, value_kind: ValueKind) -> str:
    if "EffectShaderProperty" in controller_type:
        return "effect_shader_property"
    if "LightingShaderProperty" in controller_type:
        return "lighting_shader_property"
    if "ShaderProperty" in controller_type:
        return "shader_property"
    if "Light" in controller_type:
        return "light"
    if _is_particle_controller(controller_type):
        return "particle_system"
    if "ExtraData" in controller_type:
        return "extra_data"
    if "Morph" in controller_type:
        return "geometry"
    if value_kind is ValueKind.MANAGER:
        return "manager"
    return "node"


def _controlled_field(schema: Any, controller_type: str) -> str:
    field_names = _field_names(schema, controller_type)
    for field_name in _CONTROLLED_FIELDS:
        if field_name in field_names:
            return field_name
    return ""


def _link_contexts(schema: Any, controller_type: str, value_kind: ValueKind) -> tuple[LinkContext, ...]:
    if value_kind is ValueKind.MANAGER:
        return (LinkContext.MANAGER,)
    field_names = _field_names(schema, controller_type)
    if "Next Controller" in field_names and "Target" in field_names:
        return (LinkContext.STANDALONE, LinkContext.SEQUENCE)
    return (LinkContext.SEQUENCE,)


def _field_names(schema: Any, controller_type: str) -> set[str]:
    fields = schema.get_all_fields(controller_type)
    return {field.name for field in fields}


def _is_particle_controller(controller_type: str) -> bool:
    return any(token in controller_type for token in ("PSys", "NiPS", "BSPSys"))


def _friendly_overrides() -> dict[str, ControllerRegistryEntry]:
    return {
        "BSEffectShaderPropertyFloatController": ControllerRegistryEntry(
            controller_type="BSEffectShaderPropertyFloatController",
            target_kind="effect_shader_property",
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            controlled_field="Controlled Variable",
            link_contexts=(LinkContext.STANDALONE, LinkContext.SEQUENCE),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Effect Shader Float",
        ),
        "BSEffectShaderPropertyColorController": ControllerRegistryEntry(
            controller_type="BSEffectShaderPropertyColorController",
            target_kind="effect_shader_property",
            value_kind=ValueKind.POINT3,
            interpolator_type="NiPoint3Interpolator",
            data_type="NiPosData",
            controlled_field="Controlled Color",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Effect Shader Color",
        ),
        "BSLightingShaderPropertyFloatController": ControllerRegistryEntry(
            controller_type="BSLightingShaderPropertyFloatController",
            target_kind="lighting_shader_property",
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            controlled_field="Controlled Variable",
            link_contexts=(LinkContext.STANDALONE, LinkContext.SEQUENCE),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Lighting Shader Float",
        ),
        "BSLightingShaderPropertyColorController": ControllerRegistryEntry(
            controller_type="BSLightingShaderPropertyColorController",
            target_kind="lighting_shader_property",
            value_kind=ValueKind.POINT3,
            interpolator_type="NiPoint3Interpolator",
            data_type="NiPosData",
            controlled_field="Controlled Color",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Lighting Shader Color",
        ),
        "BSLightingShaderPropertyUShortController": ControllerRegistryEntry(
            controller_type="BSLightingShaderPropertyUShortController",
            target_kind="lighting_shader_property",
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            controlled_field="Controlled U Short",
            link_contexts=(LinkContext.SEQUENCE,),
            friendly_name="Lighting Shader UShort",
        ),
        "NiAlphaController": ControllerRegistryEntry(
            controller_type="NiAlphaController",
            target_kind="node",
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_contexts=(LinkContext.STANDALONE, LinkContext.SEQUENCE),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Alpha",
        ),
        "NiVisController": ControllerRegistryEntry(
            controller_type="NiVisController",
            target_kind="node",
            value_kind=ValueKind.BOOL,
            interpolator_type="NiBoolInterpolator",
            data_type="NiBoolData",
            link_contexts=(LinkContext.STANDALONE, LinkContext.SEQUENCE),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Visibility",
        ),
        "NiTransformController": ControllerRegistryEntry(
            controller_type="NiTransformController",
            target_kind="node",
            value_kind=ValueKind.TRANSFORM,
            interpolator_type="NiTransformInterpolator",
            data_type="NiTransformData",
            link_contexts=(LinkContext.STANDALONE, LinkContext.SEQUENCE),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Transform",
        ),
        "NiLightDimmerController": ControllerRegistryEntry(
            controller_type="NiLightDimmerController",
            target_kind="light",
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Light Dimmer",
        ),
        "NiLightRadiusController": ControllerRegistryEntry(
            controller_type="NiLightRadiusController",
            target_kind="light",
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Light Radius",
        ),
        "NiLightColorController": ControllerRegistryEntry(
            controller_type="NiLightColorController",
            target_kind="light",
            value_kind=ValueKind.POINT3,
            interpolator_type="NiPoint3Interpolator",
            data_type="NiPosData",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Light Color",
        ),
        "NiTextureTransformController": ControllerRegistryEntry(
            controller_type="NiTextureTransformController",
            target_kind="shader_property",
            value_kind=ValueKind.TRANSFORM,
            interpolator_type="NiTransformInterpolator",
            data_type="NiTransformData",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Texture Transform",
        ),
        "NiFlipController": ControllerRegistryEntry(
            controller_type="NiFlipController",
            target_kind="shader_property",
            value_kind=ValueKind.CUSTOM,
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Texture Flip",
        ),
        "NiPSysEmitterCtlr": ControllerRegistryEntry(
            controller_type="NiPSysEmitterCtlr",
            target_kind="particle_system",
            value_kind=ValueKind.PARTICLE,
            interpolator_type="NiFloatInterpolator",
            data_type="NiPSysEmitterCtlrData",
            link_contexts=(LinkContext.SEQUENCE,),
            support_tier=SupportTier.FRIENDLY,
            friendly_name="Particle Emitter",
        ),
    }
