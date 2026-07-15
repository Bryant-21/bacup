from types import SimpleNamespace

from creation_lib.nif.schema import get_schema

from ui.editor.animation_effects import build_effect_stacks
from ui.editor.animation import FloatKey
from ui.editor.panels import animation_editor as animation_editor_module
from ui.editor.panels.animation_editor import (
    AnimationEditorPanel,
    EditableChannel,
    EditableKey,
    EditableSequence,
    _advanced_effect_template_rejection,
)
from ui.editor.animation_authoring import LinkContext, build_controller_registry


def _channel(
    node,
    component,
    target_property,
    values,
    controller_type="BSEffectShaderPropertyFloatController",
):
    keys = [
        SimpleNamespace(time=float(i), value=float(value))
        for i, value in enumerate(values)
    ]
    return SimpleNamespace(
        node_name=node,
        component=component,
        target_property=target_property,
        controller_type=controller_type,
        property_type="BSEffectShaderProperty",
        keys=keys,
        data_block_id=10,
        interp_block_id=9,
    )


def test_groups_uv_offsets_as_digit_counter_for_named_counter_nodes():
    sequence = SimpleNamespace(
        name="GaussAmmoCount",
        channels=[
            _channel("RadsCountTens:0", "float", "V Offset", [0.0, 0.9]),
            _channel("RadsCountTens:0", "float", "U Offset", [0.0, 0.0]),
            _channel("RadsCountOnes:0", "float", "V Offset", [0.0, 0.9]),
            _channel("RadsCountOnes:0", "float", "U Offset", [0.0, 0.9]),
        ],
    )

    stacks = build_effect_stacks(sequence)

    assert [stack.effect_type for stack in stacks] == ["Digit Counter"]
    assert stacks[0].driver == "Scrub By Value"
    assert stacks[0].target_label == "RadsCountOnes:0, RadsCountTens:0"
    assert [channel.property_label for channel in stacks[0].channels] == [
        "V Offset",
        "U Offset",
        "V Offset",
        "U Offset",
    ]


def test_classifies_progress_bar_and_texture_scroll():
    progress = SimpleNamespace(
        name="x_ammoLoopOn",
        channels=[_channel("UIProgressBar:0", "float", "U Offset", [0.0, -0.5])],
    )
    scroll = SimpleNamespace(
        name="laser",
        channels=[_channel("Plane032:0", "float", "V Offset", [0.0, 9.0])],
    )

    assert build_effect_stacks(progress)[0].effect_type == "Progress Bar"
    assert build_effect_stacks(scroll)[0].effect_type == "Texture Scroll"


def test_same_target_distinct_effect_properties_build_separate_stacks():
    sequence = SimpleNamespace(
        name="x_fx",
        channels=[
            _channel("fx1:0", "float", "V Offset", [0.0, 9.0]),
            _channel("fx1:0", "float", "Alpha Transparency", [0.0, 1.0]),
            _channel("fx1:0", "float", "EmissiveMultiple", [0.5, 2.0]),
        ],
    )

    stacks = build_effect_stacks(sequence)

    assert [stack.effect_type for stack in stacks] == [
        "Texture Scroll",
        "Alpha Flicker",
        "Glow Pulse",
    ]
    assert [[channel.property_label for channel in stack.channels] for stack in stacks] == [
        ["V Offset"],
        ["Alpha Transparency"],
        ["EmissiveMultiple"],
    ]


def test_groups_color_and_light_channels_as_display_flicker():
    sequence = SimpleNamespace(
        name="DisplayFlicker",
        channels=[
            _channel(
                "RadsCountTens:0",
                "color_r",
                "Emissive Color",
                [1.0, 0.3],
                "BSEffectShaderPropertyColorController",
            ),
            _channel("object0", "float", "Dimmer", [1.0, 0.2], "NiLightDimmerController"),
        ],
    )

    stack = build_effect_stacks(sequence)[0]

    assert stack.effect_type == "Display Flicker"
    assert stack.timing == "Random-Looking Flicker"
    assert stack.output_label == "2 channels, 2 targets"


def test_set_channel_range_remaps_key_values():
    panel = AnimationEditorPanel(SimpleNamespace())
    channel = EditableChannel(
        label="Plane032:0 : V Offset",
        node_name="Plane032:0",
        component="float",
        keys=[
            EditableKey(0.0, 0.0),
            EditableKey(1.0, 9.0),
            EditableKey(2.0, 4.5),
        ],
    )

    panel._set_channel_range(channel, -1.0, 1.0)

    assert [round(key.value, 4) for key in channel.keys] == [-1.0, 1.0, 0.0]


