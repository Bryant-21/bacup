"""Particle effect viewing support for the NIF editor."""

from .builder_model import (
    BlendPreset,
    EmissionShape,
    ModifierKind,
    ParticleEffectDraft,
    ParticleModifierDraft,
    ParticleSystemDraft,
    ParticleSystemKind,
)
from .catalog import (
    BLEND_PRESETS,
    EMISSION_SHAPES,
    MODIFIER_CATALOG,
    SYSTEM_KIND_CATALOG,
    FriendlyCatalogEntry,
    ModifierCatalogEntry,
    SystemKindCatalogEntry,
    build_preset,
    get_blend_preset_entry,
    get_emission_shape_entry,
    get_modifier_catalog_entry,
    get_system_kind_entry,
    preset_keys,
)
from .authoring import (
    AuthoringResult,
    DraftValidationIssue,
    apply_draft_to_session,
    apply_draft_to_nif,
    draft_from_particle_model,
    validate_draft,
)
from .model import (
    ParticleSystemModel,
    ParticleSupportLevel,
    ParticleWarning,
    build_particle_models,
    owner_system_for_block,
)
from .preview import (
    build_preview_models_for_draft,
    build_preview_runtime_for_draft,
)
from .runtime import (
    PARTICLE_PREVIEW_SEQUENCE,
    ParticleDrawBatch,
    ParticlePreviewOverrides,
    ParticleRuntime,
)

__all__ = [
    "AuthoringResult",
    "BlendPreset",
    "DraftValidationIssue",
    "EmissionShape",
    "ModifierKind",
    "ParticleEffectDraft",
    "ParticleModifierDraft",
    "ParticleSystemDraft",
    "ParticleSystemKind",
    "ParticleSystemModel",
    "ParticleSupportLevel",
    "ParticleWarning",
    "apply_draft_to_session",
    "apply_draft_to_nif",
    "build_particle_models",
    "build_preview_models_for_draft",
    "build_preview_runtime_for_draft",
    "draft_from_particle_model",
    "owner_system_for_block",
    "validate_draft",
]

__all__ += [
    "BLEND_PRESETS",
    "EMISSION_SHAPES",
    "MODIFIER_CATALOG",
    "SYSTEM_KIND_CATALOG",
    "FriendlyCatalogEntry",
    "ModifierCatalogEntry",
    "SystemKindCatalogEntry",
    "build_preset",
    "get_blend_preset_entry",
    "get_emission_shape_entry",
    "get_modifier_catalog_entry",
    "get_system_kind_entry",
    "preset_keys",
]

__all__ += [
    "PARTICLE_PREVIEW_SEQUENCE",
    "ParticleDrawBatch",
    "ParticlePreviewOverrides",
    "ParticleRuntime",
]
