from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Mapping

from creation_lib.nif.actions import SnapshotAction

from .builder_model import (
    EmissionShape,
    ModifierKind,
    ParticleEffectDraft,
    ParticleModifierDraft,
    ParticleSystemDraft,
)
from .catalog import MODIFIER_CATALOG
from .model import ParticleSystemModel


_EMITTER_SHAPES = {
    "NiPSysSphereEmitter": EmissionShape.SPHERE,
    "NiPSysCylinderEmitter": EmissionShape.CYLINDER,
    "NiPSysBoxEmitter": EmissionShape.BOX,
}
_MODIFIER_KINDS = {
    "NiPSysGravityModifier": ModifierKind.GRAVITY,
    "NiPSysDragModifier": ModifierKind.DRAG,
    "BSWindModifier": ModifierKind.WIND,
    "NiPSysRotationModifier": ModifierKind.ROTATION,
    "BSPSysScaleModifier": ModifierKind.SIZE_OVER_LIFE,
    "NiPSysGrowFadeModifier": ModifierKind.SIZE_OVER_LIFE,
    "BSPSysSimpleColorModifier": ModifierKind.COLOR_OVER_LIFE,
    "NiPSysColorModifier": ModifierKind.COLOR_OVER_LIFE,
    "NiPSysColliderManager": ModifierKind.COLLISION,
    "NiPSysSpawnModifier": ModifierKind.SPAWN_RATE,
}
_SKIPPED_MODIFIER_TYPES = {
    "NiPSysAgeDeathModifier",
    "NiPSysPositionModifier",
    "NiPSysBoundUpdateModifier",
    "NiPSysUpdateCtlr",
    "NiPSysEmitterCtlr",
    "BSPSysLODModifier",
}


@dataclass(frozen=True)
class DraftValidationIssue:
    message: str
    severity: str = "error"
    system_index: int | None = None


@dataclass(frozen=True)
class AuthoringResult:
    system_block_ids: tuple[int, ...]
    issues: tuple[DraftValidationIssue, ...]


def draft_from_particle_model(model: ParticleSystemModel) -> ParticleEffectDraft:
    raw_overrides: dict[str, Any] = {
        "source": _source_metadata(model),
    }
    emission_shape = _EMITTER_SHAPES.get(model.emitter_type or "", EmissionShape.POINT)
    if emission_shape is EmissionShape.POINT and model.emitter_type not in {None, "NiPSysBoxEmitter"}:
        raw_overrides["raw_emitter_type"] = model.emitter_type

    emitter_overrides: dict[str, Any] = {}
    if model.emitter_speed_variation is not None:
        emitter_overrides["speed_variation"] = model.emitter_speed_variation
    if model.emitter_lifetime_variation is not None:
        emitter_overrides["lifetime_variation"] = model.emitter_lifetime_variation
    if model.emitter_radius_variation is not None:
        emitter_overrides["radius_variation"] = model.emitter_radius_variation
    if emitter_overrides:
        raw_overrides["emitter"] = emitter_overrides

    unknown_modifier_types = []
    modifiers = []
    for modifier_type in model.modifier_types:
        if _should_skip_imported_modifier(model, modifier_type):
            continue
        modifier_kind = _MODIFIER_KINDS.get(modifier_type)
        if modifier_kind is None:
            unknown_modifier_types.append(modifier_type)
            continue
        modifiers.append(MODIFIER_CATALOG[modifier_kind].create_draft())
    if unknown_modifier_types:
        raw_overrides["raw_modifier_types"] = tuple(unknown_modifier_types)

    color = model.base_color or model.emitter_initial_color or (1.0, 1.0, 1.0, 1.0)
    atlas_rows, atlas_columns, subtexture_index = _infer_atlas_selection(model.atlas_offsets)
    system = ParticleSystemDraft(
        display_name=model.name,
        emission_shape=emission_shape,
        texture_path=model.source_texture,
        atlas_rows=atlas_rows,
        atlas_columns=atlas_columns,
        subtexture_index=subtexture_index,
        lifetime=model.emitter_lifetime if model.emitter_lifetime is not None else 1.0,
        speed=model.emitter_speed if model.emitter_speed is not None else 1.0,
        spread_degrees=math.degrees(max(
            model.emitter_declination_variation or 0.0,
            model.emitter_planar_angle_variation or 0.0,
        )),
        particle_size=_imported_particle_size(model),
        color_rgba=color,
        alpha=color[3],
        modifiers=tuple(modifiers),
        raw_overrides=raw_overrides,
    )
    return ParticleEffectDraft(effect_name=model.name, systems=(system,))