def test_effect_channel_slider_bounds_reject_float_sentinel_values():
    assert animation_editor_module._effect_channel_slider_bounds(-3.402823466e38, 0.0, "V Offset") is None


def test_effect_channel_slider_bounds_cap_uv_offsets_to_reasonable_range():
    bounds = animation_editor_module._effect_channel_slider_bounds(-2.0, 9.0, "V Offset")

    assert bounds == (-10.0, 10.0)


def test_effect_channel_slider_bounds_reject_values_outside_property_cap():
    assert animation_editor_module._effect_channel_slider_bounds(2_000_000.0, 3_000_000.0, "U Offset") is None


def test_effect_channel_controls_draw_one_range_slider_per_row(monkeypatch):
    panel = AnimationEditorPanel(SimpleNamespace())
    panel._sequence = EditableSequence(
        name="RangeRows",
        block_id=1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="Plane032:0 : V Offset",
                node_name="Plane032:0",
                component="float",
                keys=[EditableKey(0.0, 0.0), EditableKey(1.0, 9.0)],
            )
        ],
    )
    stack = SimpleNamespace(
        channels=[
            SimpleNamespace(
                channel_index=0,
                node_name="Plane032:0",
                property_label="V Offset",
            )
        ]
    )
    row_count = 0
    slider_rows = []

    monkeypatch.setattr(animation_editor_module.imgui, "begin_table", lambda *args: True)
    monkeypatch.setattr(animation_editor_module.imgui, "table_setup_column", lambda *args: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_headers_row", lambda: None)

    def fake_table_next_row():
        nonlocal row_count
        row_count += 1

    def fake_slider_float(*args):
        slider_rows.append(row_count)
        return False, args[1]

    monkeypatch.setattr(animation_editor_module.imgui, "table_next_row", fake_table_next_row)
    monkeypatch.setattr(animation_editor_module.imgui, "table_next_column", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "push_id", lambda _id: None)
    monkeypatch.setattr(animation_editor_module.imgui, "pop_id", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "text", lambda _text: None)
    monkeypatch.setattr(animation_editor_module.imgui, "text_disabled", lambda _text: None)
    monkeypatch.setattr(animation_editor_module.imgui, "set_next_item_width", lambda _width: None)
    monkeypatch.setattr(animation_editor_module.imgui, "small_button", lambda _label: False)
    monkeypatch.setattr(animation_editor_module.imgui, "end_table", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "slider_float", fake_slider_float)

    panel._draw_effect_channel_controls(0, stack)

    assert row_count == 2
    assert slider_rows == [1, 2]


def test_effect_timing_controls_render_as_sliders(monkeypatch):
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    controller = nif.add_block(
        "BSEffectShaderPropertyFloatController",
        {
            "Start Time": 0.0,
            "Stop Time": 1.0,
            "Frequency": 1.0,
            "Phase": 0.0,
        },
    )
    panel = AnimationEditorPanel(
        SimpleNamespace(
            nif=nif,
            undo_manager=_Undo(),
            registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=SimpleNamespace(scan=lambda _nif: None))),
        )
    )
    panel._sequence = EditableSequence(
        name="Timing",
        block_id=1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="Plane032:0 : V Offset",
                node_name="Plane032:0",
                component="float",
                keys=[EditableKey(0.0, 0.0), EditableKey(1.0, 1.0)],
                controller_block_id=controller.block_id,
                target_property="V Offset",
                start_time=0.0,
                stop_time=1.0,
                frequency=1.0,
                phase=0.0,
            )
        ],
    )
    stack = SimpleNamespace(
        channels=[
            SimpleNamespace(
                channel_index=0,
                node_name="Plane032:0",
                property_label="V Offset",
            )
        ]
    )
    slider_labels = []

    monkeypatch.setattr(animation_editor_module.imgui, "begin_table", lambda *args: True)
    monkeypatch.setattr(animation_editor_module.imgui, "table_setup_column", lambda *args: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_headers_row", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_next_row", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_next_column", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "push_id", lambda _id: None)
    monkeypatch.setattr(animation_editor_module.imgui, "pop_id", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "text", lambda _text: None)
    monkeypatch.setattr(animation_editor_module.imgui, "text_disabled", lambda _text: None)
    monkeypatch.setattr(animation_editor_module.imgui, "set_next_item_width", lambda _width: None)
    monkeypatch.setattr(animation_editor_module.imgui, "small_button", lambda _label: False)
    monkeypatch.setattr(animation_editor_module.imgui, "end_table", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "input_float", lambda *args: (_ for _ in ()).throw(AssertionError("input_float should not be used for timing")))
    monkeypatch.setattr(
        animation_editor_module.imgui,
        "slider_float",
        lambda *args: slider_labels.append(args[0]) or (False, args[1]),
    )

    panel._draw_effect_channel_controls(0, stack)

    assert {"##start_time", "##stop_time", "##frequency", "##phase"} <= set(slider_labels)


def test_resolve_controller_property_maps_enum_int_values():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    controller = nif.add_block("BSEffectShaderPropertyFloatController")
    controller.set_field("Controlled Variable", 8)
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))

    assert panel._resolve_controller_property(
        "BSEffectShaderPropertyFloatController",
        None,
        controller,
    ) == "V Offset"


