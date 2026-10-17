import json
import struct
from pathlib import Path

import numpy as np
import pytest
from bacup_lib import item_icons
from bacup_lib.item_icons import (CATALOG, ENCODING, HIDDEN, KEYCARD, REFRACTION, SIZE, GameData, Item, _item,
                                  alpha_tested_triangles, decoded, encoded, generate_item_icons, icon_flags,
                                  icon_model, item_icon, master_strings, model_file, view_basis, visible_triangles,
                                  worn_items)


def box(width, depth, height, x=0.0, y=0.0, z=0.0):
    """Closed box of 12 triangles: width along X, depth along Y, height along Z."""
    corners = np.array([[x + i * width, y + j * depth, z + k * height] for i in (0, 1) for j in (0, 1) for k in (0, 1)])
    faces = [(0, 1, 3), (0, 3, 2), (4, 6, 7), (4, 7, 5), (0, 4, 5), (0, 5, 1),
             (2, 3, 7), (2, 7, 6), (0, 2, 6), (0, 6, 4), (1, 5, 7), (1, 7, 3)]
    return corners[np.array(faces)].astype(float)


def glasses():
    """A wide front frame facing Y with two long thin temples running back along Y."""
    frame = box(14, .3, 4)
    temples = [box(.3, 14, .3, x=0, y=.3, z=3), box(.3, 14, .3, x=13.7, y=.3, z=3)]
    return np.concatenate([frame, *temples])


@pytest.mark.parametrize(("triangles", "signature", "view"), [
    (box(10, 6, 1), "MISC", "top"),
    (box(2, 2, 6), "MISC", "side"),
    (box(20, 1, 1), "MISC", "side-diagonal"),
    (box(20, 2, .2), "MISC", "top-diagonal"),
    (box(20, 1, 1), "ARMO", "front"),
    (box(20, 2, 6), "WEAP", "diagonal"),
    (box(20, 4, .5), "WEAP", "top-diagonal"),
    # Aid and consumables turn 45 degrees like weapons, from whichever view shows them best.
    (box(2, 2, 6), "ALCH", "side-diagonal"),
    (box(6, 3, 4), "ALCH", "side-diagonal"),
    (box(10, 6, 1), "ALCH", "top-diagonal"),
    (box(20, 1, 1), "ALCH", "side-diagonal"),
    (glasses(), "ARMO", "front"),
    (box(10, 6, 1), "ARMO", "front"),
])
def test_view_rule_per_shape(triangles, signature, view):
    assert view_basis(triangles, signature)[0] == view


@pytest.mark.parametrize(("triangles", "axis"), [(box(2, 2, 6), 2), (box(6, 3, 4), 0), (box(10, 6, 1), 0)])
def test_aid_runs_bottom_left_to_top_right_like_weapons(triangles, axis):
    # The item's longest drawn axis points up and to the right on screen; a standing bottle leans its top that way.
    _, basis = view_basis(triangles, "ALCH")
    assert basis[0][axis] > .5 and basis[2][axis] > .5, basis
    assert np.allclose(basis @ basis.T, np.eye(3))


def test_apparel_faces_the_camera_like_power_armor():
    # A camera in front of the wearer (+y) sees the wearer's right (+x) on its left, level, model up kept up.
    _, front = view_basis(glasses(), "ARMO")
    assert np.allclose(front, [[-1, 0, 0], [0, 1, 0], [0, 0, 1]])


def item(signature, model, **fields):
    return Item("000801:Fallout4.esm", signature, "Thing", "Thing", model, "Fallout4.esm", True, **fields)


@pytest.mark.parametrize(("signature", "model", "drawn"), [
    # Ammo in a generic cartridge box keeps the ammo glyph; distinctive ammo is drawn.
    ("AMMO", "ammo/10mm/10mmammo.nif", None),
    ("AMMO", "ammo/shotgunshell/shotgunshellammo.nif", None),
    ("AMMO", "dlc03/ammo/4570ammo.nif", None),
    ("AMMO", "ammo/fusioncore/fusioncore.nif", "ammo/fusioncore/fusioncore.nif"),
    # Books, notes and holotapes keep their glyphs, except objects that are things in their own right.
    ("BOOK", "props/note_lowpoly.nif", None),
    ("BOOK", "props/instr03blueprint.nif", None),
    ("NOTE", "props/holotape_prop.nif", None),
    ("BOOK", "props/transceiverobject/transceiverobjectsugargrove.nif",
     "props/transceiverobject/transceiverobjectsugargrove.nif"),
    ("BOOK", "setdressing/kitchen/beerbottle_message.nif", "setdressing/kitchen/beerbottle_message.nif"),
    # Every keycard shares one keycard icon; keys are drawn; passwords on paper or holotape keep the glyph.
    ("KEYM", "props/vaultidcard.nif", KEYCARD),
    ("KEYM", "props/generickeycard01_flat04.nif", KEYCARD),
    ("KEYM", "interface/note/lowpoly_government_id.nif", KEYCARD),
    ("KEYM", "props/key01.nif", "props/key01.nif"),
    ("KEYM", "props/holotape_prop.nif", None),
    ("KEYM", "props/note_lowpoly.nif", None),
    ("MISC", "props/bobbypin.nif", "props/bobbypin.nif"),
])
def test_icon_model_rules(signature, model, drawn):
    assert icon_model(item(signature, model)) == drawn