def validate_draft(draft: ParticleEffectDraft) -> tuple[DraftValidationIssue, ...]:
    issues: list[DraftValidationIssue] = []
    if not draft.effect_name.strip():
        issues.append(DraftValidationIssue("Particle effect name is required."))
    if not draft.systems:
        issues.append(DraftValidationIssue("Effect has no particle systems."))
        return tuple(issues)

    for index, system in enumerate(draft.systems):
        _validate_system(system, index, issues)
    return tuple(issues)


def apply_draft_to_nif(
    nif: Any,
    draft: ParticleEffectDraft,
    attach_to_block_id: int | None = 0,
) -> AuthoringResult:
    issues = validate_draft(draft)
    if issues:
        return AuthoringResult((), issues)

    system_block_ids = tuple(_write_system(nif, system, attach_to_block_id) for system in draft.systems)
    return AuthoringResult(system_block_ids, ())


def apply_draft_to_session(
    app: Any,
    draft: ParticleEffectDraft,
    attach_to_block_id: int | None = 0,
) -> AuthoringResult:
    session = _active_session(app)
    nif = getattr(session, "nif", None) if session is not None else None
    if nif is None:
        return _session_issue(app, "No active NIF session.")
    if getattr(session, "read_only", False):
        return _session_issue(app, "Cannot apply particle effect to a read-only session.")

    issues = validate_draft(draft)
    if issues:
        return _validation_result(app, issues)

    active_id = getattr(getattr(app, "registry", None), "active_id", None)
    action = SnapshotAction(_description=f"Apply particle effect: {draft.effect_name}")
    action.capture_before(nif)
    try:
        result = apply_draft_to_nif(nif, draft, attach_to_block_id=attach_to_block_id)
    except Exception as exc:
        action.undo(nif)
        return _validation_result(app, (
            DraftValidationIssue(f"Failed to apply particle effect: {exc}"),
        ))
    if result.issues:
        action.undo(nif)
        return _validation_result(app, result.issues)

    action.capture_after(nif)
    undo_manager = getattr(app, "undo_manager", None)
    if undo_manager is not None and active_id is not None:
        undo_manager.push(active_id, action)
    _mark_session_dirty(app, session)

    rebuild_scene_from_nif = getattr(app, "rebuild_scene_from_nif", None)
    if callable(rebuild_scene_from_nif) and active_id is not None:
        rebuild_scene_from_nif(active_id)

    if result.system_block_ids and active_id is not None:
        selection_mgr = getattr(app, "selection_mgr", None)
        select_by_id = getattr(selection_mgr, "select_by_id", None)
        if callable(select_by_id):
            select_by_id(active_id, result.system_block_ids[0])

    if hasattr(app, "status_text"):
        app.status_text = f"Applied particle effect: {draft.effect_name}"
    return result


def add_modifier_to_particle_system_session(
    app: Any,
    model: ParticleSystemModel,
    modifier_kind: ModifierKind,
) -> AuthoringResult:
    session = _active_session(app)
    nif = getattr(session, "nif", None) if session is not None else None
    if nif is None:
        return _session_issue(app, "No active NIF session.")
    if getattr(session, "read_only", False):
        return _session_issue(app, "Cannot edit particle effect in a read-only session.")

    active_id = getattr(getattr(app, "registry", None), "active_id", None)
    action = SnapshotAction(_description=f"Add particle modifier: {modifier_kind.value}")
    action.capture_before(nif)
    try:
        modifier_block_id = add_modifier_to_particle_system(nif, model, modifier_kind)
    except Exception as exc:
        action.undo(nif)
        return _validation_result(app, (
            DraftValidationIssue(f"Failed to add particle modifier: {exc}"),
        ))

    action.capture_after(nif)
    undo_manager = getattr(app, "undo_manager", None)
    if undo_manager is not None and active_id is not None:
        undo_manager.push(active_id, action)
    _mark_session_dirty(app, session)

    rebuild_scene_from_nif = getattr(app, "rebuild_scene_from_nif", None)
    if callable(rebuild_scene_from_nif) and active_id is not None:
        rebuild_scene_from_nif(active_id)

    selection_mgr = getattr(app, "selection_mgr", None)
    select_by_id = getattr(selection_mgr, "select_by_id", None)
    if callable(select_by_id) and active_id is not None:
        select_by_id(active_id, modifier_block_id)

    if hasattr(app, "status_text"):
        app.status_text = f"Added particle modifier: {modifier_kind.value}"
    return AuthoringResult((model.system_block_id,), ())