def test_write_back_controller_variable_updates_controller_and_channel_label():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    controller = nif.add_block("BSEffectShaderPropertyFloatController")
    controller.set_field("Controlled Variable", 8)
    undo = _Undo()
    manager = SimpleNamespace(scan=lambda _nif: None)
    panel = AnimationEditorPanel(
        SimpleNamespace(
            nif=nif,
            undo_manager=undo,
            registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
        )
    )
    panel._sequence = EditableSequence(
        name="Variable",
        block_id=1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="Plane032:0 : V Offset",
                node_name="Plane032:0",
                component="float",
                keys=[EditableKey(0.0, 0.0)],
                controller_block_id=controller.block_id,
                controller_type="BSEffectShaderPropertyFloatController",
                controlled_field="Controlled Variable",
                target_property="V Offset",
            )
        ],
    )

    panel._write_back_controller_field(0, "Controlled Variable", 6)

    assert controller.get_field("Controlled Variable") == 6
    assert panel._sequence.channels[0].target_property == "U Offset"
    assert panel._sequence.channels[0].label == "Plane032:0 : U Offset"
    assert len(undo.pushed) == 1


def test_write_back_float_channel_without_data_creates_nifloatdata():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    interp = nif.add_block("NiFloatInterpolator", {"Data": -1, "Value": 0.25})
    undo = _Undo()
    scans = []
    panel = AnimationEditorPanel(
        SimpleNamespace(
            nif=nif,
            undo_manager=undo,
            registry=SimpleNamespace(
                active_id="main",
                active_session=SimpleNamespace(anim_manager=SimpleNamespace(scan=lambda _nif: scans.append(_nif))),
            ),
        )
    )
    panel._sequence = EditableSequence(
        name="[Property Controllers]",
        block_id=-1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="Plane032:0 : V Offset",
                node_name="Plane032:0",
                component="float",
                keys=[EditableKey(0.0, 0.1), EditableKey(0.5, 0.8), EditableKey(1.0, 0.1)],
                interp_block_id=interp.block_id,
                data_block_id=-1,
                target_property="V Offset",
            )
        ],
    )

    panel._write_back_channel(0)

    data_id = interp.get_field("Data")
    data = nif.get_block(data_id)
    assert data.type_name == "NiFloatData"
    assert panel._sequence.channels[0].data_block_id == data_id
    assert data.get_field("Data")["Keys"] == [
        {"Time": 0.0, "Value": 0.1, "Interpolation": 1},
        {"Time": 0.5, "Value": 0.8, "Interpolation": 1},
        {"Time": 1.0, "Value": 0.1, "Interpolation": 1},
    ]
    assert len(undo.pushed) == 1
    assert scans == [nif]


