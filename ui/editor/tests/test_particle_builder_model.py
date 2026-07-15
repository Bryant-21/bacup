import pytest

from ui.editor.particles.builder_model import (
    BlendPreset,
    EmissionShape,
    ModifierKind,
    ParticleEffectDraft,
    ParticleModifierDraft,
    ParticleSystemDraft,
    ParticleSystemKind,
)
from ui.editor.particles.catalog import (
    BLEND_PRESETS,
    EMISSION_SHAPES,
    MODIFIER_CATALOG,
    SYSTEM_KIND_CATALOG,
    build_preset,
    get_blend_preset_entry,
    get_emission_shape_entry,
    get_modifier_catalog_entry,
    get_system_kind_entry,
)


def test_default_draft_creation_uses_friendly_authoring_defaults():
    modifier = ParticleModifierDraft(ModifierKind.GRAVITY, "Gravity")
    system = ParticleSystemDraft(display_name="Smoke Puff", modifiers=[modifier])
    effect = ParticleEffectDraft(effect_name="Campfire Smoke", systems=[system])

    assert system.kind is ParticleSystemKind.EMITTER
    assert system.emission_shape is EmissionShape.POINT
    assert system.texture_path is None
    assert system.atlas_rows == 1
    assert system.atlas_columns == 1
    assert system.subtexture_index == 0
    assert system.lifetime == 1.0
    assert system.emission_rate == 10.0
    assert system.speed == 1.0
    assert system.spread_degrees == 0.0
    assert system.particle_size == 1.0
    assert system.color_rgba == (1.0, 1.0, 1.0, 1.0)
    assert system.alpha == 1.0
    assert system.blend is BlendPreset.ADDITIVE
    assert system.modifiers == (modifier,)
    assert effect.active_system_index == 0
    assert effect.loop_preview is True
    assert effect.preview_time_scale == 1.0


def test_adding_removing_reordering_and_selecting_systems_returns_new_effects():
    smoke = ParticleSystemDraft(display_name="Smoke")
    sparks = ParticleSystemDraft(display_name="Sparks")
    glow = ParticleSystemDraft(display_name="Glow")

    effect = ParticleEffectDraft(effect_name="Fire").add_system(smoke)
    with_sparks = effect.add_system(sparks)
    reordered = with_sparks.add_system(glow).reorder_system(2, 0).select_system(1)
    updated = reordered.update_system(1, display_name="Smoke Updated", emission_rate=20.0)
    removed = updated.remove_system(0)

    assert effect.systems == (smoke,)
    assert with_sparks.systems == (smoke, sparks)
    assert reordered.systems == (glow, smoke, sparks)
    assert reordered.active_system_index == 1
    assert updated.systems[1].display_name == "Smoke Updated"
    assert updated.systems[1].emission_rate == 20.0
    assert removed.systems == (updated.systems[1], updated.systems[2])
    assert removed.active_system_index == 0


def test_reordering_active_system_to_end_keeps_selection_on_moved_system():
    smoke = ParticleSystemDraft(display_name="Smoke")
    sparks = ParticleSystemDraft(display_name="Sparks")
    glow = ParticleSystemDraft(display_name="Glow")
    effect = ParticleEffectDraft(systems=[smoke, sparks, glow], active_system_index=0)

    reordered = effect.reorder_system(0, 3)

    assert reordered.systems == (sparks, glow, smoke)
    assert reordered.active_system_index == 2


