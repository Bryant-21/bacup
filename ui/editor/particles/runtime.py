from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import numpy as np

from .model import ParticleSupportLevel, ParticleSystemModel


PARTICLE_PREVIEW_SEQUENCE = "[Particle Preview]"
_MAX_PREVIEW_PARTICLES = 4096
_FIXED_SIMULATION_STEP = 1.0 / 60.0
_SIMULATED_SUPPORT_LEVELS = {
    ParticleSupportLevel.SUPPORTED,
    ParticleSupportLevel.APPROXIMATE,
}


@dataclass(frozen=True)
class ParticlePreviewOverrides:
    spawn_rate: float | None = None
    speed: float | None = None
    lifetime: float | None = None
    color_tint: tuple[float, float, float, float] | None = None
    time_scale: float | None = None


@dataclass(frozen=True)
class ParticleDrawBatch:
    nif_id: str
    system_block_id: int
    shader_property_block_id: int | None
    alpha_property_block_id: int | None
    positions: np.ndarray
    velocities: np.ndarray
    colors: np.ndarray
    sizes: np.ndarray
    rotations: np.ndarray
    atlas_indices: np.ndarray
    atlas_offsets: np.ndarray
    emitter_object_block_id: int | None = None
    texture: Any = None
    greyscale_texture: Any = None
    greyscale_color: bool = False
    greyscale_alpha: bool = False


@dataclass
class _ParticleState:
    positions: np.ndarray = field(default_factory=lambda: np.empty((0, 3), dtype=np.float32))
    velocities: np.ndarray = field(default_factory=lambda: np.empty((0, 3), dtype=np.float32))
    ages: np.ndarray = field(default_factory=lambda: np.empty((0,), dtype=np.float32))
    lifetimes: np.ndarray = field(default_factory=lambda: np.empty((0,), dtype=np.float32))
    colors: np.ndarray = field(default_factory=lambda: np.empty((0, 4), dtype=np.float32))
    sizes: np.ndarray = field(default_factory=lambda: np.empty((0,), dtype=np.float32))
    rotations: np.ndarray = field(default_factory=lambda: np.empty((0,), dtype=np.float32))
    atlas_indices: np.ndarray = field(default_factory=lambda: np.empty((0,), dtype=np.int32))
    spawn_carry: float = 0.0