def test_effect_pulse_shape_materializes_nifloatdata_keys():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    interp = nif.add_block("NiFloatInterpolator", {"Data": -1, "Value": 0.0})
    undo = _Undo()
    panel = AnimationEditorPanel(
        SimpleNamespace(
            nif=nif,
            undo_manager=undo,
            registry=SimpleNamespace(
                active_id="main",
                active_session=SimpleNamespace(anim_manager=SimpleNamespace(scan=lambda _nif: None)),
            ),
        )
    )
    panel._sequence = EditableSequence(
        name="[Property Controllers]",
        block_id=-1,
        start_time=0.0,
        stop_time=2.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="Plane032:0 : Glow",
                node_name="Plane032:0",
                component="float",
                keys=[EditableKey(0.0, 0.0), EditableKey(2.0, 1.0)],
                interp_block_id=interp.block_id,
                data_block_id=-1,
                target_property="EmissiveMultiple",
                start_time=0.0,
                stop_time=2.0,
            )
        ],
    )

    panel._apply_effect_channel_shape(0, "pulse")

    data = nif.get_block(interp.get_field("Data"))
    assert [key.value for key in panel._sequence.channels[0].keys] == [0.0, 1.0, 0.0]
    assert data.get_field("Data")["Keys"] == [
        {"Time": 0.0, "Value": 0.0, "Interpolation": 1},
        {"Time": 1.0, "Value": 1.0, "Interpolation": 1},
        {"Time": 2.0, "Value": 0.0, "Interpolation": 1},
    ]


def test_effect_channel_controls_render_sentinel_ranges_without_sliders(monkeypatch):
    panel = AnimationEditorPanel(SimpleNamespace())
    panel._sequence = EditableSequence(
        name="SentinelRange",
        block_id=1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="Plane032:0 : V Offset",
                node_name="Plane032:0",
                component="float",
                keys=[EditableKey(0.0, -3.402823466e38)],
            )
        ],
    )
    stack = SimpleNamespace(
        channels=[
            SimpleNamespace(
                channel_index=0,
                node_name="Plane032:0",
                property_label="V Offset",
            )
        ]
    )
    slider_calls = []

    monkeypatch.setattr(animation_editor_module.imgui, "begin_table", lambda *args: True)
    monkeypatch.setattr(animation_editor_module.imgui, "table_setup_column", lambda *args: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_headers_row", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_next_row", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "table_next_column", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "push_id", lambda _id: None)
    monkeypatch.setattr(animation_editor_module.imgui, "pop_id", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "text", lambda _text: None)
    monkeypatch.setattr(animation_editor_module.imgui, "text_disabled", lambda _text: None)
    monkeypatch.setattr(animation_editor_module.imgui, "set_next_item_width", lambda _width: None)
    monkeypatch.setattr(animation_editor_module.imgui, "small_button", lambda _label: False)
    monkeypatch.setattr(animation_editor_module.imgui, "end_table", lambda: None)
    monkeypatch.setattr(
        animation_editor_module.imgui,
        "slider_float",
        lambda *args: slider_calls.append(args) or (False, args[1]),
    )

    panel._draw_effect_channel_controls(0, stack)

    assert slider_calls == []


def test_transport_controls_render_unsafe_sequence_ranges_without_slider(monkeypatch):
    panel = AnimationEditorPanel(
        SimpleNamespace(
            anim_coordinator=SimpleNamespace(
                pause=lambda: None,
                set_time=lambda _t: None,
            )
        )
    )
    panel._sequence = EditableSequence(
        name="UnsafeRange",
        block_id=1,
        start_time=0.0,
        stop_time=2.0e38,
        cycle_type=0,
    )
    text_calls = []

    monkeypatch.setattr(animation_editor_module.imgui, "text_disabled", lambda text: text_calls.append(text))
    monkeypatch.setattr(animation_editor_module.imgui, "same_line", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "separator", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "begin_disabled", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "end_disabled", lambda: None)
    monkeypatch.setattr(animation_editor_module.imgui, "set_next_item_width", lambda _width: None)
    monkeypatch.setattr(
        animation_editor_module.imgui,
        "slider_float",
        lambda *args: (_ for _ in ()).throw(AssertionError("slider_float should not be used")),
    )

    panel._draw_transport_controls()

    assert text_calls[0] == "Time"
    assert text_calls[1].endswith("/2e+38s")


def test_property_controller_sequence_imports_as_texture_scroll():
    manager = SimpleNamespace(
        _property_channels=[
            SimpleNamespace(
                node_name="fx1:0",
                material_var="V Offset",
                float_keys=[FloatKey(0.0, 0.0), FloatKey(1.0, 9.0)],
                stop_time=1.0,
                frequency=1.0,
            ),
            SimpleNamespace(
                node_name="fx1:0",
                material_var="U Offset",
                float_keys=[FloatKey(0.0, 0.0), FloatKey(1.0, 1.0)],
                stop_time=1.0,
                frequency=1.0,
            ),
        ],
        has_sequence=lambda _name: False,
    )
    panel = AnimationEditorPanel(SimpleNamespace(animation_mgr=manager))

    panel._load_property_controllers()

    assert [channel.target_property for channel in panel._sequence.channels] == [
        "V Offset",
        "U Offset",
    ]
    assert build_effect_stacks(panel._sequence)[0].effect_type == "Texture Scroll"