def remove_modifier_from_particle_system_session(
    app: Any,
    model: ParticleSystemModel,
    modifier_block_id: int,
) -> AuthoringResult:
    session = _active_session(app)
    nif = getattr(session, "nif", None) if session is not None else None
    if nif is None:
        return _session_issue(app, "No active NIF session.")
    if getattr(session, "read_only", False):
        return _session_issue(app, "Cannot edit particle effect in a read-only session.")

    active_id = getattr(getattr(app, "registry", None), "active_id", None)
    action = SnapshotAction(_description=f"Remove particle modifier: {modifier_block_id}")
    action.capture_before(nif)
    try:
        remove_modifier_from_particle_system(nif, model, modifier_block_id)
    except Exception as exc:
        action.undo(nif)
        return _validation_result(app, (
            DraftValidationIssue(f"Failed to remove particle modifier: {exc}"),
        ))

    action.capture_after(nif)
    undo_manager = getattr(app, "undo_manager", None)
    if undo_manager is not None and active_id is not None:
        undo_manager.push(active_id, action)
    _mark_session_dirty(app, session)

    rebuild_scene_from_nif = getattr(app, "rebuild_scene_from_nif", None)
    if callable(rebuild_scene_from_nif) and active_id is not None:
        rebuild_scene_from_nif(active_id)

    selection_mgr = getattr(app, "selection_mgr", None)
    select_by_id = getattr(selection_mgr, "select_by_id", None)
    if callable(select_by_id) and active_id is not None:
        select_by_id(active_id, model.system_block_id)

    if hasattr(app, "status_text"):
        app.status_text = f"Removed particle modifier: {modifier_block_id}"
    return AuthoringResult((model.system_block_id,), ())


def add_modifier_to_particle_system(
    nif: Any,
    model: ParticleSystemModel,
    modifier_kind: ModifierKind,
) -> int:
    system_block = nif.get_block(model.system_block_id)
    if system_block is None:
        raise ValueError(f"Particle system block {model.system_block_id} was not found.")

    if modifier_kind is ModifierKind.SPAWN_RATE:
        return _add_spawn_modifier_to_particle_system(nif, model, system_block)

    system = draft_from_particle_model(model).systems[0]
    modifier = MODIFIER_CATALOG[modifier_kind].create_draft()
    modifier_block = _add_explicit_modifier(
        nif,
        modifier,
        model.system_block_id,
        model.emitter_object_block_id if model.emitter_object_block_id is not None else -1,
        system,
    )
    _append_particle_modifier(system_block, modifier_block.block_id)
    return modifier_block.block_id


def remove_modifier_from_particle_system(
    nif: Any,
    model: ParticleSystemModel,
    modifier_block_id: int,
) -> int:
    modifier_block_id = int(modifier_block_id)
    system_block = nif.get_block(model.system_block_id)
    if system_block is None:
        raise ValueError(f"Particle system block {model.system_block_id} was not found.")
    if modifier_block_id not in model.modifier_block_ids:
        raise ValueError(f"Particle modifier block {modifier_block_id} is not part of this system.")

    index = model.modifier_block_ids.index(modifier_block_id)
    parent_block_id = (
        model.modifier_parent_block_ids[index]
        if index < len(model.modifier_parent_block_ids)
        else None
    )
    if parent_block_id is None:
        _detach_direct_particle_modifier(system_block, modifier_block_id)
        return modifier_block_id

    parent_block = nif.get_block(parent_block_id)
    if parent_block is None:
        raise ValueError(f"Parent modifier block {parent_block_id} was not found.")
    if not _clear_child_modifier_ref(parent_block, modifier_block_id):
        raise ValueError(
            f"Parent modifier block {parent_block_id} does not reference {modifier_block_id}."
        )
    return modifier_block_id


