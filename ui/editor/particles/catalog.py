from __future__ import annotations

from dataclasses import dataclass, field
from types import MappingProxyType
from typing import Any, Callable, Mapping, TypeVar

from .builder_model import (
    BlendPreset,
    EmissionShape,
    ModifierKind,
    ParticleModifierDraft,
    ParticleSystemDraft,
    ParticleSystemKind,
)


@dataclass(frozen=True)
class SystemKindCatalogEntry:
    kind: ParticleSystemKind
    friendly_name: str
    description: str


@dataclass(frozen=True)
class ModifierCatalogEntry:
    kind: ModifierKind
    friendly_name: str
    description: str
    default_settings: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "default_settings", MappingProxyType(dict(self.default_settings)))

    def create_draft(self) -> ParticleModifierDraft:
        return ParticleModifierDraft(self.kind, self.friendly_name, settings=self.default_settings)


@dataclass(frozen=True)
class FriendlyCatalogEntry:
    kind: EmissionShape | BlendPreset
    friendly_name: str
    description: str


SYSTEM_KIND_CATALOG: Mapping[ParticleSystemKind, SystemKindCatalogEntry] = MappingProxyType({
    ParticleSystemKind.EMITTER: SystemKindCatalogEntry(
        ParticleSystemKind.EMITTER,
        "Emitter",
        "General sprite particles emitted from a simple shape.",
    ),
    ParticleSystemKind.BOMB: SystemKindCatalogEntry(
        ParticleSystemKind.BOMB,
        "Bomb",
        "A short burst that expands quickly from a center point.",
    ),
    ParticleSystemKind.RIBBON: SystemKindCatalogEntry(
        ParticleSystemKind.RIBBON,
        "Ribbon",
        "A connected trail or beam that follows a path.",
    ),
    ParticleSystemKind.MESH_PARTICLES: SystemKindCatalogEntry(
        ParticleSystemKind.MESH_PARTICLES,
        "Mesh Particles",
        "Particles emitted from small mesh instances instead of flat sprites.",
    ),
    ParticleSystemKind.LEGACY: SystemKindCatalogEntry(
        ParticleSystemKind.LEGACY,
        "Legacy",
        "Compatibility draft for older particle systems with raw overrides.",
    ),
})


EMISSION_SHAPES: Mapping[EmissionShape, FriendlyCatalogEntry] = MappingProxyType({
    EmissionShape.POINT: FriendlyCatalogEntry(EmissionShape.POINT, "Point", "Emit from a single origin."),
    EmissionShape.SPHERE: FriendlyCatalogEntry(EmissionShape.SPHERE, "Sphere", "Emit from a rounded volume."),
    EmissionShape.BOX: FriendlyCatalogEntry(EmissionShape.BOX, "Box", "Emit from a rectangular volume."),
    EmissionShape.CONE: FriendlyCatalogEntry(EmissionShape.CONE, "Cone", "Emit outward through a cone angle."),
    EmissionShape.CYLINDER: FriendlyCatalogEntry(EmissionShape.CYLINDER, "Cylinder", "Emit from a cylindrical volume."),
    EmissionShape.DISC: FriendlyCatalogEntry(EmissionShape.DISC, "Disc", "Emit across a flat circular area."),
})


BLEND_PRESETS: Mapping[BlendPreset, FriendlyCatalogEntry] = MappingProxyType({
    BlendPreset.ADDITIVE: FriendlyCatalogEntry(BlendPreset.ADDITIVE, "Additive", "Bright particles that add light."),
    BlendPreset.ALPHA: FriendlyCatalogEntry(BlendPreset.ALPHA, "Alpha", "Standard transparent particles."),
    BlendPreset.SOFT_ADDITIVE: FriendlyCatalogEntry(BlendPreset.SOFT_ADDITIVE, "Soft Additive", "Additive particles with softer edges."),
    BlendPreset.MULTIPLY: FriendlyCatalogEntry(BlendPreset.MULTIPLY, "Multiply", "Darkening particles such as soot or shadow."),
})


