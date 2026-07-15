import pytest

from creation_lib.nif.nif_file import NifFile

from ui.editor.animation_authoring import (
    AuthoringTarget,
    ControllerChainSpec,
    LinkContext,
    ValueKind,
    add_controller_chain,
    remove_sequence_controller,
    remove_standalone_controller,
)


def _new_nif():
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    return nif


def _float_spec(prop, variable):
    return ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": variable},
        start_time=0.0,
        stop_time=1.0,
    )


def test_add_standalone_float_chain_creates_controller_interpolator_and_data():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    target = AuthoringTarget(
        block_id=prop.block_id,
        display_name="fx1:0",
        target_kind="effect_shader_property",
        property_type="BSEffectShaderProperty",
    )
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=target,
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "V Offset"},
        start_time=0.0,
        stop_time=1.0,
    )

    result = add_controller_chain(nif, spec)

    controller = nif.get_block(result.controller_id)
    interp = nif.get_block(result.interpolator_id)
    data = nif.get_block(result.data_id)
    assert prop.get_field("Controller") == controller.block_id
    assert controller.type_name == "BSEffectShaderPropertyFloatController"
    assert controller.get_field("Target") == prop.block_id
    assert controller.get_field("Interpolator") == interp.block_id
    assert controller.get_field("Controlled Variable") == "V Offset"
    assert interp.type_name == "NiFloatInterpolator"
    assert interp.get_field("Data") == data.block_id
    assert data.get_field("Data")["Keys"] == [
        {"Time": 0.0, "Value": 0.0, "Interpolation": 1},
        {"Time": 1.0, "Value": 1.0, "Interpolation": 1},
    ]


def test_add_standalone_point3_chain_writes_pos_data_keys():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    target = AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty")
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyColorController",
        target=target,
        value_kind=ValueKind.POINT3,
        interpolator_type="NiPoint3Interpolator",
        data_type="NiPosData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, (1.0, 1.0, 1.0)), (1.0, (0.2, 0.3, 0.4))],
        controlled_fields={"Controlled Color": "Emissive Color"},
        start_time=0.0,
        stop_time=1.0,
    )

    result = add_controller_chain(nif, spec)

    data = nif.get_block(result.data_id)
    assert data.type_name == "NiPosData"
    assert data.get_field("Data")["Keys"] == [
        {"Time": 0.0, "Value": {"x": 1.0, "y": 1.0, "z": 1.0}, "Interpolation": 1},
        {"Time": 1.0, "Value": {"x": 0.2, "y": 0.3, "z": 0.4}, "Interpolation": 1},
    ]


def test_add_standalone_transform_chain_writes_transform_key_groups():
    nif = _new_nif()
    target = AuthoringTarget(0, "Root", "node")
    spec = ControllerChainSpec(
        controller_type="NiTransformController",
        target=target,
        value_kind=ValueKind.TRANSFORM,
        interpolator_type="NiTransformInterpolator",
        data_type="NiTransformData",
        link_context=LinkContext.STANDALONE,
        keys=[
            (0.0, {"translation": (0.0, 1.0, 2.0), "rotation": (0.1, 0.2, 0.3), "scale": 1.0}),
            (1.0, {"translation": (3.0, 4.0, 5.0), "rotation": (0.4, 0.5, 0.6), "scale": 2.0}),
        ],
    )

    result = add_controller_chain(nif, spec)

    data = nif.get_block(result.data_id)
    assert data.type_name == "NiTransformData"
    assert data.get_field("Translations")["Keys"] == [
        {"Time": 0.0, "Value": {"x": 0.0, "y": 1.0, "z": 2.0}, "Interpolation": 1},
        {"Time": 1.0, "Value": {"x": 3.0, "y": 4.0, "z": 5.0}, "Interpolation": 1},
    ]
    assert data.get_field("Scales")["Keys"] == [
        {"Time": 0.0, "Value": 1.0, "Interpolation": 1},
        {"Time": 1.0, "Value": 2.0, "Interpolation": 1},
    ]
    assert data.get_field("Rotation Type") == 4
    assert data.get_field("XYZ Rotations")[0]["Keys"] == [
        {"Time": 0.0, "Value": 0.1},
        {"Time": 1.0, "Value": 0.4},
    ]