def _validate_system(
    system: ParticleSystemDraft,
    system_index: int,
    issues: list[DraftValidationIssue],
) -> None:
    def add(message: str) -> None:
        issues.append(DraftValidationIssue(message, system_index=system_index))

    if not system.display_name.strip():
        add("Particle system display name is required.")
    if system.atlas_rows < 1:
        add("Particle system atlas_rows must be at least 1.")
    if system.atlas_columns < 1:
        add("Particle system atlas_columns must be at least 1.")
    if system.lifetime < 0:
        add("Particle system lifetime cannot be negative.")
    if system.emission_rate < 0:
        add("Particle system emission_rate cannot be negative.")
    if system.speed < 0:
        add("Particle system speed cannot be negative.")
    if system.particle_size <= 0:
        add("Particle system particle_size must be greater than 0.")
    for modifier in system.modifiers:
        if modifier.enabled and not modifier.display_name.strip():
            add("Particle modifier display name is required.")


def _source_metadata(model: ParticleSystemModel) -> dict[str, Any]:
    source = {
        "nif_id": model.nif_id,
        "system_block_id": model.system_block_id,
    }
    for key in ("emitter_block_id", "emitter_object_block_id"):
        value = getattr(model, key)
        if value is not None:
            source[key] = value
    return source


def _should_skip_imported_modifier(model: ParticleSystemModel, modifier_type: str) -> bool:
    return (
        modifier_type == model.emitter_type
        or modifier_type.endswith("Emitter")
        or modifier_type in _SKIPPED_MODIFIER_TYPES
    )


def _imported_particle_size(model: ParticleSystemModel) -> float:
    if model.emitter_initial_radius is not None:
        return model.emitter_initial_radius
    if model.emitter_radius is not None:
        return model.emitter_radius
    return 1.0


def _infer_atlas_selection(offsets: tuple[tuple[float, float, float, float], ...]) -> tuple[int, int, int]:
    if not offsets:
        return (1, 1, 0)

    if len(offsets) == 1:
        left, right, top, bottom = offsets[0]
        width = _rounded(right - left)
        height = _rounded(bottom - top)
        if width > 0 and height > 0:
            columns = max(1, int(round(1.0 / width)))
            rows = max(1, int(round(1.0 / height)))
            column = min(columns - 1, max(0, int(round(_rounded(left) / width))))
            row = min(rows - 1, max(0, int(round(_rounded(top) / height))))
            return (rows, columns, row * columns + column)
        return (1, 1, 0)

    widths = {_rounded(right - left) for left, right, _top, _bottom in offsets}
    heights = {_rounded(bottom - top) for _left, _right, top, bottom in offsets}
    columns = {_rounded(left) for left, _right, _top, _bottom in offsets}
    rows = {_rounded(top) for _left, _right, top, _bottom in offsets}
    if len(widths) == 1 and len(heights) == 1 and len(offsets) == len(columns) * len(rows):
        return (len(rows), len(columns), 0)
    return (1, len(offsets), 0)


def _rounded(value: float) -> float:
    return round(float(value), 6)


def _active_session(app: Any) -> Any | None:
    try:
        return app.registry.active_session
    except (AttributeError, KeyError):
        return None


def _session_issue(app: Any, message: str) -> AuthoringResult:
    issue = DraftValidationIssue(message)
    if hasattr(app, "status_text"):
        app.status_text = message
    return AuthoringResult((), (issue,))


def _validation_result(app: Any, issues: tuple[DraftValidationIssue, ...]) -> AuthoringResult:
    if issues and hasattr(app, "status_text"):
        app.status_text = issues[0].message
    return AuthoringResult((), issues)


def _mark_session_dirty(app: Any, session: Any) -> None:
    if hasattr(session, "dirty"):
        session.dirty = True
    if hasattr(app, "_nif_dirty"):
        app._nif_dirty = True


