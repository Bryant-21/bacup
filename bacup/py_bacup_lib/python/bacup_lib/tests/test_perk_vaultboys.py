import json
import struct
import zlib

from creation_lib.swf import native_runtime
from creation_lib.swf.shapes import EndShape, ShapeDef, StraightEdge, StyleChange
from creation_lib.swf.tags import DefineShapeTag, PlaceObject2Tag
from creation_lib.swf.types import RGBA, FillStyle

from bacup_lib import perk_vaultboys as vb
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.tests.test_legendary_perks_ui import swf


def short_tag(code, payload=b""):
    return struct.pack("<H", code << 6 | len(payload)) + payload


def long_tag(code, payload):
    return struct.pack("<HI", code << 6 | 63, len(payload)) + payload


def box(shape_id, width, height):
    shape = ShapeDef(shape_id=shape_id, bounds=(0, 0, width * 20, height * 20),
                     fill_styles=[FillStyle(fill_type=0, color=RGBA(255, 0, 0, 255))], line_styles=[],
                     records=[StyleChange(move_x=0, move_y=0, fill1=1), StraightEdge(width * 20, 0),
                              StraightEdge(0, height * 20), StraightEdge(-width * 20, 0),
                              StraightEdge(0, -height * 20), EndShape()], shape_version=3)
    return DefineShapeTag(shape=shape, tag_id=32).to_bytes()


def timeline(character, frames, *entries):
    return struct.pack("<HH", character, frames) + b"".join(entries) + short_tag(0)


def place(depth, character):
    return long_tag(26, PlaceObject2Tag(depth=depth, character_id=character).to_bytes())


def symbols(*rows):
    return struct.pack("<H", len(rows)) + b"".join(struct.pack("<H", cid) + name.encode() + b"\0" for cid, name in rows)


def library(tmp_path):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    art = timeline(20, 3, place(1, 10), short_tag(1), short_tag(1), short_tag(1))
    perk = timeline(30, 2, long_tag(43, b"static\0"), place(1, 11), short_tag(1),
                    long_tag(43, b"animated\0"), struct.pack("<H", 28 << 6 | 2) + struct.pack("<H", 1),
                    place(2, 20), short_tag(1))
    still = timeline(31, 2, long_tag(43, b"static\0"), place(1, 11), short_tag(1),
                     long_tag(43, b"animated\0"), short_tag(1))
    card = timeline(32, 1, place(1, 11), short_tag(1))
    tags = [(69, b"\x08\0\0\0"), (32, box(10, 100, 200)), (32, box(11, 40, 40)), (39, art), (39, timeline(21, 1, short_tag(1))),
            (39, perk), (39, still), (39, card), (82, b"FO76 classes"),
            (76, symbols((20, "PerksLibrary_Small_fla.NerdRageAnimated_760"), (30, "NerdRage"), (31, "GlowingOne"),
                         (32, "CardFrame"), (21, "Shared.AS3.Helper"))), (1, b"")]
    (interface / vb.LIBRARY).write_bytes(swf(tags))
    return interface.parent


def test_clip_names_follow_fo76_hud_lookup():
    assert vb.clip_name("Components/VaultBoys/Perks/Robotics Expert.swf") == "RoboticsExpert"
    assert vb.clip_name("Components\\VaultBoys\\Perks\\NerdRage.SWF") == "NerdRage"
    assert vb.clip_name("Scrounger") == "Scrounger"


def test_animated_frame_uses_the_display_list(tmp_path):
    tags = swf_tags((library(tmp_path) / "interface" / vb.LIBRARY).read_bytes())
    definitions = vb._definitions(tags)
    assert vb.animated_clip(definitions, 30) == 20
    # Only a sprite on the animated frame is a clip; static shapes and plain card art are skipped.
    assert vb.animated_clip(definitions, 31) is None
    assert vb.animated_clip(definitions, 32) is None


def test_clip_becomes_a_standalone_fo4_hud_movie(tmp_path):
    output = tmp_path / "output"
    manifest = vb.convert_perk_vaultboys(library(tmp_path), output)
    assert [item["class"] for item in manifest["files"]] == ["NerdRage"]
    data = (output / vb.OUTPUT / "NerdRage.swf").read_bytes()
    assert data[:3] == b"CWS"
    body = zlib.decompress(data[8:])
    rect = (5 + 4 * (body[0] >> 3) + 7) // 8
    assert struct.unpack_from("<H", body, rect + 2)[0] == 3
    tags = swf_tags(data)
    assert [code for code, _ in tags][0] == 69
    assert vb.symbol_ids(tags) == {vb.ROOT_CLASS: 0}
    assert native_runtime.abc_class_names(data) == [vb.ROOT_CLASS]
    assert native_runtime.unbacked_symbol_classes(data) == []
    # Only the art closure ships: no FO76 classes, card art or helper clips.
    defined = sorted(struct.unpack_from("<H", payload)[0] for code, payload in tags if code in (32, 39))
    assert defined == [10, 20, 33]
    assert sum(code == 1 for code, _ in tags) == 3
    placed = [PlaceObject2Tag.parse(payload) for code, payload in tags if code == 26]
    assert [(item.depth, item.character_id, item.name) for item in placed] == [(1, 33, None), (2, 20, vb.ART_NAME)]
    # The 100x200 art is scaled to 90% of the stage height and centred on (275, 200).
    matrix = placed[1].matrix
    assert abs(matrix.scale_x - 1.8) < 1e-3
    assert abs(matrix.translate_x / 20 + 1.8 * 50 - 275) < 0.1
    assert abs(matrix.translate_y / 20 + 1.8 * 100 - 200) < 0.1
    frame = native_runtime.render_frame_image(data, 1, 0)
    assert frame["bounds"] == [0, 0, 550, 400]
    assert not frame["warnings"]