def test_unrecognized_controller_imports_as_advanced_controller_stack():
    sequence = SimpleNamespace(
        name="x_path",
        channels=[
            _channel(
                "PathNode",
                "float",
                "Float",
                [0.0, 1.0],
                "NiPathController",
            )
        ],
    )

    stack = build_effect_stacks(sequence)[0]

    assert stack.effect_type == "Advanced Controller"
    assert stack.channels[0].controller_type == "NiPathController"


def test_particle_controller_imports_as_particle_effect_stack():
    sequence = SimpleNamespace(
        name="Smoke",
        channels=[
            _channel(
                "SmokeL",
                "float",
                "Birth Rate",
                [0.0, 10.0],
                "NiPSysEmitterCtlr",
            )
        ],
    )

    stack = build_effect_stacks(sequence)[0]

    assert stack.effect_type == "Particle Emitter"


def test_uv_like_unknown_controller_stays_advanced_controller():
    sequence = SimpleNamespace(
        name="laser",
        channels=[
            _channel(
                "Plane032:0",
                "float",
                "V Offset",
                [0.0, 9.0],
                "NiPathController",
            )
        ],
    )

    assert build_effect_stacks(sequence)[0].effect_type == "Advanced Controller"


def test_uv_like_particle_controller_stays_particle_effect():
    sequence = SimpleNamespace(
        name="Smoke",
        channels=[
            _channel(
                "SmokeL",
                "float",
                "U Offset",
                [0.0, 1.0],
                "NiPSysEmitterCtlr",
            )
        ],
    )

    assert build_effect_stacks(sequence)[0].effect_type == "Particle Emitter"


class _Undo:
    def __init__(self):
        self.pushed = []

    def push(self, nif_id, action):
        self.pushed.append((nif_id, action))


def test_panel_add_effect_uses_snapshot_action_and_rescans():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    undo = _Undo()
    scans = []
    manager = SimpleNamespace(scan=lambda n: scans.append(n), _property_channels=[])
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)

    panel._add_effect_for_target(
        target_block_id=prop.block_id,
        template_id="texture_scroll_v",
        link_context=LinkContext.STANDALONE,
    )

    assert len(undo.pushed) == 1
    assert scans == [nif]
    assert nif.get_block(prop.block_id).get_field("Controller") >= 0


def test_add_effect_menu_on_named_sequence_appends_controlled_block(monkeypatch):
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    nif.add_block("BSTriShape", {"Name": "fx1:0", "Shader Property": prop.block_id})
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_scroll",
            "Start Time": 0.0,
            "Stop Time": 0.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    undo = _Undo()
    manager = SimpleNamespace(
        scan=lambda _nif: None,
        _property_channels=[],
        has_sequence=lambda _name: False,
    )
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)
    panel._sequence = panel._load_sequence_from_nif(nif, seq.block_id)

    monkeypatch.setattr(
        animation_editor_module.imgui,
        "menu_item",
        lambda label, shortcut, p_selected, enabled=True: (label == "Texture Scroll V", p_selected),
    )
    monkeypatch.setattr(animation_editor_module.imgui, "begin_menu", lambda _label: False)

    panel._draw_add_effect_menu()

    controlled = seq.get_field("Controlled Blocks")
    assert seq.get_field("Num Controlled Blocks") == 1
    assert controlled[0]["Controller ID"] == "V Offset"
    assert prop.get_field("Controller") == -1


def test_panel_remove_effect_uses_snapshot_action_and_rescans():
    from creation_lib.nif.nif_file import NifFile
    from ui.editor.animation_authoring import AuthoringTarget, ControllerChainSpec, ValueKind, add_controller_chain

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    result = add_controller_chain(
        nif,
        ControllerChainSpec(
            controller_type="BSEffectShaderPropertyFloatController",
            target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_context=LinkContext.STANDALONE,
            keys=[(0.0, 0.0), (1.0, 1.0)],
            controlled_fields={"Controlled Variable": "V Offset"},
        ),
    )
    undo = _Undo()
    scans = []
    manager = SimpleNamespace(scan=lambda n: scans.append(n), _property_channels=[])
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)

    panel._remove_standalone_effect(prop.block_id, result.controller_id)

    assert len(undo.pushed) == 1
    assert scans == [nif]
    assert nif.get_block(prop.block_id).get_field("Controller") == -1


