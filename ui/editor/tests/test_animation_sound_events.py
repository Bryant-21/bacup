import sqlite3
from types import SimpleNamespace

import zstandard as zstd

from ui.editor.animation import AnimationManager, AnimSequence, SoundEvent
from ui.editor.panels.animation_editor import (
    AnimationEditorPanel,
    EditableChannel,
    EditableKey,
    EditableSequence,
    EditableSoundEvent,
)
from ui.editor.sound_events import (
    clear_sound_resolution_cache,
    parse_sound_text_key,
    resolve_sound_cue,
)


class _Block:
    def __init__(self, block_id=0, type_name="", **fields):
        self.block_id = block_id
        self.type_name = type_name
        self._fields = fields

    def get_field(self, name):
        return self._fields.get(name)

    def set_field(self, name, value):
        self._fields[name] = value


class _Schema:
    enums = {}
    _parents = {
        "NiFloatInterpolator": {"NiFloatInterpolator"},
        "NiPoint3Interpolator": {"NiPoint3Interpolator"},
        "NiBoolInterpolator": {"NiBoolInterpolator"},
        "NiTransformInterpolator": {"NiTransformInterpolator"},
    }

    def is_subtype_of(self, type_name, parent_type):
        return parent_type in self._parents.get(type_name, {type_name})

    def get_all_fields(self, _type_name):
        return []


class _Nif:
    def __init__(self, blocks):
        self._blocks = {block.block_id: block for block in blocks}
        self.schema = _Schema()

    def get_block(self, block_id):
        return self._blocks.get(block_id)


def test_parse_sound_text_key():
    assert parse_sound_text_key("Sound: DRScSafeContainerOpen") == "DRScSafeContainerOpen"
    assert parse_sound_text_key(" sound :  Foo ") == "Foo"
    assert parse_sound_text_key("start") is None


def test_resolve_sound_cue_uses_toolkit_extracted_dir_key(tmp_path, monkeypatch):
    from ui.editor import sound_events

    extracted = tmp_path / "extracted"
    sound_file = extracted / "Sound" / "FX" / "safe.xwm"
    sound_file.parent.mkdir(parents=True)
    sound_file.write_bytes(b"xwm")
    settings = SimpleNamespace(
        get_game_paths=lambda _game: {"extracted_dir": str(extracted)}
    )
    app = SimpleNamespace(_toolkit_settings=settings)

    monkeypatch.setattr(
        sound_events,
        "_lookup_sndr_sound_paths",
        lambda _cue, *, game_id: ["Sound/FX/safe.wav"],
    )

    resolved = resolve_sound_cue("DRScSafeContainerOpen", app, game_id="fo4")

    assert resolved.path == sound_file
    assert resolved.error == ""


def test_lookup_sndr_sound_paths_decodes_compressed_record_content(tmp_path, monkeypatch):
    from ui.editor import sound_events

    db_path = tmp_path / "records.db"
    yaml_content = (
        'form_id: "026105"\n'
        "eid: DRScSafeContainerOpen\n"
        "fields:\n"
        "- Sound: data\\Sound\\FX\\DRSc\\SafeContainer\\drsc_safe_container_open_01.wav\n"
        "- Sound: data\\Sound\\FX\\DRSc\\SafeContainer\\drsc_safe_container_open_02.wav\n"
    )
    compressed = zstd.ZstdCompressor().compress(yaml_content.encode("utf-8"))
    conn = sqlite3.connect(db_path)
    conn.execute(
        "create table records (editor_id text, record_type text, yaml_path text, content blob)"
    )
    conn.execute(
        "insert into records values (?, ?, ?, ?)",
        ("DRScSafeContainerOpen", "SNDR", "", compressed),
    )
    conn.commit()
    conn.close()
    monkeypatch.setattr(sound_events, "_records_db_path", lambda _game_id: db_path)

    paths = sound_events._lookup_sndr_sound_paths("DRScSafeContainerOpen", game_id="fo4")

    assert paths == [
        "data\\Sound\\FX\\DRSc\\SafeContainer\\drsc_safe_container_open_01.wav",
        "data\\Sound\\FX\\DRSc\\SafeContainer\\drsc_safe_container_open_02.wav",
    ]


