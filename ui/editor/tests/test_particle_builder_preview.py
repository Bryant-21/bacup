import math

import numpy as np

from ui.editor.particles.builder_model import (
    EmissionShape,
    ModifierKind,
    ParticleEffectDraft,
    ParticleModifierDraft,
    ParticleSystemDraft,
)
from ui.editor.particles.catalog import build_preset
from ui.editor.particles.model import ParticleSupportLevel, ParticleSystemModel
from ui.editor.particles.preview import build_preview_models_for_draft, build_preview_runtime_for_draft
from ui.editor.particles.runtime import ParticlePreviewOverrides, ParticleRuntime


def test_smoke_preset_draft_builds_supported_preview_model():
    system = build_preset("smoke_puff").with_updates(
        atlas_rows=2,
        atlas_columns=4,
        subtexture_index=6,
    )
    draft = ParticleEffectDraft(effect_name="Smoke", systems=[system])

    [model] = build_preview_models_for_draft(draft, nif_id="draft-nif")

    assert isinstance(model, ParticleSystemModel)
    assert model.nif_id == "draft-nif"
    assert model.system_block_id == 1_000_000
    assert model.data_block_id == 1_000_001
    assert model.shader_property_block_id == 1_000_002
    assert model.alpha_property_block_id == 1_000_003
    assert model.controller_block_id == 1_000_004
    assert model.emitter_block_id == 1_000_005
    assert model.name == "Smoke Puff"
    assert model.source_texture == r"textures\effects\smoke.dds"
    assert model.base_color == (0.55, 0.55, 0.55, 0.75)
    assert model.max_particles == 64
    assert model.emitter_type == "NiPSysSphereEmitter"
    assert model.emitter_speed == 0.35
    assert model.emitter_lifetime == 2.5
    assert model.emitter_initial_radius == 1.4
    assert model.emitter_radius == 1.4
    assert model.emitter_declination_variation == math.radians(35.0)
    assert model.emitter_planar_angle_variation == math.radians(35.0)
    assert model.atlas_offsets == ((0.5, 0.75, 0.5, 1.0),)
    assert model.support_level is ParticleSupportLevel.SUPPORTED
    assert model.modifier_types == (
        "NiPSysSphereEmitter",
        "NiPSysDragModifier",
        "BSPSysScaleModifier",
        "BSPSysSimpleColorModifier",
    )
    assert "Drag" in model.warning_text
    assert "Size Over Life" in model.warning_text
    assert "Alpha Over Life" in model.warning_text
    assert "preview is approximate" in model.warning_text


def test_emission_shapes_map_to_authoring_emitter_types():
    expected = {
        EmissionShape.POINT: "NiPSysBoxEmitter",
        EmissionShape.BOX: "NiPSysBoxEmitter",
        EmissionShape.SPHERE: "NiPSysSphereEmitter",
        EmissionShape.CYLINDER: "NiPSysCylinderEmitter",
        EmissionShape.DISC: "NiPSysCylinderEmitter",
        EmissionShape.CONE: "NiPSysCylinderEmitter",
    }

    draft = ParticleEffectDraft(
        systems=[
            ParticleSystemDraft(display_name=shape.value, emission_shape=shape)
            for shape in EmissionShape
        ],
    )

    assert [model.emitter_type for model in build_preview_models_for_draft(draft)] == [
        expected[shape]
        for shape in EmissionShape
    ]


def test_runtime_can_draw_batches_for_preview_models():
    draft = ParticleEffectDraft(
        systems=[
            ParticleSystemDraft(
                display_name="Spark",
                emission_shape=EmissionShape.POINT,
                lifetime=1.0,
                emission_rate=20.0,
                speed=2.0,
                particle_size=0.25,
            ),
        ],
    )
    runtime = ParticleRuntime(build_preview_models_for_draft(draft), seed=22)
    runtime.set_overrides(1_000_000, ParticlePreviewOverrides(spawn_rate=1000.0))

    runtime.play()
    runtime.update(0.02)

    [batch] = runtime.build_draw_batches()
    assert batch.system_block_id == 1_000_000
    assert batch.positions.shape[0] > 0
    assert batch.positions.shape[1] == 3
    assert np.allclose(batch.atlas_offsets, np.array([[0.0, 1.0, 0.0, 1.0]], dtype=np.float32))


def test_disabled_modifiers_are_excluded_from_types_and_warnings():
    draft = ParticleEffectDraft(
        systems=[
            ParticleSystemDraft(
                modifiers=(
                    ParticleModifierDraft(ModifierKind.GRAVITY, "Gravity", enabled=False),
                    ParticleModifierDraft(ModifierKind.WIND, "Wind"),
                ),
            ),
        ],
    )

    [model] = build_preview_models_for_draft(draft)

    assert "NiPSysGravityModifier" not in model.modifier_types
    assert "Gravity" not in model.warning_text
    assert "BSWindModifier" in model.modifier_types
    assert "Wind" in model.warning_text


def test_preview_runtime_helper_builds_runtime():
    draft = ParticleEffectDraft(systems=[ParticleSystemDraft()])

    runtime = build_preview_runtime_for_draft(draft, seed=91)

    assert isinstance(runtime, ParticleRuntime)
    assert runtime.seed == 91
    assert runtime.models[0].system_block_id == 1_000_000


def test_package_exports_preview_helper():
    from ui.editor.particles import build_preview_models_for_draft

    assert callable(build_preview_models_for_draft)