def _write_system(nif: Any, system: ParticleSystemDraft, attach_to_block_id: int | None) -> int:
    emitter_object_block_id = _emitter_object_block_id(nif, attach_to_block_id)
    data = nif.add_block("NiPSysData", _data_fields(system))
    shader = nif.add_block("BSEffectShaderProperty", _shader_fields(system))
    alpha = nif.add_block("NiAlphaProperty", _alpha_fields())
    particle_system = nif.add_block("NiParticleSystem", {
        "Name": system.display_name,
        "Data": data.block_id,
        "Shader Property": shader.block_id,
        "Alpha Property": alpha.block_id,
        "World Space": 1,
        "Controller": -1,
        "Num Modifiers": 0,
        "Modifiers": [],
    })

    emitter = nif.add_block(
        _emitter_type(system.emission_shape),
        _emitter_fields(system, particle_system.block_id, emitter_object_block_id),
    )
    particle_system.set_field(
        "Controller",
        _add_controller_chain(nif, system, particle_system.block_id, str(emitter.get_field("Name") or "")),
    )
    modifiers = [
        emitter,
        nif.add_block("NiPSysAgeDeathModifier", _modifier_fields("Age Death", particle_system.block_id)),
        nif.add_block("NiPSysPositionModifier", _modifier_fields("Position", particle_system.block_id)),
        nif.add_block("NiPSysBoundUpdateModifier", _modifier_fields("Bounds", particle_system.block_id)),
    ]
    for modifier in system.modifiers:
        if modifier.enabled:
            modifiers.append(_add_explicit_modifier(nif, modifier, particle_system.block_id, emitter_object_block_id, system))

    particle_system.set_field("Modifiers", [modifier.block_id for modifier in modifiers])
    particle_system.set_field("Num Modifiers", len(modifiers))

    raw_overrides = system.raw_overrides
    _apply_overrides(particle_system, raw_overrides.get("system"))
    _apply_overrides(data, raw_overrides.get("data"))
    _apply_overrides(shader, raw_overrides.get("shader"))
    _apply_overrides(alpha, raw_overrides.get("alpha"))
    _apply_overrides(emitter, raw_overrides.get("emitter"))

    _attach_to_node(nif, particle_system.block_id, attach_to_block_id)
    return particle_system.block_id


def _data_fields(system: ParticleSystemDraft) -> dict[str, Any]:
    max_vertices = max(64, math.ceil(system.emission_rate * system.lifetime))
    offset = _subtexture_offset(system)
    return {
        "BS Max Vertices": max_vertices,
        "Has Texture Indices": 1,
        "Num Subtexture Offsets": 1,
        "Subtexture Offsets": [offset],
    }


def _subtexture_offset(system: ParticleSystemDraft) -> tuple[float, float, float, float]:
    cell_count = system.atlas_rows * system.atlas_columns
    index = min(max(int(system.subtexture_index), 0), cell_count - 1)
    column = index % system.atlas_columns
    row = index // system.atlas_columns
    width = 1.0 / system.atlas_columns
    height = 1.0 / system.atlas_rows
    return (column * width, width, row * height, height)


def _shader_fields(system: ParticleSystemDraft) -> dict[str, Any]:
    red, green, blue, _color_alpha = system.color_rgba
    return {
        "Source Texture": system.texture_path or "",
        "Base Color": _color_dict(red, green, blue, system.alpha),
    }


def _alpha_fields() -> dict[str, Any]:
    return {
        "Flags": 4845,
        "Threshold": 0,
    }


def _emitter_fields(
    system: ParticleSystemDraft,
    system_block_id: int,
    emitter_object_block_id: int,
) -> dict[str, Any]:
    spread_radians = math.radians(system.spread_degrees)
    fields = _modifier_fields("Emitter", system_block_id)
    fields.update({
        "Speed": system.speed,
        "Speed Variation": _float_override(system, "speed_variation", 0.0),
        "Declination": 0.0,
        "Declination Variation": spread_radians,
        "Planar Angle": 0.0,
        "Planar Angle Variation": spread_radians,
        "Life Span": system.lifetime,
        "Life Span Variation": _float_override(system, "lifetime_variation", 0.0),
        "Initial Radius": system.particle_size,
        "Radius Variation": _float_override(system, "radius_variation", 0.0),
        "Initial Color": _initial_color(system),
        "Emitter Object": emitter_object_block_id,
    })
    if system.emission_shape in {EmissionShape.BOX, EmissionShape.POINT}:
        fields.update({
            "Width": system.particle_size,
            "Height": system.particle_size,
            "Depth": system.particle_size,
        })
    elif system.emission_shape in {EmissionShape.CYLINDER, EmissionShape.DISC, EmissionShape.CONE}:
        fields.update({
            "Radius": system.particle_size,
            "Height": system.particle_size,
        })
    else:
        fields["Radius"] = system.particle_size
    return fields


def _initial_color(system: ParticleSystemDraft) -> dict[str, float]:
    red, green, blue, _color_alpha = system.color_rgba
    return _color_dict(red, green, blue, system.alpha)


def _emitter_type(shape: EmissionShape) -> str:
    if shape is EmissionShape.SPHERE:
        return "NiPSysSphereEmitter"
    if shape in {EmissionShape.CYLINDER, EmissionShape.DISC, EmissionShape.CONE}:
        return "NiPSysCylinderEmitter"
    return "NiPSysBoxEmitter"