def test_worn_outfit_draws_its_human_addons_instead_of_the_folded_ground_model():
    outfit = item("ARMO", "clothes/suit_go.nif", addons=("000A01:Fallout4.esm", "000A02:Fallout4.esm",
                                                         "000A03:Fallout4.esm"))
    hat = item("ARMO", "clothes/hat_go.nif", addons=())
    addons = {"000A01:Fallout4.esm": (item_icons.HUMAN_RACE, "clothes/suitm.nif"),
              "000A02:Fallout4.esm": (item_icons.HUMAN_RACE, "clothes/glovesm.nif"),
              "000A03:Fallout4.esm": ("01A009:Fallout4.esm", "clothes/suitmutant.nif")}
    worn_outfit, worn_hat = worn_items([outfit, hat], addons)
    assert worn_outfit.worn == ("clothes/glovesm.nif", "clothes/suitm.nif")
    assert icon_model(worn_outfit) == "clothes/glovesm.nif|clothes/suitm.nif"
    assert worn_hat.worn == () and icon_model(worn_hat) == "clothes/hat_go.nif"


def test_armo_record_keeps_addons_only_for_body_outfits():
    def armor(slots):
        return {"eid": "Suit", "flags": "0", "fields": [
            {"FULL": "Suit"}, {"Males": [{"MOD2": "Clothes\\Suit_GO.nif"}]}, {"BipedBodyTemplate": slots},
            {"ArmorAddon": {"reference": {"plugin": "Fallout4.esm", "object_id": "000A01"}}}]}
    outfit = _item(0x00000801, armor(["33BODY", "34LHand"]), "ARMO", "Fallout4.esm", [])
    helmet = _item(0x00000802, armor(["46Headband"]), "ARMO", "Fallout4.esm", [])
    assert outfit.model == "clothes/suit_go.nif" and outfit.addons == ("000A01:Fallout4.esm",)
    assert helmet.addons == ()


def test_alpha_test_cuts_a_card_along_its_texture():
    # One quad spanning the whole texture; only its left half is opaque.
    points = np.array([[0, 0, 0], [10, 0, 0], [10, 0, 10], [0, 0, 10]], float)
    uvs = np.array([[0, 1], [1, 1], [1, 0], [0, 0]], float)
    faces = np.array([[0, 1, 2], [0, 2, 3]])
    texture = np.zeros((64, 64), np.uint8)
    texture[:, :32] = 255
    kept = alpha_tested_triangles(points, uvs, faces, texture, np.full(4, 255.0), 128, pixel=.01)
    area = np.linalg.norm(np.cross(kept[:, 1] - kept[:, 0], kept[:, 2] - kept[:, 0]), axis=1).sum() / 2
    assert 40 < area < 60 and kept[:, :, 0].max() <= 5.5
    # Vertex alpha multiplies texture alpha, as FO4's alpha test does.
    assert len(alpha_tested_triangles(points, uvs, faces, texture, np.full(4, 100.0), 128, pixel=.01)) == 0
    # No piece is cut finer than half an icon pixel (fur and hair cards made millions of pieces): at a 5-unit
    # pixel the 14-unit diagonal splits 6 ways instead of the texture's 23.
    coarse = alpha_tested_triangles(points, uvs, faces, texture, np.full(4, 255.0), 128, pixel=5)
    assert 0 < len(coarse) <= 2 * 6 * 6