class ParticleRuntime:
    def __init__(
        self,
        models: list[ParticleSystemModel],
        seed: int = 1729,
        texture_by_system: dict[int, Any] | None = None,
        greyscale_texture_by_system: dict[int, Any] | None = None,
    ):
        self.models = tuple(models)
        self.seed = seed
        self._texture_by_system = dict(texture_by_system or {})
        self._greyscale_texture_by_system = dict(greyscale_texture_by_system or {})
        self.overrides: dict[int, ParticlePreviewOverrides] = {}
        self._states: dict[int, _ParticleState] = {}
        self._rng = np.random.default_rng(seed)
        self._playing = False
        self._paused = False
        self._sequence_name: str | None = None
        self._time = 0.0
        self._simulation_carry = 0.0
        self._warnings: list[str] = []
        self._reset_states()

    @property
    def has_particles(self) -> bool:
        return bool(self.models)

    @property
    def is_playing(self) -> bool:
        return self._playing

    @property
    def is_paused(self) -> bool:
        return self._paused

    @property
    def current_time(self) -> float:
        return self._time

    @property
    def sequence_name(self) -> str:
        return self._sequence_name or PARTICLE_PREVIEW_SEQUENCE

    @property
    def warning_text(self) -> str:
        return "; ".join(self._warnings)

    def set_overrides(self, system_block_id: int, overrides: ParticlePreviewOverrides) -> None:
        self.overrides[system_block_id] = overrides

    def clear_overrides(self, system_block_id: int | None = None) -> None:
        if system_block_id is None:
            self.overrides.clear()
            return
        self.overrides.pop(system_block_id, None)

    def restore_playback_from(self, previous: "ParticleRuntime") -> None:
        self.overrides = dict(getattr(previous, "overrides", {}) or {})
        if not getattr(previous, "is_playing", False):
            return

        self.play(getattr(previous, "sequence_name", PARTICLE_PREVIEW_SEQUENCE))
        previous_time = float(getattr(previous, "current_time", 0.0) or 0.0)
        if previous_time > 0.0:
            self.set_time(previous_time)
        if getattr(previous, "is_paused", False):
            self.pause()

    def play(self, sequence_name: str = PARTICLE_PREVIEW_SEQUENCE) -> None:
        self._sequence_name = None if sequence_name == PARTICLE_PREVIEW_SEQUENCE else sequence_name
        self._playing = True
        self._paused = False
        self._time = 0.0
        self._reset_states()

    def pause(self) -> None:
        if self._playing:
            self._paused = True

    def resume(self) -> None:
        if self._playing:
            self._paused = False

    def stop(self) -> None:
        self._playing = False
        self._paused = False
        self._time = 0.0
        self._reset_states()

    def set_time(self, time_seconds: float) -> None:
        target_time = max(0.0, float(time_seconds))
        was_playing = self._playing
        was_paused = self._paused
        self._playing = True
        self._paused = False
        self._time = 0.0
        self._reset_states()
        self.update(target_time)
        self._playing = was_playing
        self._paused = was_paused
        if not was_playing:
            self._time = target_time

    def update(self, dt: float) -> None:
        if not self._playing or self._paused or dt <= 0:
            return

        step = float(dt)
        self._time += step
        self._simulation_carry += step
        while self._simulation_carry >= _FIXED_SIMULATION_STEP:
            self._update_step(_FIXED_SIMULATION_STEP)
            self._simulation_carry -= _FIXED_SIMULATION_STEP

    def _update_step(self, step: float) -> None:
        for model in self.models:
            if model.support_level not in _SIMULATED_SUPPORT_LEVELS:
                continue
            state = self._states[model.system_block_id]
            scaled_step = step * self._time_scale(model)
            self._advance_particles(state, scaled_step)
            self._spawn_particles(model, state, scaled_step)

    def build_draw_batches(self) -> list[ParticleDrawBatch]:
        batches = []
        for model in self.models:
            state = self._states[model.system_block_id]
            if state.positions.shape[0] == 0:
                continue
            batches.append(ParticleDrawBatch(
                nif_id=model.nif_id,
                system_block_id=model.system_block_id,
                shader_property_block_id=model.shader_property_block_id,
                alpha_property_block_id=model.alpha_property_block_id,
                positions=state.positions.copy(),
                velocities=state.velocities.copy(),
                colors=state.colors.copy(),
                sizes=state.sizes.copy(),
                rotations=state.rotations.copy(),
                atlas_indices=state.atlas_indices.copy(),
                atlas_offsets=self._atlas_offsets_array(model),
                emitter_object_block_id=model.emitter_object_block_id,
                texture=self._texture_by_system.get(model.system_block_id),
                greyscale_texture=self._greyscale_texture_by_system.get(model.system_block_id),
                greyscale_color=model.greyscale_color,
                greyscale_alpha=model.greyscale_alpha,
            ))
        return batches

    def snapshot_positions(self, system_block_id: int) -> list[tuple[float, float, float]]:
        state = self._states.get(system_block_id)
        if state is None:
            return []
        return [tuple(float(round(value, 6)) for value in position) for position in state.positions]

    def _reset_states(self) -> None:
        self._rng = np.random.default_rng(self.seed)
        self._states = {model.system_block_id: _ParticleState() for model in self.models}
        self._simulation_carry = 0.0
        self._warnings = [
            model.warning_text
            for model in self.models
            if model.warning_text
        ]

    def _advance_particles(self, state: _ParticleState, dt: float) -> None:
        if state.positions.shape[0] == 0:
            return

        state.ages = state.ages + dt
        alive = state.ages < state.lifetimes
        state.positions = state.positions[alive] + state.velocities[alive] * dt
        state.velocities = state.velocities[alive]
        state.ages = state.ages[alive]
        state.lifetimes = state.lifetimes[alive]
        state.colors = state.colors[alive]
        state.sizes = state.sizes[alive]
        state.rotations = state.rotations[alive]
        state.atlas_indices = state.atlas_indices[alive]

    def _spawn_particles(self, model: ParticleSystemModel, state: _ParticleState, dt: float) -> None:
        limit = self._particle_limit(model)
        available = limit - state.positions.shape[0]
        if available <= 0:
            return

        state.spawn_carry += self._spawn_rate(model) * dt
        spawn_count = min(int(state.spawn_carry), available)
        if spawn_count <= 0:
            return
        state.spawn_carry -= spawn_count

        positions = self._spawn_positions(model, spawn_count)
        velocities = self._spawn_velocities(model, spawn_count, self._speed(model), self._speed_variation(model))
        lifetimes = self._lifetimes(model, spawn_count)
        ages = np.zeros((spawn_count,), dtype=np.float32)
        colors = np.tile(np.array(self._color_tint(model), dtype=np.float32), (spawn_count, 1))
        sizes = self._sizes(model, spawn_count)
        rotations = self._rng.uniform(0.0, np.pi * 2.0, size=spawn_count).astype(np.float32)
        atlas_indices = self._spawn_atlas_indices(model, spawn_count)

        state.positions = np.concatenate((state.positions, positions))
        state.velocities = np.concatenate((state.velocities, velocities))
        state.ages = np.concatenate((state.ages, ages))
        state.lifetimes = np.concatenate((state.lifetimes, lifetimes))
        state.colors = np.concatenate((state.colors, colors))
        state.sizes = np.concatenate((state.sizes, sizes))
        state.rotations = np.concatenate((state.rotations, rotations))
        state.atlas_indices = np.concatenate((state.atlas_indices, atlas_indices))

    def _spawn_positions(self, model: ParticleSystemModel, count: int) -> np.ndarray:
        radius = self._emitter_radius(model)
        if model.emitter_type == "NiPSysSphereEmitter":
            return self._unit_vectors(count) * self._rng.uniform(0.0, radius, size=(count, 1))
        if model.emitter_type == "NiPSysCylinderEmitter":
            angles = self._rng.uniform(0.0, np.pi * 2.0, size=count)
            radii = self._rng.uniform(0.0, radius, size=count)
            heights = self._rng.uniform(-radius, radius, size=count)
            return np.column_stack((np.cos(angles) * radii, np.sin(angles) * radii, heights)).astype(np.float32)
        return self._rng.uniform(-radius, radius, size=(count, 3)).astype(np.float32)

    def _spawn_velocities(
        self,
        model: ParticleSystemModel,
        count: int,
        speed: float,
        variation: float,
    ) -> np.ndarray:
        if variation > 0.0:
            speeds = self._rng.uniform(
                max(0.0, speed - variation),
                speed + variation,
                size=(count, 1),
            )
        else:
            speeds = np.full((count, 1), speed, dtype=np.float32)
        return (self._spawn_directions(model, count) * speeds).astype(np.float32)

    def _spawn_directions(self, model: ParticleSystemModel, count: int) -> np.ndarray:
        if model.emitter_declination is None and model.emitter_planar_angle is None:
            return self._unit_vectors(count)

        declination = self._angle_values(
            model.emitter_declination or 0.0,
            model.emitter_declination_variation or 0.0,
            count,
        )
        planar_angle = self._angle_values(
            model.emitter_planar_angle or 0.0,
            model.emitter_planar_angle_variation or 0.0,
            count,
        )
        return np.column_stack((
            np.sin(declination) * np.cos(planar_angle),
            np.sin(declination) * np.sin(planar_angle),
            np.cos(declination),
        )).astype(np.float32)

    def _angle_values(self, value: float, variation: float, count: int) -> np.ndarray:
        if variation <= 0.0:
            return np.full((count,), value, dtype=np.float32)
        return self._rng.uniform(value - variation, value + variation, size=count).astype(np.float32)

    def _unit_vectors(self, count: int) -> np.ndarray:
        vectors = self._rng.normal(size=(count, 3))
        lengths = np.linalg.norm(vectors, axis=1, keepdims=True)
        return vectors / np.maximum(lengths, 0.0001)

    def _spawn_atlas_indices(self, model: ParticleSystemModel, count: int) -> np.ndarray:
        atlas_count = max(1, len(model.atlas_offsets))
        return self._rng.integers(0, atlas_count, size=count, dtype=np.int32)

    def _atlas_offsets_array(self, model: ParticleSystemModel) -> np.ndarray:
        if model.atlas_offsets:
            return np.array(model.atlas_offsets, dtype=np.float32)
        return np.array(((0.0, 1.0, 0.0, 1.0),), dtype=np.float32)

    def _particle_limit(self, model: ParticleSystemModel) -> int:
        max_particles = 256 if model.max_particles is None else model.max_particles
        return max(0, min(int(max_particles), _MAX_PREVIEW_PARTICLES))

    def _spawn_rate(self, model: ParticleSystemModel) -> float:
        override = self.overrides.get(model.system_block_id)
        if override and override.spawn_rate is not None:
            return max(0.0, float(override.spawn_rate))
        return min(32.0, max(8.0, float(self._particle_limit(model))))

    def _speed(self, model: ParticleSystemModel) -> float:
        override = self.overrides.get(model.system_block_id)
        if override and override.speed is not None:
            return max(0.0, float(override.speed))
        if model.emitter_speed is not None:
            return max(0.0, float(model.emitter_speed))
        return 0.75

    def _speed_variation(self, model: ParticleSystemModel) -> float:
        if model.emitter_speed_variation is not None:
            return max(0.0, float(model.emitter_speed_variation))
        return 0.0

    def _lifetime(self, model: ParticleSystemModel) -> float:
        override = self.overrides.get(model.system_block_id)
        if override and override.lifetime is not None:
            return max(0.01, float(override.lifetime))
        if model.emitter_lifetime is not None:
            return max(0.01, float(model.emitter_lifetime))
        return 2.5

    def _lifetimes(self, model: ParticleSystemModel, count: int) -> np.ndarray:
        lifetime = self._lifetime(model)
        variation = max(0.0, float(model.emitter_lifetime_variation or 0.0))
        if variation <= 0.0:
            return np.full((count,), lifetime, dtype=np.float32)
        return self._rng.uniform(
            max(0.01, lifetime - variation),
            max(0.01, lifetime + variation),
            size=count,
        ).astype(np.float32)

    def _color_tint(self, model: ParticleSystemModel) -> tuple[float, float, float, float]:
        override = self.overrides.get(model.system_block_id)
        if override and override.color_tint is not None:
            return tuple(max(0.0, min(1.0, float(value))) for value in override.color_tint)
        color = model.base_color or model.emitter_initial_color
        if color is not None:
            return tuple(max(0.0, min(1.0, float(value))) for value in color)
        return (1.0, 1.0, 1.0, 1.0)

    def _sizes(self, model: ParticleSystemModel, count: int) -> np.ndarray:
        if model.emitter_initial_radius is None:
            return self._rng.uniform(0.08, 0.18, size=count).astype(np.float32)
        radius = max(0.001, float(model.emitter_initial_radius))
        variation = max(0.0, float(model.emitter_radius_variation or 0.0))
        if variation <= 0.0:
            return np.full((count,), radius, dtype=np.float32)
        return self._rng.uniform(
            max(0.001, radius - variation),
            max(0.001, radius + variation),
            size=count,
        ).astype(np.float32)

    def _emitter_radius(self, model: ParticleSystemModel) -> float:
        if model.emitter_radius is not None:
            return max(0.001, float(model.emitter_radius))
        return 0.25

    def _time_scale(self, model: ParticleSystemModel) -> float:
        override = self.overrides.get(model.system_block_id)
        if override and override.time_scale is not None:
            return max(0.0, float(override.time_scale))
        return 1.0