def test_panel_add_effect_without_registry_does_not_mutate_nif():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif, undo_manager=_Undo()))

    panel._add_effect_for_target(prop.block_id, "texture_scroll_v", LinkContext.STANDALONE)

    assert nif.get_block(prop.block_id).get_field("Controller") == -1


def test_after_authoring_mutation_tolerates_session_without_anim_manager():
    panel = AnimationEditorPanel(
        SimpleNamespace(
            nif=SimpleNamespace(),
            registry=SimpleNamespace(active_session=SimpleNamespace()),
        )
    )
    panel._sequence = EditableSequence(
        name="[Property Controllers]",
        block_id=-1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[_channel("fx1:0", "float", "V Offset", [0.0, 1.0])],
    )

    panel._after_authoring_mutation()

    assert panel._sequence.channels == []


def test_remove_last_property_effect_clears_stale_property_sequence():
    from creation_lib.nif.nif_file import NifFile
    from ui.editor.animation import FloatKey
    from ui.editor.animation_authoring import AuthoringTarget, ControllerChainSpec, ValueKind, add_controller_chain

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    result = add_controller_chain(
        nif,
        ControllerChainSpec(
            controller_type="BSEffectShaderPropertyFloatController",
            target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_context=LinkContext.STANDALONE,
            keys=[(0.0, 0.0), (1.0, 1.0)],
            controlled_fields={"Controlled Variable": "V Offset"},
        ),
    )
    undo = _Undo()
    manager = SimpleNamespace(
        _property_channels=[
            SimpleNamespace(
                node_name="fx1:0",
                material_var="V Offset",
                float_keys=[FloatKey(0.0, 0.0), FloatKey(1.0, 1.0)],
                stop_time=1.0,
                frequency=1.0,
            )
        ],
        scan=lambda _nif: setattr(manager, "_property_channels", []),
    )
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)
    panel._sequence = EditableSequence(
        name="[Property Controllers]",
        block_id=-1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[_channel("fx1:0", "float", "V Offset", [0.0, 1.0])],
    )

    panel._remove_standalone_effect(prop.block_id, result.controller_id)

    assert panel._sequence.channels == []


def test_add_effect_rejects_lighting_shader_property_target():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    lighting = nif.add_block("BSLightingShaderProperty")
    undo = _Undo()
    manager = SimpleNamespace(scan=lambda _nif: None, _property_channels=[])
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)

    panel._add_effect_for_target(lighting.block_id, "texture_scroll_v", LinkContext.STANDALONE)

    assert undo.pushed == []
    assert lighting.get_field("Controller") == -1


def test_add_effect_rejects_advanced_template_without_key_data():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    undo = _Undo()
    manager = SimpleNamespace(scan=lambda _nif: None, _property_channels=[])
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)

    panel._add_effect_for_target(prop.block_id, "advanced:NiPSysUpdateCtlr", LinkContext.STANDALONE)

    assert undo.pushed == []
    assert prop.get_field("Controller") == -1


def test_add_effect_rejects_sequence_only_advanced_template_from_standalone_menu():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    undo = _Undo()
    manager = SimpleNamespace(scan=lambda _nif: None, _property_channels=[])
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)

    panel._add_effect_for_target(
        prop.block_id,
        "advanced:BSEffectShaderPropertyColorController",
        LinkContext.STANDALONE,
    )

    assert undo.pushed == []
    assert prop.get_field("Controller") == -1


def test_advanced_effect_menu_filters_templates_that_standalone_effects_cannot_author():
    registry = build_controller_registry(get_schema())

    assert _advanced_effect_template_rejection(registry["NiPSysUpdateCtlr"], LinkContext.STANDALONE)
    assert _advanced_effect_template_rejection(
        registry["BSEffectShaderPropertyColorController"],
        LinkContext.STANDALONE,
    )
    assert _advanced_effect_template_rejection(
        registry["BSEffectShaderPropertyFloatController"],
        LinkContext.STANDALONE,
    ) == ""