def test_alpha_cut_is_repeatable_for_wrapped_uvs():
    rng = np.random.default_rng(3)
    points, uvs = rng.random((40, 3)) * 10, rng.random((40, 2)) * 2
    faces = rng.integers(0, 40, (60, 3))
    texture = (rng.random((32, 32)) > .5).astype(np.uint8) * 255
    whole = alpha_tested_triangles(points, uvs, faces, texture, np.full(40, 255.0), 128, pixel=.05)
    assert np.array_equal(alpha_tested_triangles(points, uvs, faces, texture, np.full(40, 255.0), 128, pixel=.05), whole)


def test_srgb_block_textures_read_their_alpha(tmp_path):
    # A 4x4 BC1_UNORM_SRGB (DXGI 72) texture, Pillow has no decoder for it: left half opaque, right half cut out.
    header = (struct.pack("<4s7I44x", b"DDS ", 124, 0x81007, 4, 4, 8, 0, 1)
              + struct.pack("<2I4s5I", 32, 4, b"DX10", 0, 0, 0, 0, 0) + struct.pack("<5I", 0x1000, 0, 0, 0, 0)
              + struct.pack("<5I", 72, 3, 0, 1, 0))
    (tmp_path / "textures").mkdir()
    (tmp_path / "textures" / "card.dds").write_bytes(header + struct.pack("<2H4B", 0, 0xFFFF, 0xF0, 0xF0, 0xF0, 0xF0))
    alpha = item_icons._texture_alpha("textures/card.dds", GameData((str(tmp_path),)))
    assert alpha.tolist() == [[255, 255, 0, 0]] * 4


def test_loose_data_wins_in_priority_order_and_master_strings_are_copied(tmp_path):
    first, second = tmp_path / "mod", tmp_path / "game"
    for root, text in ((first, b"mod"), (second, b"game")):
        (root / "strings").mkdir(parents=True)
        (root / "strings" / "Fallout4_en.strings").write_bytes(text)
    (second / "strings" / "Fallout4_en.dlstrings").write_bytes(b"descriptions")
    (second / "meshes" / "static").mkdir(parents=True)
    (second / "meshes" / "static" / "lamp.nif").touch()
    data = GameData((str(first), str(second)))
    assert data.read("strings/Fallout4_en.strings") == b"mod" and data.read("strings/missing.strings") is None
    copied = master_strings(data, [Path("Data/Fallout4.esm")], tmp_path / "out")
    assert sorted(path.name for path in copied.iterdir()) == ["Fallout4_en.dlstrings", "Fallout4_en.strings"]
    # A static copy BACUP has not written yet falls back to the source model.
    assert model_file(data, "bacup_static/static/lamp.nif") == "meshes/static/lamp.nif"
    assert model_file(data, "static/missing.nif") is None


def test_top_view_looks_down_and_side_view_is_raised():
    _, top = view_basis(box(10, 6, 1), "MISC")
    assert np.allclose(top[1], [0, 0, 1])
    _, side = view_basis(box(2, 3, 6), "MISC")
    # Twenty degrees above the side: model up still points mostly up the screen.
    assert np.isclose(side[2][2], np.cos(np.radians(item_icons.ELEVATION)))
    assert np.allclose(side @ side.T, np.eye(3))


def test_stick_diagonal_puts_the_long_axis_on_the_square_diagonal():
    icon = item_icon(box(20, 1, 1), "MISC")[1]
    ys, xs = np.nonzero(icon)
    assert abs(np.corrcoef(xs, ys)[0, 1]) > .95


class Block:
    def __init__(self, type_name, **fields):
        self.type_name, self.fields = type_name, fields

    def get_field(self, name):
        return self.fields.get(name)

    def to_json(self):
        return self.fields


class Nif:
    def __init__(self, blocks):
        self.blocks = blocks

    def get_block(self, index):
        return self.blocks[index] if 0 <= index < len(self.blocks) else None


def shape(name, shader=-1, controller=-1, flags=0, z=0.0):
    vertices = [{"Vertex": {"x": 0, "y": 0, "z": z}}, {"Vertex": {"x": 1, "y": 0, "z": z}},
                {"Vertex": {"x": 0, "y": 1, "z": z}}]
    return Block("BSTriShape", Name=name, Flags=flags, Controller=controller, **{
        "Shader Property": shader, "Vertex Data": vertices, "Triangles": [{"v1": 0, "v2": 1, "v3": 2}]})