def test_add_sequence_chain_appends_controlled_block_metadata():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_test",
            "Start Time": 0.0,
            "Stop Time": 0.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    target = AuthoringTarget(
        block_id=prop.block_id,
        display_name="fx1:0",
        target_kind="effect_shader_property",
        property_type="BSEffectShaderProperty",
    )
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=target,
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.SEQUENCE,
        keys=[(0.0, 0.0), (2.0, 1.0)],
        controlled_fields={"Controlled Variable": "Alpha Transparency"},
        start_time=0.0,
        stop_time=2.0,
        sequence_block_id=seq.block_id,
        node_name="fx1:0",
    )

    result = add_controller_chain(nif, spec)

    controlled = seq.get_field("Controlled Blocks")
    assert seq.get_field("Num Controlled Blocks") == 1
    assert seq.get_field("Stop Time") == 2.0
    assert controlled == [
        {
            "Interpolator": result.interpolator_id,
            "Controller": result.controller_id,
            "Priority": 0,
            "Node Name": "fx1:0",
            "Property Type": "BSEffectShaderProperty",
            "Controller Type": "BSEffectShaderPropertyFloatController",
            "Controller ID": "Alpha Transparency",
            "Interpolator ID": "",
        }
    ]


def test_remove_sequence_controller_removes_matching_controlled_block():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_test",
            "Start Time": 0.0,
            "Stop Time": 1.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.SEQUENCE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "V Offset"},
        sequence_block_id=seq.block_id,
        node_name="fx1:0",
    )
    result = add_controller_chain(nif, spec)

    removed = remove_sequence_controller(nif, seq.block_id, result.controller_id)

    assert seq.get_field("Controlled Blocks") == []
    assert seq.get_field("Num Controlled Blocks") == 0
    assert result.controller_id in removed.removed_block_ids


def test_remove_sequence_controller_keeps_shared_interpolator_and_data():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_test",
            "Start Time": 0.0,
            "Stop Time": 1.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    first_spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.SEQUENCE,
        keys=[(0.0, 0.0), (1.0, 1.0)],
        controlled_fields={"Controlled Variable": "U Offset"},
        sequence_block_id=seq.block_id,
        node_name="fx1:0",
    )
    second_spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.SEQUENCE,
        keys=[(0.0, 1.0), (1.0, 0.0)],
        controlled_fields={"Controlled Variable": "V Offset"},
        sequence_block_id=seq.block_id,
        node_name="fx1:0",
    )
    first = add_controller_chain(nif, first_spec)
    second = add_controller_chain(nif, second_spec)
    second_controller = nif.get_block(second.controller_id)
    second_controller.set_field("Interpolator", first.interpolator_id)
    controlled = seq.get_field("Controlled Blocks")
    controlled[1]["Interpolator"] = first.interpolator_id

    removed = remove_sequence_controller(nif, seq.block_id, first.controller_id)

    assert first.controller_id in removed.removed_block_ids
    assert first.interpolator_id not in removed.removed_block_ids
    assert first.data_id not in removed.removed_block_ids
    assert len(seq.get_field("Controlled Blocks")) == 1
    remaining_controller = nif.get_block(seq.get_field("Controlled Blocks")[0]["Controller"])
    remaining_interpolator = nif.get_block(remaining_controller.get_field("Interpolator"))
    assert remaining_interpolator.type_name == "NiFloatInterpolator"
    assert nif.get_block(remaining_interpolator.get_field("Data")).type_name == "NiFloatData"


