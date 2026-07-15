from __future__ import annotations

from dataclasses import dataclass, field, replace
from enum import Enum
from types import MappingProxyType
from typing import Any, Mapping


class ParticleSystemKind(Enum):
    EMITTER = "emitter"
    BOMB = "bomb"
    RIBBON = "ribbon"
    MESH_PARTICLES = "mesh-particles"
    LEGACY = "legacy"


class EmissionShape(Enum):
    POINT = "point"
    SPHERE = "sphere"
    BOX = "box"
    CONE = "cone"
    CYLINDER = "cylinder"
    DISC = "disc"


class BlendPreset(Enum):
    ADDITIVE = "additive"
    ALPHA = "alpha"
    SOFT_ADDITIVE = "soft-additive"
    MULTIPLY = "multiply"


class ModifierKind(Enum):
    GRAVITY = "gravity"
    DRAG = "drag"
    WIND = "wind"
    ROTATION = "rotation"
    SIZE_OVER_LIFE = "size-over-life"
    COLOR_OVER_LIFE = "color-over-life"
    ALPHA_OVER_LIFE = "alpha-over-life"
    COLLISION = "collision"
    SPAWN_RATE = "spawn-rate"


@dataclass(frozen=True)
class ParticleModifierDraft:
    kind: ModifierKind
    display_name: str
    enabled: bool = True
    settings: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "settings", _freeze_mapping(self.settings))

    def with_updates(self, **changes: Any) -> ParticleModifierDraft:
        return replace(self, **changes)


@dataclass(frozen=True)
class ParticleSystemDraft:
    kind: ParticleSystemKind = ParticleSystemKind.EMITTER
    display_name: str = "Particle System"
    emission_shape: EmissionShape = EmissionShape.POINT
    texture_path: str | None = None
    atlas_rows: int = 1
    atlas_columns: int = 1
    subtexture_index: int = 0
    lifetime: float = 1.0
    emission_rate: float = 10.0
    speed: float = 1.0
    spread_degrees: float = 0.0
    particle_size: float = 1.0
    color_rgba: tuple[float, float, float, float] = (1.0, 1.0, 1.0, 1.0)
    alpha: float = 1.0
    blend: BlendPreset = BlendPreset.ADDITIVE
    modifiers: tuple[ParticleModifierDraft, ...] = field(default_factory=tuple)
    raw_overrides: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "color_rgba", _float_tuple(self.color_rgba, 4, "color_rgba"))
        object.__setattr__(self, "modifiers", tuple(self.modifiers))
        object.__setattr__(self, "raw_overrides", _freeze_mapping(self.raw_overrides))

    def with_updates(self, **changes: Any) -> ParticleSystemDraft:
        return replace(self, **changes)

    def add_modifier(self, modifier: ParticleModifierDraft, index: int | None = None) -> ParticleSystemDraft:
        modifiers = list(self.modifiers)
        insert_index = len(modifiers) if index is None else index
        _require_insert_index(insert_index, len(modifiers), "modifier")
        modifiers.insert(insert_index, modifier)
        return replace(self, modifiers=tuple(modifiers))

    def remove_modifier(self, index: int) -> ParticleSystemDraft:
        modifiers = list(self.modifiers)
        _require_index(index, len(modifiers), "modifier")
        del modifiers[index]
        return replace(self, modifiers=tuple(modifiers))

    def reorder_modifier(self, old_index: int, new_index: int) -> ParticleSystemDraft:
        modifiers = list(self.modifiers)
        _require_index(old_index, len(modifiers), "modifier")
        _require_insert_index(new_index, len(modifiers), "modifier")
        modifier = modifiers.pop(old_index)
        modifiers.insert(new_index, modifier)
        return replace(self, modifiers=tuple(modifiers))

    def update_modifier(self, index: int, modifier: ParticleModifierDraft | None = None, **changes: Any) -> ParticleSystemDraft:
        modifiers = list(self.modifiers)
        _require_index(index, len(modifiers), "modifier")
        if modifier is not None and changes:
            raise ValueError("Pass a replacement modifier or field updates, not both.")
        modifiers[index] = modifier if modifier is not None else modifiers[index].with_updates(**changes)
        return replace(self, modifiers=tuple(modifiers))