def effect_model():
    blocks = [Block("NiNode", Name="root", Children=list(range(1, 11)))]
    blocks += [shape("body", z=0),                                         # 1 drawn
               shape("hidden", flags=HIDDEN),                              # 2
               Block("NiBillboardNode", Name="glow card", Children=[17]),  # 3
               shape("EditorMarker01"),                                    # 4
               shape("BaseRefractionMesh", shader=13),                     # 5
               shape("flame", shader=14),                                  # 6 animated by its shader
               shape("glass", shader=15, z=1),                             # 7 still effect: drawn
               Block("NiSwitchNode", Name="state", Index=1, Children=[18, 19]),  # 8
               Block("NiLODNode", Name="lod", Children=[20, 21]),          # 9
               shape("sparks", shader=16, controller=22)]                  # 10 animated by the shape
    blocks += [Block("NiStringExtraData"), Block("NiStringExtraData"),     # 11, 12
               Block("BSLightingShaderProperty", **{"Shader Flags 1": REFRACTION}),  # 13
               Block("BSEffectShaderProperty", Controller=22),             # 14
               Block("BSEffectShaderProperty", Controller=-1),             # 15
               Block("BSEffectShaderProperty", Controller=-1)]             # 16
    blocks += [shape("billboard child"), shape("switch off"), shape("switch on", z=2),
               shape("lod near", z=3), shape("lod far"), Block("NiFloatInterpController")]
    return Nif(blocks)


def test_visible_triangles_skips_hidden_marker_refraction_and_animated_effects():
    triangles, skipped = visible_triangles(effect_model())
    assert dict(skipped) == {"hidden": "hidden", "glow card": "billboard", "EditorMarker01": "editor marker",
                             "BaseRefractionMesh": "refraction", "flame": "animated effect",
                             "sparks": "animated effect"}
    # Body, still glass, the switch's active child and the LOD's first child.
    assert sorted(triangles[:, 0, 2]) == [0, 1, 2, 3]


def test_alpha_encoding_round_trips_losslessly():
    alpha = np.random.default_rng(7).integers(0, 256, (SIZE, SIZE), dtype=np.uint8)
    text = encoded(alpha)
    assert text.isascii()
    assert np.array_equal(decoded(text), alpha)


def test_render_is_deterministic():
    model = np.concatenate([box(4, 3, 8), box(1, 1, 3, x=4, z=2)])
    first, second = item_icon(model, "MISC"), item_icon(model.copy(), "MISC")
    assert first[0] == second[0] and np.array_equal(first[1], second[1])


def record(*, animation=None, keywords=()):
    fields = [{"FULL": "Thing"}, {"MODL": "Weapons\\Thing.nif"},
              {"Keywords": [{"reference": {"plugin": plugin, "object_id": object_id}} for object_id, plugin in keywords]}]
    if animation:
        fields.append({"Data": {"AnimationType": animation}})
    return {"eid": "Thing", "flags": "0", "fields": fields}


@pytest.mark.parametrize(("animation", "keywords", "thrown"), [
    ("Grenade", (), True), ("Mine", (), True), ("Gun", [("10C414", "Fallout4.esm")], True),
    ("Gun", [("10C415", "Fallout4.esm")], True), ("Gun", [("10C416", "Fallout4.esm")], False), (None, (), False)])
def test_grenades_and_mines_are_thrown(animation, keywords, thrown):
    item = _item(0x01000801, record(animation=animation, keywords=keywords), "WEAP", "Mod.esp", ["Fallout4.esm"])
    assert item.thrown is thrown
    assert item.form == "000801:Mod.esp" and item.model == "weapons/thing.nif"