MODIFIER_CATALOG: Mapping[ModifierKind, ModifierCatalogEntry] = MappingProxyType({
    ModifierKind.GRAVITY: ModifierCatalogEntry(
        ModifierKind.GRAVITY,
        "Gravity",
        "Pull particles in a direction over time.",
        {"strength": 1.0, "direction": (0.0, 0.0, -1.0)},
    ),
    ModifierKind.DRAG: ModifierCatalogEntry(
        ModifierKind.DRAG,
        "Drag",
        "Slow particles as they travel.",
        {"amount": 0.2},
    ),
    ModifierKind.WIND: ModifierCatalogEntry(
        ModifierKind.WIND,
        "Wind",
        "Push particles sideways with a steady force.",
        {"strength": 0.5, "direction": (1.0, 0.0, 0.0)},
    ),
    ModifierKind.ROTATION: ModifierCatalogEntry(
        ModifierKind.ROTATION,
        "Rotation",
        "Spin particles during their lifetime.",
        {"degrees_per_second": 90.0},
    ),
    ModifierKind.SIZE_OVER_LIFE: ModifierCatalogEntry(
        ModifierKind.SIZE_OVER_LIFE,
        "Size Over Life",
        "Scale particles along a lifetime curve.",
        {"curve": (1.0, 0.75, 0.0)},
    ),
    ModifierKind.COLOR_OVER_LIFE: ModifierCatalogEntry(
        ModifierKind.COLOR_OVER_LIFE,
        "Color Over Life",
        "Tint particles along a lifetime gradient.",
        {"start_color": (1.0, 1.0, 1.0, 1.0), "end_color": (1.0, 1.0, 1.0, 0.0)},
    ),
    ModifierKind.ALPHA_OVER_LIFE: ModifierCatalogEntry(
        ModifierKind.ALPHA_OVER_LIFE,
        "Alpha Over Life",
        "Fade particles in or out over time.",
        {"curve": (0.0, 1.0, 0.0)},
    ),
    ModifierKind.COLLISION: ModifierCatalogEntry(
        ModifierKind.COLLISION,
        "Collision",
        "Bounce or stop particles when they hit nearby geometry.",
        {"bounce": 0.25},
    ),
    ModifierKind.SPAWN_RATE: ModifierCatalogEntry(
        ModifierKind.SPAWN_RATE,
        "Spawn Rate",
        "Change emission rate while the effect plays.",
        {"curve": (1.0, 1.0)},
    ),
})


def build_preset(key: str) -> ParticleSystemDraft:
    try:
        return _PRESETS[key]()
    except KeyError as exc:
        raise KeyError(f"Unknown particle system preset: {key}") from exc


def preset_keys() -> tuple[str, ...]:
    return tuple(_PRESETS)


def get_system_kind_entry(kind: ParticleSystemKind | str) -> SystemKindCatalogEntry:
    return SYSTEM_KIND_CATALOG[_coerce_enum(ParticleSystemKind, kind)]


def get_emission_shape_entry(kind: EmissionShape | str) -> FriendlyCatalogEntry:
    return EMISSION_SHAPES[_coerce_enum(EmissionShape, kind)]


def get_blend_preset_entry(kind: BlendPreset | str) -> FriendlyCatalogEntry:
    return BLEND_PRESETS[_coerce_enum(BlendPreset, kind)]


def get_modifier_catalog_entry(kind: ModifierKind | str) -> ModifierCatalogEntry:
    return MODIFIER_CATALOG[_coerce_enum(ModifierKind, kind)]


def _modifier(kind: ModifierKind) -> ParticleModifierDraft:
    return get_modifier_catalog_entry(kind).create_draft()


def _smoke_puff() -> ParticleSystemDraft:
    return ParticleSystemDraft(
        kind=ParticleSystemKind.EMITTER,
        display_name="Smoke Puff",
        emission_shape=EmissionShape.SPHERE,
        texture_path=r"textures\effects\smoke.dds",
        lifetime=2.5,
        emission_rate=18.0,
        speed=0.35,
        spread_degrees=35.0,
        particle_size=1.4,
        color_rgba=(0.55, 0.55, 0.55, 0.75),
        alpha=0.75,
        blend=BlendPreset.ALPHA,
        modifiers=(
            _modifier(ModifierKind.DRAG),
            _modifier(ModifierKind.SIZE_OVER_LIFE),
            _modifier(ModifierKind.ALPHA_OVER_LIFE),
        ),
    )