def test_adding_updating_removing_and_reordering_modifiers_returns_new_systems():
    gravity = ParticleModifierDraft(ModifierKind.GRAVITY, "Gravity", settings={"strength": 1.0})
    drag = ParticleModifierDraft(ModifierKind.DRAG, "Drag", settings={"amount": 0.25})
    wind = ParticleModifierDraft(ModifierKind.WIND, "Wind")

    system = ParticleSystemDraft(display_name="Smoke").add_modifier(gravity).add_modifier(drag)
    with_wind = system.add_modifier(wind, index=1)
    reordered = with_wind.reorder_modifier(2, 0)
    updated = reordered.update_modifier(1, enabled=False, settings={"strength": 0.5})
    removed = updated.remove_modifier(0)

    assert system.modifiers == (gravity, drag)
    assert with_wind.modifiers == (gravity, wind, drag)
    assert reordered.modifiers == (drag, gravity, wind)
    assert updated.modifiers[1].enabled is False
    assert updated.modifiers[1].settings["strength"] == 0.5
    assert removed.modifiers == (updated.modifiers[1], updated.modifiers[2])


def test_add_modifier_rejects_invalid_insert_indexes():
    gravity = ParticleModifierDraft(ModifierKind.GRAVITY, "Gravity")
    system = ParticleSystemDraft(display_name="Smoke")

    with pytest.raises(IndexError):
        system.add_modifier(gravity, index=-1)
    with pytest.raises(IndexError):
        system.add_modifier(gravity, index=1)


def test_catalog_contains_all_enum_values_and_valid_preset_drafts():
    assert {entry.kind for entry in SYSTEM_KIND_CATALOG.values()} == set(ParticleSystemKind)
    assert {entry.kind for entry in EMISSION_SHAPES.values()} == set(EmissionShape)
    assert {entry.kind for entry in BLEND_PRESETS.values()} == set(BlendPreset)
    assert {entry.kind for entry in MODIFIER_CATALOG.values()} == set(ModifierKind)

    assert get_system_kind_entry(ParticleSystemKind.RIBBON).friendly_name == "Ribbon"
    assert get_emission_shape_entry(EmissionShape.SPHERE).friendly_name == "Sphere"
    assert get_blend_preset_entry(BlendPreset.ALPHA).friendly_name == "Alpha"
    assert get_modifier_catalog_entry(ModifierKind.GRAVITY).friendly_name == "Gravity"

    for key in ("smoke_puff", "spark_burst", "magic_glow", "falling_embers", "beam_ribbon"):
        draft = build_preset(key)
        assert isinstance(draft, ParticleSystemDraft)
        assert draft.display_name
        assert draft.atlas_rows >= 1
        assert draft.atlas_columns >= 1
        assert draft.lifetime > 0
        assert draft.emission_rate >= 0
        assert 0.0 <= draft.alpha <= 1.0
        assert len(draft.color_rgba) == 4
        for modifier in draft.modifiers:
            assert modifier.kind in ModifierKind


def test_mutable_inputs_are_defensively_copied_and_exposed_read_only():
    modifier_settings = {"curve": [0.0, 1.0]}
    modifier = ParticleModifierDraft(ModifierKind.SIZE_OVER_LIFE, "Size", settings=modifier_settings)
    raw_overrides = {"controller": {"frequency": 1.0}}
    modifiers = [modifier]
    system = ParticleSystemDraft(
        display_name="Mutable Inputs",
        color_rgba=[0.2, 0.4, 0.6, 0.8],
        modifiers=modifiers,
        raw_overrides=raw_overrides,
    )
    systems = [system]
    effect = ParticleEffectDraft(effect_name="Mutable Effect", systems=systems)

    modifier_settings["curve"].append(2.0)
    raw_overrides["controller"]["frequency"] = 2.0
    modifiers.append(ParticleModifierDraft(ModifierKind.DRAG, "Drag"))
    systems.append(ParticleSystemDraft(display_name="Late System"))

    assert modifier.settings["curve"] == (0.0, 1.0)
    assert system.raw_overrides["controller"]["frequency"] == 1.0
    assert system.modifiers == (modifier,)
    assert system.color_rgba == (0.2, 0.4, 0.6, 0.8)
    assert effect.systems == (system,)

    with pytest.raises(TypeError):
        modifier.settings["new"] = 1.0
    with pytest.raises(TypeError):
        system.raw_overrides["new"] = 1.0