def test_add_effect_menu_passes_full_imgui_menu_item_signature(monkeypatch):
    seen = []
    added = []
    entry = SimpleNamespace(
        interpolator_type="NiFloatInterpolator",
        data_type="NiFloatData",
        link_contexts=(LinkContext.STANDALONE,),
        target_kind="effect_shader_property",
    )
    registry = {"BSEffectShaderPropertyFloatController": entry}
    template = SimpleNamespace(
        friendly=False,
        authorable=True,
        chain_specs=[SimpleNamespace(controller_type="BSEffectShaderPropertyFloatController")],
        display_name="Effect Shader Float",
        template_id="advanced:BSEffectShaderPropertyFloatController",
    )

    def fake_menu_item(label, shortcut, p_selected, enabled=True):
        seen.append((label, shortcut, p_selected, enabled))
        return False, p_selected

    monkeypatch.setattr(animation_editor_module.imgui, "menu_item", fake_menu_item)
    monkeypatch.setattr(animation_editor_module.imgui, "begin_menu", lambda _label: True)
    monkeypatch.setattr(animation_editor_module.imgui, "end_menu", lambda: None)
    monkeypatch.setattr(animation_editor_module, "get_schema", lambda: None)
    monkeypatch.setattr(animation_editor_module, "build_controller_registry", lambda _schema: registry)
    monkeypatch.setattr(animation_editor_module, "build_controller_templates", lambda _registry: [template])

    panel = AnimationEditorPanel(SimpleNamespace(nif=None))
    panel._selected_effect_target_id = lambda: 12
    panel._add_effect_for_target = lambda target, template_id, link_context: added.append(
        (target, template_id, link_context)
    )

    panel._draw_add_effect_menu()

    assert [call[0] for call in seen] == [
        "Texture Scroll U",
        "Texture Scroll V",
        "Glow Pulse",
        "Alpha Flicker",
        "Effect Shader Float",
    ]
    assert all(call[1:] == ("", False, True) for call in seen)
    assert added == []


def test_selected_effect_target_rejects_lighting_shader_channel():
    from creation_lib.nif.nif_file import NifFile
    from ui.editor.animation_authoring import AuthoringTarget, ControllerChainSpec, ValueKind, add_controller_chain

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    lighting = nif.add_block("BSLightingShaderProperty")
    result = add_controller_chain(
        nif,
        ControllerChainSpec(
            controller_type="BSLightingShaderPropertyFloatController",
            target=AuthoringTarget(lighting.block_id, "LightPlane:0", "lighting_shader_property", "BSLightingShaderProperty"),
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_context=LinkContext.STANDALONE,
            keys=[(0.0, 0.0), (1.0, 1.0)],
            controlled_fields={"Controlled Variable": "U Offset"},
        ),
    )
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))
    panel._sequence = EditableSequence(
        name="[Property Controllers]",
        block_id=-1,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="LightPlane:0 : U Offset",
                node_name="LightPlane:0",
                component="float",
                controller_block_id=result.controller_id,
                property_type="BSLightingShaderProperty",
                target_property="U Offset",
            )
        ],
    )
    panel._selected_channel_idx = 0

    assert panel._selected_effect_target_id() == -1


def test_selected_effect_target_falls_back_to_scene_tree_shader_selection():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    effect_prop = nif.add_block("BSEffectShaderProperty")
    shape = nif.add_block("BSTriShape", {"Name": "fx1:0", "Shader Property": effect_prop.block_id})
    panel = AnimationEditorPanel(
        SimpleNamespace(
            nif=nif,
            selection_mgr=SimpleNamespace(selected_block_id=shape.block_id),
        )
    )
    panel._sequence = EditableSequence(
        name="x_scroll",
        block_id=10,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="LightPlane:0 : U Offset",
                node_name="LightPlane:0",
                component="float",
                property_type="BSLightingShaderProperty",
                target_property="U Offset",
            )
        ],
    )
    panel._selected_channel_idx = 0

    assert panel._selected_effect_target_id() == effect_prop.block_id


def test_selected_effect_target_resolves_sequence_channel_without_controller_target():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    nif.add_block("BSTriShape", {"Name": "fx1:0", "Shader Property": prop.block_id})
    controller = nif.add_block("BSEffectShaderPropertyFloatController")
    controller.set_field("Target", -1)
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))
    panel._sequence = EditableSequence(
        name="x_scroll",
        block_id=10,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=0,
        channels=[
            EditableChannel(
                label="fx1:0 : V Offset",
                node_name="fx1:0",
                component="float",
                controller_block_id=controller.block_id,
                property_type="BSEffectShaderProperty",
                target_property="V Offset",
            )
        ],
    )
    panel._selected_channel_idx = 0

    assert panel._selected_effect_target_id() == prop.block_id