def _modifier_type(kind: ModifierKind) -> str:
    return {
        ModifierKind.GRAVITY: "NiPSysGravityModifier",
        ModifierKind.DRAG: "NiPSysDragModifier",
        ModifierKind.WIND: "BSWindModifier",
        ModifierKind.ROTATION: "NiPSysRotationModifier",
        ModifierKind.SIZE_OVER_LIFE: "NiPSysGrowFadeModifier",
        ModifierKind.ALPHA_OVER_LIFE: "NiPSysGrowFadeModifier",
        ModifierKind.COLOR_OVER_LIFE: "NiPSysColorModifier",
        ModifierKind.COLLISION: "NiPSysColliderManager",
        ModifierKind.SPAWN_RATE: "NiPSysSpawnModifier",
    }[kind]


def _modifier_fields(name: str, system_block_id: int) -> dict[str, Any]:
    return {
        "Name": name,
        "Target": system_block_id,
        "Active": 1,
    }


def _add_controller_chain(nif: Any, system: ParticleSystemDraft, system_block_id: int, emitter_name: str) -> int:
    stop_time = max(0.01, float(system.lifetime))
    bool_data = nif.add_block("NiBoolData", {
        "Data": {
            "Num Keys": 2,
            "Interpolation": 5,
            "Keys": [
                {"Time": 0.0, "Value": 1},
                {"Time": stop_time, "Value": 1},
            ],
        },
    })
    visibility = nif.add_block("NiBoolInterpolator", {
        "Value": 1,
        "Data": bool_data.block_id,
    })
    birth_rate = nif.add_block("NiFloatInterpolator", {
        "Value": system.emission_rate,
        "Data": -1,
    })
    update = nif.add_block("NiPSysUpdateCtlr", {
        "Next Controller": -1,
        "Flags": 76,
        "Frequency": 1.0,
        "Phase": 0.0,
        "Start Time": 0.0,
        "Stop Time": 0.0,
        "Target": system_block_id,
    })
    emitter_controller = nif.add_block("NiPSysEmitterCtlr", {
        "Next Controller": update.block_id,
        "Flags": 72,
        "Frequency": 1.0,
        "Phase": 0.0,
        "Start Time": 0.0,
        "Stop Time": stop_time,
        "Target": system_block_id,
        "Interpolator": birth_rate.block_id,
        "Modifier Name": emitter_name,
        "Visibility Interpolator": visibility.block_id,
    })
    return emitter_controller.block_id


def _add_explicit_modifier(
    nif: Any,
    modifier: ParticleModifierDraft,
    system_block_id: int,
    emitter_object_block_id: int,
    system: ParticleSystemDraft,
) -> Any:
    if modifier.kind is ModifierKind.SIZE_OVER_LIFE:
        return nif.add_block("BSPSysScaleModifier", _scale_modifier_fields(modifier, system_block_id))
    if modifier.kind is ModifierKind.COLOR_OVER_LIFE:
        return nif.add_block("BSPSysSimpleColorModifier", _color_modifier_fields(modifier, system_block_id, system))
    if modifier.kind is ModifierKind.ALPHA_OVER_LIFE:
        return nif.add_block("BSPSysSimpleColorModifier", _alpha_modifier_fields(modifier, system_block_id, system))
    if modifier.kind is ModifierKind.COLLISION:
        manager = nif.add_block("NiPSysColliderManager", _modifier_fields(modifier.display_name, system_block_id))
        collider = nif.add_block("NiPSysSphericalCollider", {
            "Bounce": float(modifier.settings.get("bounce", 0.25)),
            "Spawn on Collide": 0,
            "Die on Collide": 0,
            "Spawn Modifier": -1,
            "Parent": manager.block_id,
            "Next Collider": -1,
            "Collider Object": emitter_object_block_id,
            "Radius": float(modifier.settings.get("radius", system.particle_size)),
        })
        manager.set_field("Collider", collider.block_id)
        return manager
    return nif.add_block(_modifier_type(modifier.kind), _explicit_modifier_fields(modifier, system_block_id, system))


def _add_spawn_modifier_to_particle_system(nif: Any, model: ParticleSystemModel, system_block: Any) -> int:
    system = draft_from_particle_model(model).systems[0]
    modifier = MODIFIER_CATALOG[ModifierKind.SPAWN_RATE].create_draft()
    age_death = _find_or_create_age_death_modifier(nif, model, system_block)
    existing_spawn_ref = _positive_ref(age_death.get_field("Spawn Modifier"))
    if existing_spawn_ref >= 0:
        return existing_spawn_ref

    spawn_block = nif.add_block(
        "NiPSysSpawnModifier",
        _explicit_modifier_fields(modifier, model.system_block_id, system),
    )
    age_death.set_field("Spawn Modifier", spawn_block.block_id)
    return spawn_block.block_id