def _spark_burst() -> ParticleSystemDraft:
    return ParticleSystemDraft(
        kind=ParticleSystemKind.BOMB,
        display_name="Spark Burst",
        emission_shape=EmissionShape.POINT,
        texture_path=r"textures\effects\spark.dds",
        lifetime=0.6,
        emission_rate=80.0,
        speed=5.0,
        spread_degrees=65.0,
        particle_size=0.08,
        color_rgba=(1.0, 0.75, 0.25, 1.0),
        blend=BlendPreset.ADDITIVE,
        modifiers=(
            _modifier(ModifierKind.GRAVITY),
            _modifier(ModifierKind.COLLISION),
        ),
    )


def _magic_glow() -> ParticleSystemDraft:
    return ParticleSystemDraft(
        kind=ParticleSystemKind.EMITTER,
        display_name="Magic Glow",
        emission_shape=EmissionShape.DISC,
        texture_path=r"textures\effects\magicglow.dds",
        lifetime=1.8,
        emission_rate=24.0,
        speed=0.15,
        spread_degrees=20.0,
        particle_size=0.8,
        color_rgba=(0.35, 0.65, 1.0, 0.85),
        alpha=0.85,
        blend=BlendPreset.SOFT_ADDITIVE,
        modifiers=(
            _modifier(ModifierKind.ROTATION),
            _modifier(ModifierKind.COLOR_OVER_LIFE),
            _modifier(ModifierKind.ALPHA_OVER_LIFE),
        ),
    )


def _falling_embers() -> ParticleSystemDraft:
    return ParticleSystemDraft(
        kind=ParticleSystemKind.EMITTER,
        display_name="Falling Embers",
        emission_shape=EmissionShape.BOX,
        texture_path=r"textures\effects\embers.dds",
        lifetime=3.0,
        emission_rate=12.0,
        speed=0.45,
        spread_degrees=15.0,
        particle_size=0.12,
        color_rgba=(1.0, 0.35, 0.08, 0.9),
        alpha=0.9,
        blend=BlendPreset.ADDITIVE,
        modifiers=(
            _modifier(ModifierKind.GRAVITY),
            _modifier(ModifierKind.WIND),
            _modifier(ModifierKind.ALPHA_OVER_LIFE),
        ),
    )


def _beam_ribbon() -> ParticleSystemDraft:
    return ParticleSystemDraft(
        kind=ParticleSystemKind.RIBBON,
        display_name="Beam Ribbon",
        emission_shape=EmissionShape.CONE,
        texture_path=r"textures\effects\beam.dds",
        lifetime=0.9,
        emission_rate=36.0,
        speed=2.0,
        spread_degrees=5.0,
        particle_size=0.3,
        color_rgba=(0.6, 0.9, 1.0, 0.95),
        alpha=0.95,
        blend=BlendPreset.SOFT_ADDITIVE,
        modifiers=(
            _modifier(ModifierKind.SIZE_OVER_LIFE),
            _modifier(ModifierKind.ALPHA_OVER_LIFE),
        ),
    )


EnumT = TypeVar("EnumT", bound=ParticleSystemKind | EmissionShape | BlendPreset | ModifierKind)


def _coerce_enum(enum_type: type[EnumT], value: EnumT | str) -> EnumT:
    if isinstance(value, enum_type):
        return value
    normalized = value.lower().replace("_", "-")
    for enum_value in enum_type:
        if normalized in {enum_value.value, enum_value.name.lower().replace("_", "-")}:
            return enum_value
    raise KeyError(f"Unknown {enum_type.__name__}: {value}")


_PRESETS: Mapping[str, Callable[[], ParticleSystemDraft]] = MappingProxyType({
    "smoke_puff": _smoke_puff,
    "spark_burst": _spark_burst,
    "magic_glow": _magic_glow,
    "falling_embers": _falling_embers,
    "beam_ribbon": _beam_ribbon,
})