def test_advanced_effect_template_allows_sequence_transform_controller():
    registry = build_controller_registry(get_schema())

    assert _advanced_effect_template_rejection(registry["NiTransformController"], LinkContext.SEQUENCE) == ""


def test_add_advanced_transform_to_named_sequence_from_selected_shape():
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    shape = nif.add_block("BSTriShape", {"Name": "fx1:0"})
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_move",
            "Start Time": 0.0,
            "Stop Time": 0.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    undo = _Undo()
    manager = SimpleNamespace(
        scan=lambda _nif: None,
        _property_channels=[],
        has_sequence=lambda _name: False,
    )
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        selection_mgr=SimpleNamespace(selected_block_id=shape.block_id),
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)
    panel._sequence = panel._load_sequence_from_nif(nif, seq.block_id)

    panel._add_effect_for_target(shape.block_id, "advanced:NiTransformController", LinkContext.SEQUENCE)

    controlled = seq.get_field("Controlled Blocks")
    assert seq.get_field("Num Controlled Blocks") == 1
    assert controlled[0]["Controller Type"] == "NiTransformController"
    assert {channel.component for channel in panel._sequence.channels} >= {"pos_x", "pos_y"}


def test_loaded_sequence_channel_preserves_controller_timing_metadata():
    from creation_lib.nif.nif_file import NifFile
    from ui.editor.animation_authoring import AuthoringTarget, ControllerChainSpec, ValueKind, add_controller_chain

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_scroll",
            "Start Time": 0.0,
            "Stop Time": 3.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    add_controller_chain(
        nif,
        ControllerChainSpec(
            controller_type="BSEffectShaderPropertyFloatController",
            target=AuthoringTarget(prop.block_id, "fx1:0", "effect_shader_property", "BSEffectShaderProperty"),
            value_kind=ValueKind.FLOAT,
            interpolator_type="NiFloatInterpolator",
            data_type="NiFloatData",
            link_context=LinkContext.SEQUENCE,
            keys=[(0.25, 0.0), (2.0, 1.0)],
            controlled_fields={"Controlled Variable": "V Offset"},
            start_time=0.25,
            stop_time=2.0,
            frequency=1.5,
            phase=0.1,
            sequence_block_id=seq.block_id,
            node_name="fx1:0",
        ),
    )
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))

    loaded = panel._load_sequence_from_nif(nif, seq.block_id)
    channel = loaded.channels[0]

    assert channel.start_time == 0.25
    assert channel.stop_time == 2.0
    assert channel.frequency == 1.5
    assert channel.phase == 0.1


def test_write_back_channel_timing_updates_controller_fields():
    from creation_lib.nif.nif_file import NifFile
    from ui.editor.animation_authoring import AuthoringTarget, ControllerChainSpec, ValueKind, add_controller_chain

    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root"})
    prop = nif.add_block("BSEffectShaderProperty")
    seq = nif.add_block(
        "NiControllerSequence",
        {
            "Name": "x_scroll",
            "Start Time": 0.0,
            "Stop Time": 1.0,
            "Cycle Type": 0,
            "Num Controlled Blocks": 0,
            "Controlled Blocks": [],
        },
    )
    result = add_controller_chain(
        nif,
        ControllerChainSpec(
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
        ),
    )
    undo = _Undo()
    scans = []
    manager = SimpleNamespace(scan=lambda n: scans.append(n))
    app = SimpleNamespace(
        nif=nif,
        undo_manager=undo,
        registry=SimpleNamespace(active_id="main", active_session=SimpleNamespace(anim_manager=manager)),
    )
    panel = AnimationEditorPanel(app)
    panel._sequence = panel._load_sequence_from_nif(nif, seq.block_id)

    panel._write_back_channel_timing(0, start_time=0.25, stop_time=2.0, frequency=1.5, phase=0.1)

    controller = nif.get_block(result.controller_id)
    assert controller.get_field("Start Time") == 0.25
    assert controller.get_field("Stop Time") == 2.0
    assert controller.get_field("Frequency") == 1.5
    assert controller.get_field("Phase") == 0.1
    assert len(undo.pushed) == 1
    assert scans == [nif]
