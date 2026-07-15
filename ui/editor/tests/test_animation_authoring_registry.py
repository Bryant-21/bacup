from creation_lib.nif.schema import get_schema

from ui.editor.animation_authoring import (
    LinkContext,
    SupportTier,
    ValueKind,
    build_controller_templates,
    build_controller_registry,
    iter_schema_controller_types,
)


def test_iter_schema_controller_types_finds_concrete_animation_controllers():
    schema = get_schema()

    controller_types = set(iter_schema_controller_types(schema))

    assert "BSEffectShaderPropertyFloatController" in controller_types
    assert "NiPSysEmitterCtlr" in controller_types
    assert "NiControllerManager" in controller_types
    assert "NiTimeController" not in controller_types


def test_registry_covers_every_concrete_schema_controller():
    schema = get_schema()
    registry = build_controller_registry(schema)
    missing = sorted(set(iter_schema_controller_types(schema)) - set(registry))

    assert missing == []


def test_registry_entries_have_actionable_metadata():
    schema = get_schema()
    registry = build_controller_registry(schema)

    for controller_type, entry in registry.items():
        assert entry.controller_type == controller_type
        assert entry.target_kind
        assert isinstance(entry.value_kind, ValueKind)
        assert isinstance(entry.support_tier, SupportTier)
        assert entry.link_contexts
        for context in entry.link_contexts:
            assert isinstance(context, LinkContext)


def test_common_controllers_get_friendly_overrides():
    registry = build_controller_registry(get_schema())

    effect_float = registry["BSEffectShaderPropertyFloatController"]
    particle_emitter = registry["NiPSysEmitterCtlr"]
    path_controller = registry["NiPathController"]
    controller_manager = registry["NiControllerManager"]

    assert effect_float.support_tier is SupportTier.FRIENDLY
    assert effect_float.value_kind is ValueKind.FLOAT
    assert effect_float.interpolator_type == "NiFloatInterpolator"
    assert effect_float.data_type == "NiFloatData"
    assert effect_float.controlled_field == "Controlled Variable"

    assert particle_emitter.support_tier is SupportTier.FRIENDLY
    assert particle_emitter.value_kind is ValueKind.PARTICLE

    assert path_controller.support_tier is SupportTier.ADVANCED
    assert path_controller.value_kind is ValueKind.TRANSFORM
    assert path_controller.interpolator_type == "NiTransformInterpolator"

    assert controller_manager.support_tier is SupportTier.READ_ONLY
    assert controller_manager.value_kind is ValueKind.MANAGER
    assert controller_manager.link_contexts == (LinkContext.MANAGER,)


def test_particle_lifecycle_controllers_do_not_claim_key_data():
    registry = build_controller_registry(get_schema())

    update_controller = registry["NiPSysUpdateCtlr"]

    assert update_controller.value_kind is ValueKind.PARTICLE
    assert update_controller.interpolator_type == ""
    assert update_controller.data_type == ""


def test_advanced_templates_include_every_registry_entry():
    registry = build_controller_registry(get_schema())
    templates = build_controller_templates(registry)
    controller_types = {chain.controller_type for template in templates for chain in template.chain_specs}

    assert set(registry) - {"NiControllerManager", "NiControllerSequence"} <= controller_types


def test_unkeyed_advanced_templates_are_not_authorable():
    registry = build_controller_registry(get_schema())
    templates = {template.template_id: template for template in build_controller_templates(registry)}

    update_controller = templates["advanced:NiPSysUpdateCtlr"]

    assert update_controller.chain_specs[0].controller_type == "NiPSysUpdateCtlr"
    assert update_controller.authorable is False
    assert update_controller.unsupported_reason == "Controller does not expose key data"


def test_friendly_texture_scroll_template_creates_two_float_chains():
    registry = build_controller_registry(get_schema())
    templates = {template.template_id: template for template in build_controller_templates(registry)}

    template = templates["texture_scroll"]

    assert template.display_name == "Texture Scroll"
    assert [chain.controlled_fields["Controlled Variable"] for chain in template.chain_specs] == [
        "U Offset",
        "V Offset",
    ]