def _find_or_create_age_death_modifier(nif: Any, model: ParticleSystemModel, system_block: Any) -> Any:
    for block_id in model.modifier_block_ids:
        block = nif.get_block(block_id)
        if block is not None and block.type_name == "NiPSysAgeDeathModifier":
            return block

    age_death = nif.add_block(
        "NiPSysAgeDeathModifier",
        _modifier_fields("Age Death", model.system_block_id),
    )
    _append_particle_modifier(system_block, age_death.block_id)
    return age_death


def _append_particle_modifier(system_block: Any, modifier_block_id: int) -> None:
    modifiers = list(system_block.get_field("Modifiers") or [])
    if modifier_block_id not in modifiers:
        modifiers.append(modifier_block_id)
        system_block.set_field("Modifiers", modifiers)
    if system_block.get_field("Num Modifiers") is not None:
        system_block.set_field("Num Modifiers", len(modifiers))


def _detach_direct_particle_modifier(system_block: Any, modifier_block_id: int) -> None:
    modifiers = [
        block_id
        for block_id in list(system_block.get_field("Modifiers") or [])
        if int(block_id) != int(modifier_block_id)
    ]
    system_block.set_field("Modifiers", modifiers)
    if system_block.get_field("Num Modifiers") is not None:
        system_block.set_field("Num Modifiers", len(modifiers))


def _clear_child_modifier_ref(parent_block: Any, modifier_block_id: int) -> bool:
    for field_name, value in list(getattr(parent_block, "fields", []) or []):
        if field_name == "Target":
            continue
        if _positive_ref(value) == modifier_block_id:
            parent_block.set_field(field_name, -1)
            return True
    return False


def _explicit_modifier_fields(
    modifier: ParticleModifierDraft,
    system_block_id: int,
    system: ParticleSystemDraft,
) -> dict[str, Any]:
    fields = _modifier_fields(modifier.display_name, system_block_id)
    settings = modifier.settings
    if modifier.kind is ModifierKind.GRAVITY:
        fields["Strength"] = settings.get("strength", 1.0)
        fields["Gravity Axis"] = settings.get("direction", (0.0, 0.0, -1.0))
        fields["Gravity Object"] = _positive_ref(settings.get("object", -1))
    elif modifier.kind is ModifierKind.DRAG:
        fields["Percentage"] = settings.get("amount", 0.2)
        fields["Drag Object"] = _positive_ref(settings.get("object", -1))
    elif modifier.kind is ModifierKind.WIND:
        fields["Strength"] = settings.get("strength", 0.5)
    elif modifier.kind is ModifierKind.ROTATION:
        fields["Rotation Speed"] = math.radians(float(settings.get("degrees_per_second", 90.0)))
    elif modifier.kind is ModifierKind.SPAWN_RATE:
        fields["Num Spawn Generations"] = int(settings.get("generations", 0))
        fields["Percentage Spawned"] = float(settings.get("percentage_spawned", 1.0))
        fields["Min Num to Spawn"] = int(settings.get("min_spawn", 1))
        fields["Max Num to Spawn"] = int(settings.get("max_spawn", 1))
        fields["Spawn Speed Variation"] = float(settings.get("speed_variation", 0.0))
        fields["Spawn Dir Variation"] = math.radians(float(settings.get("direction_variation_degrees", 0.0)))
        fields["Life Span"] = system.lifetime
        fields["Life Span Variation"] = 0.0
    return fields


def _scale_modifier_fields(modifier: ParticleModifierDraft, system_block_id: int) -> dict[str, Any]:
    curve = _float_sequence(modifier.settings.get("curve", (1.0, 1.0)))
    fields = _modifier_fields(modifier.display_name, system_block_id)
    fields.update({
        "Num Scales": len(curve),
        "Scales": list(curve),
    })
    return fields