def test_resolve_sound_cue_uses_archive_manager_without_extracted_dir(monkeypatch):
    from ui.editor import sound_events

    class ArchiveManager:
        def find(self, path):
            if path.lower().replace("\\", "/") == "sound/fx/safe.xwm":
                return b"xwm"
            return None

    settings = SimpleNamespace(
        get_game_paths=lambda _game: {"root_dir": "C:/Games/Fallout 4"}
    )
    app = SimpleNamespace(_toolkit_settings=settings, ba2_manager=ArchiveManager())

    monkeypatch.setattr(
        sound_events,
        "_lookup_sndr_sound_paths",
        lambda _cue, *, game_id: ["Sound/FX/safe.wav"],
    )

    resolved = resolve_sound_cue("DRScSafeContainerOpen", app, game_id="fo4")

    assert resolved.path is not None
    assert resolved.path.read_bytes() == b"xwm"
    assert resolved.error == ""


def test_resolve_sound_cue_uses_open_nif_relative_data_dir(tmp_path, monkeypatch):
    from ui.editor import sound_events

    clear_sound_resolution_cache()
    data_dir = tmp_path / "Data"
    nif_path = data_dir / "Meshes" / "SetDressing" / "Safe" / "safe.nif"
    sound_file = data_dir / "Sound" / "FX" / "safe.xwm"
    nif_path.parent.mkdir(parents=True)
    nif_path.write_bytes(b"nif")
    sound_file.parent.mkdir(parents=True)
    sound_file.write_bytes(b"xwm")
    app = SimpleNamespace(nif_path=str(nif_path))

    monkeypatch.setattr(
        sound_events,
        "_lookup_sndr_sound_paths",
        lambda _cue, *, game_id: ["Sound/FX/safe.wav"],
    )

    resolved = resolve_sound_cue("DRScSafeContainerOpen", app, game_id="fo4")

    assert resolved.path == sound_file
    assert resolved.error == ""


def test_resolve_sound_cue_caches_lookup_for_same_context(tmp_path, monkeypatch):
    from ui.editor import sound_events

    clear_sound_resolution_cache()
    sound_file = tmp_path / "Sound" / "FX" / "safe.xwm"
    sound_file.parent.mkdir(parents=True)
    sound_file.write_bytes(b"xwm")
    app = SimpleNamespace(nif_path=str(tmp_path / "Meshes" / "safe.nif"))
    calls = {"sndr": 0, "loose": 0}

    def lookup(_cue, *, game_id):
        calls["sndr"] += 1
        return ["Sound/FX/safe.wav"]

    def resolve(_dirs, _path):
        calls["loose"] += 1
        return sound_file

    monkeypatch.setattr(sound_events, "_lookup_sndr_sound_paths", lookup)
    monkeypatch.setattr(sound_events, "_resolve_audio_path_in_dirs", resolve)

    first = resolve_sound_cue("DRScSafeContainerOpen", app, game_id="fo4")
    second = resolve_sound_cue("DRScSafeContainerOpen", app, game_id="fo4")

    assert first == second
    assert calls == {"sndr": 1, "loose": 1}


def test_preview_wav_conversion_is_cached_for_same_source(tmp_path, monkeypatch):
    from ui.editor import sound_events

    clear_sound_resolution_cache()
    source = tmp_path / "safe.xwm"
    source.write_bytes(b"xwm")
    calls = []

    def fake_to_wav(input_path, output_dir):
        calls.append((input_path, output_dir))
        wav_path = tmp_path / "safe.wav"
        wav_path.write_bytes(b"wav")
        return str(wav_path)

    monkeypatch.setattr("ui.voice_changer.format_converter.to_wav", fake_to_wav)

    first = sound_events._to_wav_for_preview(source)
    second = sound_events._to_wav_for_preview(source)

    assert first == second
    assert len(calls) == 1
    assert calls[0][0] == str(source)