@dataclass(frozen=True)
class ParticleEffectDraft:
    effect_name: str = "Particle Effect"
    systems: tuple[ParticleSystemDraft, ...] = field(default_factory=tuple)
    active_system_index: int | None = None
    loop_preview: bool = True
    preview_time_scale: float = 1.0

    def __post_init__(self) -> None:
        systems = tuple(self.systems)
        active_system_index = self.active_system_index
        if active_system_index is None and systems:
            active_system_index = 0
        if active_system_index is not None:
            _require_index(active_system_index, len(systems), "system")
        object.__setattr__(self, "systems", systems)
        object.__setattr__(self, "active_system_index", active_system_index)

    def with_updates(self, **changes: Any) -> ParticleEffectDraft:
        return replace(self, **changes)

    def add_system(
        self,
        system: ParticleSystemDraft,
        index: int | None = None,
        select: bool = True,
    ) -> ParticleEffectDraft:
        systems = list(self.systems)
        insert_index = len(systems) if index is None else index
        _require_insert_index(insert_index, len(systems), "system")
        systems.insert(insert_index, system)
        active_system_index = insert_index if select else self._active_index_after_insert(insert_index)
        return replace(self, systems=tuple(systems), active_system_index=active_system_index)

    def remove_system(self, index: int) -> ParticleEffectDraft:
        systems = list(self.systems)
        _require_index(index, len(systems), "system")
        del systems[index]
        return replace(self, systems=tuple(systems), active_system_index=self._active_index_after_remove(index, len(systems)))

    def update_system(self, index: int, system: ParticleSystemDraft | None = None, **changes: Any) -> ParticleEffectDraft:
        systems = list(self.systems)
        _require_index(index, len(systems), "system")
        if system is not None and changes:
            raise ValueError("Pass a replacement system or field updates, not both.")
        systems[index] = system if system is not None else systems[index].with_updates(**changes)
        return replace(self, systems=tuple(systems))

    def select_system(self, index: int) -> ParticleEffectDraft:
        _require_index(index, len(self.systems), "system")
        return replace(self, active_system_index=index)

    def reorder_system(self, old_index: int, new_index: int) -> ParticleEffectDraft:
        systems = list(self.systems)
        _require_index(old_index, len(systems), "system")
        _require_insert_index(new_index, len(systems), "system")
        system = systems.pop(old_index)
        final_index = min(new_index, len(systems))
        systems.insert(final_index, system)
        return replace(self, systems=tuple(systems), active_system_index=_reordered_index(self.active_system_index, old_index, final_index))

    def _active_index_after_insert(self, insert_index: int) -> int | None:
        if self.active_system_index is None:
            return 0
        return self.active_system_index + 1 if insert_index <= self.active_system_index else self.active_system_index

    def _active_index_after_remove(self, removed_index: int, new_count: int) -> int | None:
        if new_count == 0 or self.active_system_index is None:
            return None
        if self.active_system_index == removed_index:
            return min(removed_index, new_count - 1)
        if self.active_system_index > removed_index:
            return self.active_system_index - 1
        return self.active_system_index


def _freeze_mapping(value: Mapping[str, Any]) -> Mapping[str, Any]:
    return MappingProxyType({key: _freeze_value(item) for key, item in dict(value).items()})


def _freeze_value(value: Any) -> Any:
    if isinstance(value, Mapping):
        return _freeze_mapping(value)
    if isinstance(value, list | tuple):
        return tuple(_freeze_value(item) for item in value)
    if isinstance(value, set | frozenset):
        return frozenset(_freeze_value(item) for item in value)
    return value


def _float_tuple(value: Any, length: int, field_name: str) -> tuple[float, ...]:
    values = tuple(float(item) for item in value)
    if len(values) != length:
        raise ValueError(f"{field_name} must have {length} values.")
    return values


def _require_index(index: int, count: int, label: str) -> None:
    if index < 0 or index >= count:
        raise IndexError(f"{label} index {index} is out of range.")


def _require_insert_index(index: int, count: int, label: str) -> None:
    if index < 0 or index > count:
        raise IndexError(f"{label} index {index} is out of range.")


def _reordered_index(active_index: int | None, old_index: int, new_index: int) -> int | None:
    if active_index is None:
        return None
    if active_index == old_index:
        return new_index
    if old_index < active_index <= new_index:
        return active_index - 1
    if new_index <= active_index < old_index:
        return active_index + 1
    return active_index