def _color_modifier_fields(
    modifier: ParticleModifierDraft,
    system_block_id: int,
    system: ParticleSystemDraft,
) -> dict[str, Any]:
    start = _color_tuple(modifier.settings.get("start_color", system.color_rgba))
    end = _color_tuple(modifier.settings.get("end_color", (system.color_rgba[0], system.color_rgba[1], system.color_rgba[2], 0.0)))
    mid = (
        (start[0] + end[0]) * 0.5,
        (start[1] + end[1]) * 0.5,
        (start[2] + end[2]) * 0.5,
        (start[3] + end[3]) * 0.5,
    )
    return _simple_color_fields(modifier.display_name, system_block_id, (start, mid, end))


def _alpha_modifier_fields(
    modifier: ParticleModifierDraft,
    system_block_id: int,
    system: ParticleSystemDraft,
) -> dict[str, Any]:
    curve = _float_sequence(modifier.settings.get("curve", (0.0, system.alpha, 0.0)))
    if len(curve) == 1:
        curve = (curve[0], curve[0], curve[0])
    elif len(curve) == 2:
        curve = (curve[0], curve[1], curve[1])
    base = _color_tuple(system.color_rgba)
    colors = tuple(
        (base[0], base[1], base[2], max(0.0, min(1.0, value)))
        for value in curve[:3]
    )
    return _simple_color_fields(modifier.display_name, system_block_id, colors)


def _simple_color_fields(
    name: str,
    system_block_id: int,
    colors: tuple[tuple[float, float, float, float], ...],
) -> dict[str, Any]:
    first, second, third = colors
    fields = _modifier_fields(name, system_block_id)
    fields.update({
        "Fade In Percent": 0.0,
        "Fade Out Percent": 1.0,
        "Color 1 End Percent": 0.0,
        "Color 1 Start Percent": 0.5,
        "Color 2 End Percent": 0.5,
        "Color 2 Start Percent": 1.0,
        "Colors": [_color_dict(*first), _color_dict(*second), _color_dict(*third)],
    })
    return fields


def _float_override(system: ParticleSystemDraft, name: str, default: float) -> float:
    emitter_overrides = system.raw_overrides.get("emitter")
    if isinstance(emitter_overrides, Mapping) and name in emitter_overrides:
        return float(emitter_overrides[name])
    return default


def _emitter_object_block_id(nif: Any, attach_to_block_id: int | None) -> int:
    if attach_to_block_id is None:
        return -1
    parent = nif.get_block(attach_to_block_id)
    if parent is None or not _is_node(nif, parent):
        return -1
    return int(attach_to_block_id)


def _positive_ref(value: Any) -> int:
    try:
        block_id = int(value)
    except (TypeError, ValueError):
        return -1
    return block_id if block_id >= 0 else -1


def _float_sequence(value: Any) -> tuple[float, ...]:
    if isinstance(value, (str, bytes)):
        return (float(value),)
    try:
        values = tuple(float(item) for item in value)
    except TypeError:
        values = (float(value),)
    return values or (1.0,)


def _color_tuple(value: Any) -> tuple[float, float, float, float]:
    if isinstance(value, Mapping):
        return (
            float(value.get("r", value.get("R", value.get("x", value.get("X", 1.0))))),
            float(value.get("g", value.get("G", value.get("y", value.get("Y", 1.0))))),
            float(value.get("b", value.get("B", value.get("z", value.get("Z", 1.0))))),
            float(value.get("a", value.get("A", value.get("w", value.get("W", 1.0))))),
        )
    red, green, blue, alpha = value
    return (float(red), float(green), float(blue), float(alpha))


def _color_dict(red: float, green: float, blue: float, alpha: float) -> dict[str, float]:
    return {
        "r": float(red),
        "g": float(green),
        "b": float(blue),
        "a": float(alpha),
    }


def _apply_overrides(block: Any, overrides: Any) -> None:
    if not isinstance(overrides, Mapping):
        return
    for name, value in overrides.items():
        block.set_field(str(name), value)


def _attach_to_node(nif: Any, system_block_id: int, attach_to_block_id: int | None) -> None:
    if attach_to_block_id is None:
        return
    parent = nif.get_block(attach_to_block_id)
    if parent is None or not _is_node(nif, parent):
        return
    children = list(parent.get_field("Children") or [])
    children.append(system_block_id)
    parent.set_field("Children", children)
    if parent.get_field("Num Children") is not None:
        parent.set_field("Num Children", len(children))


def _is_node(nif: Any, block: Any) -> bool:
    schema = getattr(nif, "schema", None)
    if schema is not None:
        return bool(schema.is_subtype_of(block.type_name, "NiNode"))
    return block.type_name in {"NiNode", "BSFadeNode"}