def test_animation_manager_parses_sequence_sound_events():
    seq = _Block(4, "NiControllerSequence", **{"Text Keys": 11})
    text_keys = _Block(
        11,
        "NiTextKeyExtraData",
        **{
            "Text Keys": [
                {"Time": 0.0, "Value": "start"},
                {"Time": 0.01, "Value": "Sound: DRScSafeContainerOpen"},
                {"Time": 1.0, "Value": "end"},
            ],
        },
    )
    manager = AnimationManager()

    events = manager._parse_sequence_sound_events(_Nif([seq, text_keys]), seq)

    assert [(event.time, event.cue) for event in events] == [
        (0.01, "DRScSafeContainerOpen")
    ]


def test_animation_manager_sound_mute_suppresses_events():
    manager = AnimationManager()
    fired = []
    manager.set_sound_callback(fired.append)
    manager._current_seq = AnimSequence(
        name="Open",
        start_time=0.0,
        stop_time=1.0,
        cycle_type=2,
        sound_events=[SoundEvent(0.5, "Cue", "Sound: Cue")],
    )

    manager._trigger_sound_events(0.0, 1.0, wrapped=False)
    assert [event.cue for event in fired] == ["Cue"]

    fired.clear()
    manager.sound_muted = True
    manager._trigger_sound_events(0.0, 1.0, wrapped=False)
    assert fired == []


def test_animation_editor_writes_sound_events_back():
    text_keys = _Block(
        11,
        "NiTextKeyExtraData",
        **{
            "Num Text Keys": 3,
            "Text Keys": [
                {"Time": 0.0, "Value": "start"},
                {"Time": 0.01, "Value": "Sound: OldCue"},
                {"Time": 1.0, "Value": "end"},
            ],
        },
    )
    nif = _Nif([text_keys])
    undo = SimpleNamespace(push=lambda *args, **kwargs: None)
    anim_manager = SimpleNamespace(scan=lambda _nif: None)
    registry = SimpleNamespace(
        active_id="main",
        active_session=SimpleNamespace(anim_manager=anim_manager),
    )
    app = SimpleNamespace(nif=nif, undo_manager=undo, registry=registry)
    panel = AnimationEditorPanel(app)
    panel._sequence = EditableSequence(
        name="Open",
        block_id=4,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=2,
        text_keys_block_id=11,
        sound_events=[EditableSoundEvent(0.02, "NewCue")],
    )
    panel._load_sequence_from_nif = lambda _nif, _block_id: panel._sequence

    panel._write_back_sound_events()

    assert text_keys.get_field("Num Text Keys") == 3
    assert text_keys.get_field("Text Keys") == [
        {"Time": 0.0, "Value": "start"},
        {"Time": 0.02, "Value": "Sound: NewCue"},
        {"Time": 1.0, "Value": "end"},
    ]


def test_animation_editor_writes_bool_data_back():
    bool_data = _Block(
        31,
        "NiBoolData",
        **{"Data": {"Keys": [{"Time": 0.0, "Value": False}, {"Time": 1.0, "Value": True}]}},
    )
    nif = _Nif([bool_data])
    undo = SimpleNamespace(push=lambda *args, **kwargs: None)
    anim_manager = SimpleNamespace(scan=lambda _nif: None)
    registry = SimpleNamespace(
        active_id="main",
        active_session=SimpleNamespace(anim_manager=anim_manager),
    )
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif, undo_manager=undo, registry=registry))
    panel._sequence = EditableSequence(
        name="Visibility",
        block_id=4,
        start_time=0.0,
        stop_time=1.0,
        cycle_type=2,
        channels=[
            EditableChannel(
                label="Node : Visibility",
                node_name="Node",
                component="bool",
                keys=[EditableKey(0.0, 1.0), EditableKey(1.0, 0.0)],
                data_block_id=31,
            )
        ],
    )

    panel._write_back_channel(0)

    assert bool_data.get_field("Data")["Keys"] == [
        {"Time": 0.0, "Value": True, "Interpolation": 1},
        {"Time": 1.0, "Value": False, "Interpolation": 1},
    ]


