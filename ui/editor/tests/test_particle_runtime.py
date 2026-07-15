from dataclasses import replace

import numpy as np

from ui.editor.particles.model import ParticleSupportLevel, ParticleSystemModel
from ui.editor.particles.runtime import (
    PARTICLE_PREVIEW_SEQUENCE,
    ParticlePreviewOverrides,
    ParticleRuntime,
)


def _model(max_particles=8):
    return ParticleSystemModel(
        nif_id="main",
        system_block_id=10,
        name="Smoke",
        data_block_id=11,
        shader_property_block_id=-1,
        alpha_property_block_id=-1,
        controller_block_id=-1,
        emitter_block_id=12,
        emitter_type="NiPSysBoxEmitter",
        modifier_block_ids=(12,),
        modifier_types=("NiPSysBoxEmitter",),
        max_particles=max_particles,
        atlas_offsets=((0.0, 1.0, 0.0, 1.0),),
        support_level=ParticleSupportLevel.SUPPORTED,
    )


def test_runtime_play_update_stop_lifecycle_is_deterministic():
    runtime = ParticleRuntime([_model()], seed=1234)
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.25)
    first_positions = runtime.snapshot_positions(10)

    runtime.stop()
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.25)
    second_positions = runtime.snapshot_positions(10)

    assert first_positions == second_positions
    assert runtime.is_playing is True

    runtime.stop()
    assert runtime.is_playing is False
    assert runtime.snapshot_positions(10) == []


def test_fresh_runtime_reports_particle_models_available():
    assert ParticleRuntime([_model()], seed=2).has_particles is True
    assert ParticleRuntime([], seed=2).has_particles is False


def test_runtime_pause_freezes_particles():
    runtime = ParticleRuntime([_model()], seed=5)
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.25)
    before = runtime.snapshot_positions(10)
    runtime.pause()
    runtime.update(1.0)

    assert runtime.snapshot_positions(10) == before
    assert runtime.is_paused is True


def test_zero_max_particles_supported_model_does_not_spawn():
    runtime = ParticleRuntime([_model(max_particles=0)], seed=3)
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(1.0)

    assert runtime.snapshot_positions(10) == []
    assert runtime.build_draw_batches() == []


def test_diagnostic_only_and_unsupported_models_do_not_spawn():
    diagnostic_model = replace(
        _model(),
        system_block_id=20,
        support_level=ParticleSupportLevel.DIAGNOSTIC_ONLY,
    )
    unsupported_model = replace(
        _model(),
        system_block_id=30,
        support_level=ParticleSupportLevel.UNSUPPORTED,
    )
    runtime = ParticleRuntime([diagnostic_model, unsupported_model], seed=4)
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(1.0)

    assert runtime.snapshot_positions(20) == []
    assert runtime.snapshot_positions(30) == []
    assert runtime.build_draw_batches() == []


def test_preview_overrides_do_not_mutate_models():
    model = _model()
    runtime = ParticleRuntime([model], seed=7)
    runtime.set_overrides(model.system_block_id, ParticlePreviewOverrides(spawn_rate=2.0, speed=3.0))

    assert runtime.models[0] is model
    assert runtime.overrides[model.system_block_id].spawn_rate == 2.0
    assert model.max_particles == 8


def test_runtime_models_are_immutable_snapshot():
    models = [_model()]
    runtime = ParticleRuntime(models, seed=8)
    models.clear()

    assert runtime.models[0].system_block_id == 10
    assert runtime.has_particles is True


def test_rebuilt_runtime_restores_playback_state_from_previous_runtime():
    old_runtime = ParticleRuntime([_model(max_particles=16)], seed=23)
    old_runtime.set_overrides(10, ParticlePreviewOverrides(spawn_rate=1000.0))
    old_runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    old_runtime.update(0.10)
    edited_model = replace(
        _model(max_particles=16),
        emitter_speed=9.0,
        emitter_speed_variation=0.0,
    )
    new_runtime = ParticleRuntime([edited_model], seed=23)

    new_runtime.restore_playback_from(old_runtime)

    assert new_runtime.is_playing is True
    assert new_runtime.current_time == old_runtime.current_time
    assert new_runtime.overrides == old_runtime.overrides
    assert new_runtime.models[0].emitter_speed == 9.0
    [batch] = new_runtime.build_draw_batches()
    assert np.allclose(np.linalg.norm(batch.velocities, axis=1), 9.0)


def test_set_time_rebuilds_from_zero_deterministically():
    runtime = ParticleRuntime([_model(max_particles=16)], seed=9)
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.50)
    update_positions = runtime.snapshot_positions(10)

    runtime.set_time(0.50)
    seek_positions = runtime.snapshot_positions(10)

    assert seek_positions == update_positions