def test_catalog_shares_icons_and_keeps_weapon_fallbacks_apart(tmp_path):
    from creation_lib.nif.nif_file import NifFile
    shapes = {"a.nif": box(10, 6, 1), "copy/a.nif": box(10, 6, 1), "can.nif": box(2, 2, 6), "gun.nif": box(20, 2, 6)}
    for name in shapes:
        (tmp_path / "meshes" / name).parent.mkdir(parents=True, exist_ok=True)
        triangles = shapes[name]
        nif = NifFile.new()
        shape = nif.add_block("BSTriShape", {
            "Flags": 0, "Vertex Desc": (0x401 << 44) | 4,
            "Num Vertices": len(triangles) * 3, "Num Triangles": len(triangles),
            "Data Size": len(triangles) * (3 * 16 + 6),
            "Vertex Data": [{"Vertex": dict(zip("xyz", point))} for point in triangles.reshape(-1, 3)],
            "Triangles": [dict(zip(("v1", "v2", "v3"), range(index * 3, index * 3 + 3)))
                          for index in range(len(triangles))],
        })
        nif.get_block(0).set_field("Children", [shape.block_id])
        nif.get_block(0).set_field("Num Children", 1)
        nif.save(str(tmp_path / "meshes" / name))
    items = [Item("000801:Fallout4.esm", "MISC", "A", "A", "a.nif", "Fallout4.esm", True),
             Item("000802:Fallout4.esm", "MISC", "B", "B", "copy/a.nif", "Fallout4.esm", True),
             Item("000803:SeventySix.esm", "ALCH", "C", "C", "can.nif", "SeventySix.esm", True),
             Item("000804:Fallout4.esm", "WEAP", "Gun", "Test Gun", "gun.nif", "Fallout4.esm", True),
             Item("000805:Fallout4.esm", "WEAP", "Mine", "Mine", "gun.nif", "Fallout4.esm", True, thrown=True),
             Item("000806:Fallout4.esm", "MISC", "Gone", "Gone", "missing.nif", "Fallout4.esm", True),
             Item("000807:Fallout4.esm", "MISC", "", "", "a.nif", "Fallout4.esm", True),
             Item("000808:Fallout4.esm", "BOOK", "Note", "Note", "a.nif", "Fallout4.esm", True)]
    def record_bytes(signature, form_id, fields):
        body = b"".join(tag.encode() + struct.pack("<H", len(value)) + value for tag, value in fields)
        return struct.pack("<4sIIIIHH", signature.encode(), len(body), 0, form_id, 0, 131, 0) + body

    plugins = []
    for plugin in ("Fallout4.esm", "SeventySix.esm"):
        records = []
        for entry in items:
            if entry.plugin != plugin:
                continue
            fields = [("EDID", (entry.editor_id + "\0").encode()), ("FULL", (entry.name + "\0").encode()),
                      ("MODL", (entry.model + "\0").encode())]
            if entry.thrown:
                fields += [("KSIZ", struct.pack("<I", 1)), ("KWDA", struct.pack("<I", 0x10C414))]
            records.append((entry.signature, record_bytes(entry.signature, int(entry.form.split(":")[0], 16), fields)))
        header = record_bytes("TES4", 0, [("HEDR", struct.pack("<fII", 1.0, len(records), 0x900))])
        groups = [struct.pack("<4sI4sI8s", b"GRUP", 24 + len(record), signature.encode(), 0, b"\0" * 8) + record
                  for signature, record in records]
        path = tmp_path / plugin
        path.write_bytes(header + b"".join(groups))
        plugins.append((path, None))

    def build(root):
        path = generate_item_icons(plugins, GameData((str(tmp_path),)), root, workers=1, report=root / "report.json")
        return path, path.read_bytes()

    path, first = build(tmp_path / "one")
    assert path == tmp_path / "one" / CATALOG
    assert build(tmp_path / "two")[1] == first
    catalog = json.loads(first)
    assert {key: catalog[key] for key in ("version", "width", "height", "encoding")} == {
        "version": 1, "width": SIZE, "height": SIZE, "encoding": ENCODING}
    forms, fallbacks = catalog["forms"], catalog["fallback_forms"]
    assert forms["Fallout4.esm"]["000801"] == forms["Fallout4.esm"]["000802"]
    assert set(forms) == {"Fallout4.esm", "SeventySix.esm"}
    assert set(forms["Fallout4.esm"]) == {"000801", "000802", "000805"}
    assert fallbacks == {"Fallout4.esm": {"000804": forms["Fallout4.esm"]["000805"]}}
    assert len(catalog["icons"]) == 3
    can = catalog["icons"][forms["SeventySix.esm"]["000803"]]
    assert can["view"] == "side-diagonal" and np.array_equal(decoded(can["alpha"]), item_icon(shapes["can.nif"], "ALCH")[1])
    report = {entry["form"]: entry for entry in json.loads((tmp_path / "one" / "report.json").read_text("utf-8"))}
    assert report["000806:Fallout4.esm"]["error"] == "missing model" and report["000806:Fallout4.esm"]["icon"] is None
    assert report["000808:Fallout4.esm"]["error"] == "category glyph" and "000808" not in forms["Fallout4.esm"]


def test_review_flags():
    assert icon_flags(np.zeros((SIZE, SIZE), np.uint8)) == ["empty"]
    assert icon_flags(item_icon(box(4, 3, 4), "MISC")[1]) == []
    assert "thin" in icon_flags(item_icon(box(40, .5, .5), "MISC")[1])
    scattered = np.zeros((SIZE, SIZE), np.uint8)
    scattered[60:70, 100:110] = 255
    scattered[5, 5] = 255
    assert "scattered" in icon_flags(scattered)