def test_animation_editor_preserves_shader_float_controller_metadata():
    float_data = _Block(
        9,
        "NiFloatData",
        **{
            "Data": {
                "Keys": [
                    {"Time": 0.0, "Value": 0.0},
                    {"Time": 1.0, "Value": 9.0},
                ]
            }
        },
    )
    interp = _Block(8, "NiFloatInterpolator", **{"Data": 9})
    controller = _Block(
        10,
        "BSEffectShaderPropertyFloatController",
        **{"Controlled Variable": "V Offset"},
    )
    nif = _Nif([float_data, interp, controller])
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))

    channels = panel._parse_controlled_block_to_channels(
        nif,
        {
            "Interpolator": 8,
            "Controller": 10,
            "Node Name": "Plane032:0",
            "Property Type": "BSEffectShaderProperty",
            "Controller Type": "BSEffectShaderPropertyFloatController",
            "Controller ID": "8",
        },
    )

    assert len(channels) == 1
    assert channels[0].target_property == "V Offset"
    assert channels[0].controller_type == "BSEffectShaderPropertyFloatController"
    assert channels[0].property_type == "BSEffectShaderProperty"
    assert channels[0].label == "Plane032:0 : V Offset"


def test_animation_editor_parses_point3_color_channels():
    pos_data = _Block(
        27,
        "NiPosData",
        **{
            "Data": {
                "Keys": [
                    {"Time": 0.0, "Value": {"x": 1.0, "y": 0.5, "z": 0.25}},
                    {"Time": 1.0, "Value": {"x": 0.2, "y": 0.3, "z": 0.4}},
                ]
            }
        },
    )
    interp = _Block(26, "NiPoint3Interpolator", **{"Data": 27})
    controller = _Block(
        28,
        "BSEffectShaderPropertyColorController",
        **{"Controlled Color": "Emissive Color"},
    )
    nif = _Nif([pos_data, interp, controller])
    panel = AnimationEditorPanel(SimpleNamespace(nif=nif))

    channels = panel._parse_controlled_block_to_channels(
        nif,
        {
            "Interpolator": 26,
            "Controller": 28,
            "Node Name": "RadsCountTens:0",
            "Property Type": "BSEffectShaderProperty",
            "Controller Type": "BSEffectShaderPropertyColorController",
            "Controller ID": "0",
        },
    )

    assert [channel.component for channel in channels] == ["color_r", "color_g", "color_b"]
    assert [channel.target_property for channel in channels] == ["Emissive Color"] * 3
    assert [channel.keys[0].value for channel in channels] == [1.0, 0.5, 0.25]


def test_animation_manager_prefers_controller_ref_type_over_stale_metadata():
    float_data = _Block(
        9,
        "NiFloatData",
        **{"Data": {"Keys": [{"Time": 0.0, "Value": 0.0}]}},
    )
    interp = _Block(8, "NiFloatInterpolator", **{"Data": 9})
    controller = _Block(
        10,
        "BSLightingShaderPropertyFloatController",
        **{"Controlled Variable": "U Offset"},
    )
    nif = _Nif([float_data, interp, controller])
    manager = AnimationManager()

    channel = manager._parse_controlled_block(
        nif,
        {
            "Interpolator": 8,
            "Controller": 10,
            "Node Name": "LightPlane:0",
            "Property Type": "BSLightingShaderProperty",
            "Controller Type": "BSEffectShaderPropertyFloatController",
            "Controller ID": "6",
        },
    )

    assert channel.material_var == "U Offset"