def test_generated_class_file_wins_over_a_spaced_loose_copy(tmp_path):
    folder = tmp_path / vb.OUTPUT
    folder.mkdir(parents=True)
    for name in ("robotics expert.swf", "RoboticsExpert.swf", "well rested.swf"):
        (folder / name).write_bytes(b"")
    assert vb.available_clips(tmp_path) == {"roboticsexpert": "RoboticsExpert.swf", "wellrested": "well rested.swf"}


def test_batched_bounds_match_rendering_with_empty_and_transformed_frames():
    from creation_lib.swf.types import MATRIX

    shifted = PlaceObject2Tag(depth=1, character_id=10, matrix=MATRIX(translate_x=-700, translate_y=300))
    remove = short_tag(28, struct.pack("<H", 1))
    art = timeline(20, 4, short_tag(1), place(1, 10), short_tag(1), remove,
                   long_tag(26, shifted.to_bytes()), short_tag(1), remove, short_tag(1))
    movie = swf([(32, box(10, 100, 200)), (39, art), (1, b"")])
    expected = []
    for frame in (1, 2, 3, 4, 2):
        try:
            expected.append(native_runtime.render_frame_image(movie, frame, 20)["bounds"])
        except ValueError:
            expected.append(None)
    assert expected[0] is None and expected[3] is None
    assert expected[1] != expected[2]
    assert native_runtime.sprite_frame_bounds(movie, 20, [1, 2, 3, 4, 2]) == expected


class Source:
    def __init__(self, records):
        self.records = records
        self.handle = 1
        self.native = self

    def plugin_handle_search_records(self, handle, query, signatures, case_sensitive):
        return [(raw, signature, eid, None) for raw, (signature, eid, _) in self.records.items()
                if signature in signatures]

    def fields(self, raw):
        return self.records[raw]

    def lookup(self, signature, eid):
        raw = next(raw for raw, (sig, found, _) in self.records.items() if (sig, found) == (signature, eid))
        return raw, self.records[raw][2]


class Resolver:
    def __init__(self, plugin, editor_ids):
        self.plugin, self.editor_ids = plugin, editor_ids

    def resolve(self, signature, eid):
        return f"{self.plugin}:{self.editor_ids[eid]:06X}" if eid in self.editor_ids else None


def reference(object_id):
    return {"reference": {"plugin": "SeventySix.esm", "object_id": f"{object_id:06X}"}}


def test_catalog_binds_perk_icons_sounds_and_script_cooldowns():
    source = Source({
        1: ("PERK", "Serendipity01", {"PerkActivationIcon": ["Components/VaultBoys/Perks/Serendipity.swf"],
                                      "PerkActivationSound": [reference(10)]}),
        2: ("PERK", "Scrounger01", {"PerkActivationIcon": ["Scrounger"]}),
        3: ("PERK", "Conductor01", {"PerkActivationIcon": ["Conductor"]}),
        4: ("PERK", "Plain", {}),
        5: ("PERK", "Dropped01", {"PerkActivationIcon": ["Scrounger"]}),
        6: ("MGEF", "PerkTeamMedicHealEffect", {"VirtualMachineAdapter": [{"Scripts": [
            {"ScriptName": "VaultBoyHUDEffectScript", "Properties": [
                {"propertyName": "VaultBoySwfName", "Value": "\"Components/VaultBoys/Perks/TeamMedic.swf\""},
                {"propertyName": "VaultBoySound", "Value": {"Alias": -1, "FormID": reference(11)}},
                {"propertyName": "Cooldown", "Value": 30.0}]}]}]}),
        10: ("SNDR", "UIPerkActivateCustomA", {}),
        11: ("SNDR", "UIPerkActivateGeneric", {}),
    })
    converted = Resolver("SeventySix.esm", {"Serendipity01": 0x330B9B, "Scrounger01": 0x04A0B0,
                                            "UIPerkActivateCustomA": 0x3A0826})
    base = Resolver("Fallout4.esm", {"UIPerkActivateGeneric": 0x389CC6})
    clips = {"serendipity": "Serendipity.swf", "scrounger": "Scrounger.swf", "teammedic": "TeamMedic.swf"}
    catalog = vb.build_catalog(source, converted, base, clips)
    assert catalog["default_sound"] == "Fallout4.esm:389CC6"
    assert catalog["clips"]["serendipity"] == {"path": "Components/VaultBoys/Perks/Serendipity.swf",
                                               "sound": "SeventySix.esm:3A0826", "cooldown": None}
    assert catalog["clips"]["teammedic"] == {"path": "Components/VaultBoys/Perks/TeamMedic.swf",
                                             "sound": "Fallout4.esm:389CC6", "cooldown": 30.0}
    assert [(item["editor_id"], item["perk"], item["source_perk"], item["clip"]) for item in catalog["perks"]] == [
        ("Dropped01", None, "000005:SeventySix.esm", "scrounger"),
        ("Scrounger01", "SeventySix.esm:04A0B0", "000002:SeventySix.esm", "scrounger"),
        ("Serendipity01", "SeventySix.esm:330B9B", "000001:SeventySix.esm", "serendipity")]
    assert catalog["missing_clips"] == ["Conductor"]
    json.dumps(catalog)
