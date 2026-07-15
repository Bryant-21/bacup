from __future__ import annotations

import math

from .builder_model import EmissionShape, ModifierKind, ParticleEffectDraft, ParticleSystemDraft
from .model import ParticleSupportLevel, ParticleSystemModel, ParticleWarning
from .runtime import ParticleRuntime


_SYNTHETIC_BLOCK_BASE = 1_000_000
_SYNTHETIC_BLOCK_STRIDE = 100

_MODIFIER_TYPES = {
    ModifierKind.GRAVITY: "NiPSysGravityModifier",
    ModifierKind.DRAG: "NiPSysDragModifier",
    ModifierKind.WIND: "BSWindModifier",
    ModifierKind.ROTATION: "NiPSysRotationModifier",
    ModifierKind.SIZE_OVER_LIFE: "BSPSysScaleModifier",
    ModifierKind.COLOR_OVER_LIFE: "BSPSysSimpleColorModifier",
    ModifierKind.ALPHA_OVER_LIFE: "BSPSysSimpleColorModifier",
    ModifierKind.COLLISION: "NiPSysColliderManager",
    ModifierKind.SPAWN_RATE: "NiPSysSpawnModifier",
}


def build_preview_models_for_draft(
    draft: ParticleEffectDraft,
    nif_id: str = "draft",
) -> list[ParticleSystemModel]:
    return [
        _build_preview_model(system, system_index, nif_id)
        for system_index, system in enumerate(draft.systems)
    ]


def build_preview_runtime_for_draft(
    draft: ParticleEffectDraft,
    seed: int = 1729,
) -> ParticleRuntime:
    return ParticleRuntime(build_preview_models_for_draft(draft), seed=seed)


def _build_preview_model(
    system: ParticleSystemDraft,
    system_index: int,
    nif_id: str,
) -> ParticleSystemModel:
    base_block_id = _SYNTHETIC_BLOCK_BASE + system_index * _SYNTHETIC_BLOCK_STRIDE
    emitter_type = _emitter_type(system.emission_shape)
    modifier_block_ids = _modifier_block_ids(system, base_block_id)
    modifier_types = _modifier_types(system, emitter_type)

    return ParticleSystemModel(
        nif_id=nif_id,
        system_block_id=base_block_id,
        name=system.display_name,
        data_block_id=base_block_id + 1,
        shader_property_block_id=base_block_id + 2,
        alpha_property_block_id=base_block_id + 3,
        controller_block_id=base_block_id + 4,
        emitter_block_id=base_block_id + 5,
        emitter_type=emitter_type,
        modifier_block_ids=modifier_block_ids,
        modifier_types=modifier_types,
        world_space=True,
        max_particles=_max_particles(system),
        atlas_offsets=(_atlas_offset(system),),
        source_texture=system.texture_path,
        base_color=_base_color(system),
        emitter_initial_color=_base_color(system),
        emitter_speed=system.speed,
        emitter_declination=0.0,
        emitter_declination_variation=math.radians(system.spread_degrees),
        emitter_planar_angle=0.0,
        emitter_planar_angle_variation=math.radians(system.spread_degrees),
        emitter_lifetime=system.lifetime,
        emitter_initial_radius=system.particle_size,
        emitter_radius=system.particle_size,
        support_level=ParticleSupportLevel.SUPPORTED,
        warnings=_warnings(system, base_block_id),
    )


def _emitter_type(shape: EmissionShape) -> str:
    if shape is EmissionShape.SPHERE:
        return "NiPSysSphereEmitter"
    if shape in {EmissionShape.CYLINDER, EmissionShape.DISC, EmissionShape.CONE}:
        return "NiPSysCylinderEmitter"
    return "NiPSysBoxEmitter"


def _modifier_block_ids(system: ParticleSystemDraft, base_block_id: int) -> tuple[int, ...]:
    return (base_block_id + 5, *(
        base_block_id + 10 + modifier_index
        for modifier_index, modifier in enumerate(system.modifiers)
        if modifier.enabled
    ))


def _modifier_types(system: ParticleSystemDraft, emitter_type: str) -> tuple[str, ...]:
    return (emitter_type, *(
        _MODIFIER_TYPES[modifier.kind]
        for modifier in system.modifiers
        if modifier.enabled
    ))


def _warnings(system: ParticleSystemDraft, base_block_id: int) -> tuple[ParticleWarning, ...]:
    return tuple(
        ParticleWarning(
            block_id=base_block_id + 10 + modifier_index,
            message=f"{modifier.display_name} preview is approximate; saved modifier behavior is not simulated.",
        )
        for modifier_index, modifier in enumerate(system.modifiers)
        if modifier.enabled
    )


def _max_particles(system: ParticleSystemDraft) -> int:
    return max(64, math.ceil(system.emission_rate * max(system.lifetime, 0.01)))


def _atlas_offset(system: ParticleSystemDraft) -> tuple[float, float, float, float]:
    rows = max(1, int(system.atlas_rows))
    columns = max(1, int(system.atlas_columns))
    cell_count = rows * columns
    index = min(max(int(system.subtexture_index), 0), cell_count - 1)
    column = index % columns
    row = index // columns
    width = 1.0 / columns
    height = 1.0 / rows
    u_min = column * width
    v_min = row * height
    return (u_min, u_min + width, v_min, v_min + height)


def _base_color(system: ParticleSystemDraft) -> tuple[float, float, float, float]:
    red, green, blue, _color_alpha = system.color_rgba
    return (red, green, blue, system.alpha)