def test_set_time_matches_multi_step_playback():
    playback_runtime = ParticleRuntime([_model(max_particles=16)], seed=11)
    playback_runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    playback_runtime.update(0.25)
    playback_runtime.update(0.25)

    seek_runtime = ParticleRuntime([_model(max_particles=16)], seed=11)
    seek_runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    seek_runtime.set_time(0.50)

    assert seek_runtime.snapshot_positions(10) == playback_runtime.snapshot_positions(10)


def test_substep_remainder_is_accumulated_across_updates():
    single_update_runtime = ParticleRuntime([_model(max_particles=64)], seed=12)
    single_update_runtime.set_overrides(10, ParticlePreviewOverrides(spawn_rate=1000.0))
    single_update_runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    single_update_runtime.update(0.02)

    split_update_runtime = ParticleRuntime([_model(max_particles=64)], seed=12)
    split_update_runtime.set_overrides(10, ParticlePreviewOverrides(spawn_rate=1000.0))
    split_update_runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    split_update_runtime.update(0.01)
    split_update_runtime.update(0.01)

    assert split_update_runtime.snapshot_positions(10) == single_update_runtime.snapshot_positions(10)


def test_draw_data_contains_camera_independent_particle_fields():
    runtime = ParticleRuntime([_model(max_particles=4)], seed=1)
    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.20)

    draw_batches = runtime.build_draw_batches()

    assert len(draw_batches) == 1
    batch = draw_batches[0]
    assert batch.nif_id == "main"
    assert batch.system_block_id == 10
    assert batch.shader_property_block_id == -1
    assert batch.alpha_property_block_id == -1
    assert batch.positions.shape[1] == 3
    assert batch.colors.shape[1] == 4
    assert batch.sizes.ndim == 1
    assert batch.atlas_indices.ndim == 1


def test_runtime_uses_model_emitter_and_material_fields():
    model = replace(
        _model(max_particles=8),
        emitter_type="NiPSysSphereEmitter",
        base_color=(0.2, 0.4, 0.8, 0.5),
        emitter_speed=6.0,
        emitter_speed_variation=0.0,
        emitter_lifetime=1.25,
        emitter_lifetime_variation=0.0,
        emitter_initial_radius=2.0,
        emitter_radius_variation=0.0,
        emitter_radius=10.0,
    )
    runtime = ParticleRuntime([model], seed=13)
    runtime.set_overrides(model.system_block_id, ParticlePreviewOverrides(spawn_rate=1000.0))

    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.02)

    [batch] = runtime.build_draw_batches()
    assert np.allclose(np.linalg.norm(batch.velocities, axis=1), 6.0)
    assert np.allclose(batch.colors, np.array([[0.2, 0.4, 0.8, 0.5]], dtype=np.float32))
    assert np.allclose(batch.sizes, 2.0)
    assert np.max(np.linalg.norm(batch.positions, axis=1)) > 1.0
    assert np.allclose(runtime._states[model.system_block_id].lifetimes, 1.25)


def test_runtime_uses_model_emitter_direction_angles():
    model = replace(
        _model(max_particles=4),
        emitter_speed=4.0,
        emitter_speed_variation=0.0,
        emitter_declination=np.pi / 2.0,
        emitter_declination_variation=0.0,
        emitter_planar_angle=0.0,
        emitter_planar_angle_variation=0.0,
    )
    runtime = ParticleRuntime([model], seed=19)
    runtime.set_overrides(model.system_block_id, ParticlePreviewOverrides(spawn_rate=1000.0))

    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.02)

    [batch] = runtime.build_draw_batches()
    expected = np.array([[4.0, 0.0, 0.0]], dtype=np.float32)
    assert np.allclose(batch.velocities, expected, atol=1e-6)


def test_runtime_carries_particle_texture_into_draw_batch():
    texture = object()
    greyscale_texture = object()
    model = replace(_model(max_particles=4), greyscale_color=True, greyscale_alpha=True)
    runtime = ParticleRuntime(
        [model],
        texture_by_system={model.system_block_id: texture},
        greyscale_texture_by_system={model.system_block_id: greyscale_texture},
        seed=17,
    )
    runtime.set_overrides(model.system_block_id, ParticlePreviewOverrides(spawn_rate=1000.0))

    runtime.play(PARTICLE_PREVIEW_SEQUENCE)
    runtime.update(0.02)

    [batch] = runtime.build_draw_batches()
    assert batch.texture is texture
    assert batch.greyscale_texture is greyscale_texture
    assert batch.greyscale_color is True
    assert batch.greyscale_alpha is True