def test_add_chain_missing_target_does_not_mutate_nif():
    nif = _new_nif()
    before = len(nif.blocks)
    target = AuthoringTarget(99, "Missing", "node")
    spec = ControllerChainSpec(
        controller_type="NiTransformController",
        target=target,
        value_kind=ValueKind.TRANSFORM,
        interpolator_type="NiTransformInterpolator",
        data_type="NiTransformData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, {"translation": (0.0, 0.0, 0.0), "rotation": (0.0, 0.0, 0.0), "scale": 1.0})],
    )

    with pytest.raises(ValueError, match="Target block not found"):
        add_controller_chain(nif, spec)

    assert len(nif.blocks) == before


def test_add_chain_invalid_transform_value_does_not_mutate_nif():
    nif = _new_nif()
    before = len(nif.blocks)
    spec = ControllerChainSpec(
        controller_type="NiTransformController",
        target=AuthoringTarget(0, "Root", "node"),
        value_kind=ValueKind.TRANSFORM,
        interpolator_type="NiTransformInterpolator",
        data_type="NiTransformData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, 0.0)],
    )

    with pytest.raises(ValueError, match="Transform keys must be dictionaries"):
        add_controller_chain(nif, spec)

    assert len(nif.blocks) == before


def test_add_chain_cycle_detection_does_not_mutate_nif():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    controller = nif.add_block("BSEffectShaderPropertyFloatController")
    prop.set_field("Controller", controller.block_id)
    controller.set_field("Next Controller", controller.block_id)
    before = len(nif.blocks)
    spec = ControllerChainSpec(
        controller_type="BSEffectShaderPropertyFloatController",
        target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
        value_kind=ValueKind.FLOAT,
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_context=LinkContext.STANDALONE,
        keys=[(0.0, 0.0)],
        controlled_fields={"Controlled Variable": "U Offset"},
    )

    with pytest.raises(ValueError, match="Controller chain contains a cycle"):
        add_controller_chain(nif, spec)

    assert len(nif.blocks) == before


def test_remove_first_standalone_controller_preserves_next_controller():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    first = add_controller_chain(nif, _float_spec(prop, "U Offset"))
    add_controller_chain(nif, _float_spec(prop, "V Offset"))

    removed = remove_standalone_controller(nif, prop.block_id, first.controller_id)

    assert removed.removed_block_ids == [first.controller_id, first.interpolator_id, first.data_id]
    remaining_controller = nif.get_block(prop.get_field("Controller"))
    assert remaining_controller.get_field("Controlled Variable") == "V Offset"


def test_remove_middle_standalone_controller_relinks_previous_to_next():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    first = add_controller_chain(nif, _float_spec(prop, "U Offset"))
    middle = add_controller_chain(nif, _float_spec(prop, "V Offset"))
    add_controller_chain(nif, _float_spec(prop, "EmissiveMultiple"))

    remove_standalone_controller(nif, prop.block_id, middle.controller_id)

    first_controller = nif.get_block(first.controller_id)
    next_controller = nif.get_block(first_controller.get_field("Next Controller"))
    assert next_controller.get_field("Controlled Variable") == "EmissiveMultiple"


def test_remove_keeps_shared_data_block_referenced_elsewhere():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    first = add_controller_chain(nif, _float_spec(prop, "U Offset"))
    second = add_controller_chain(nif, _float_spec(prop, "V Offset"))
    second_controller = nif.get_block(second.controller_id)
    second_controller.set_field("Interpolator", first.interpolator_id)

    removed = remove_standalone_controller(nif, prop.block_id, first.controller_id)

    assert first.controller_id in removed.removed_block_ids
    assert first.interpolator_id not in removed.removed_block_ids
    assert first.data_id not in removed.removed_block_ids


def test_remove_unlinked_standalone_controller_does_not_mutate_nif():
    nif = _new_nif()
    prop = nif.add_block("BSEffectShaderProperty")
    other = nif.add_block("BSEffectShaderProperty")
    chain = add_controller_chain(nif, _float_spec(other, "U Offset"))
    before = len(nif.blocks)

    with pytest.raises(ValueError, match=f"Controller {chain.controller_id} is not linked from target {prop.block_id}"):
        remove_standalone_controller(nif, prop.block_id, chain.controller_id)

    assert len(nif.blocks) == before
