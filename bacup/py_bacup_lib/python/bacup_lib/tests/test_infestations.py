import json
import struct
import zlib
from pathlib import Path
from types import SimpleNamespace as NS

import pytest

from bacup_lib import infestations as inf
from creation_lib.swf.types import MATRIX, BitWriter


def catalog_plugins(tmp_path, *, missing_owned_quest=False, center_data=None):
    from creation_lib.esp import Plugin
    from bacup_lib.tests.test_vmad_codec import _prop, _script, _vmad

    fixture = Path(__file__).resolve().parents[3] / "native/conversion/src/test_fixtures/infestations/catalog.json"
    data = json.loads(fixture.read_text())
    source = tmp_path / "source" / "SeventySix.esm"
    target = tmp_path / "converted" / "SeventySix.esm"
    source.parent.mkdir()
    target.parent.mkdir()

    def form(value):
        return int(value["reference"]["object_id"], 16)

    with Plugin.new(source.name, game="fo76") as plugin:
        for raw, row in data["records"].items():
            record = plugin.new_record(row["signature"], form_id=int(raw))
            record.editor_id = row["editor_id"]
            for field in row["value"]["fields"]:
                for key, value in field.items():
                    if key == "VirtualMachineAdapter":
                        scripts = [_script(script["ScriptName"], [
                            _prop(prop["propertyName"], 4, struct.pack("<f", prop["Value"]))
                            for prop in script["Properties"]]) for script in value["Scripts"]]
                        record.add_subrecord("VMAD", _vmad(scripts))
                    elif key == "Value":
                        record.add_subrecord("FLTV", struct.pack("<f", value))
                    elif key == "ParentLocation":
                        record.add_subrecord("PNAM", struct.pack("<I", form(value)))
                    elif key == "FormID":
                        record.add_subrecord("LNAM", struct.pack("<I", form(value)))
                    elif key == "MasterSpecialReferences":
                        record.add_subrecord("LCSR", b"".join(struct.pack("<IIIhh",
                            form(ref["MasterSpecialReferencesLocRefType"]),
                            form(ref["MasterSpecialReferencesRef"]),
                            form(ref["MasterSpecialReferencesWorldCell"]), 0, 0) for ref in value))
                    else:
                        raise AssertionError(key)
            plugin.add_record(record)
        plugin.save(source)
    with Plugin.new(target.name, game="fo4") as plugin:
        plugin.add_master("Fallout4.esm")
        for raw, row in data["converted"].items():
            if missing_owned_quest and int(raw) == 0x865FA8:
                continue
            record = plugin.new_record(row["signature"], form_id=0x01000000 | int(raw))
            if row["position"] is not None:
                data_bytes = center_data if center_data is not None and int(raw) == 0x700000 else struct.pack("<6f", *row["position"], 0, 0, 0)
                record.add_subrecord("DATA", data_bytes)
            if row["has_entries"]:
                record.add_subrecord("LVLO", struct.pack("<HHIHH", 1, 0, 0x1234, 1, 0))
            plugin.add_record(record)
        plugin.add_record(plugin.new_record("QUST", form_id=0x865FA8))
        plugin.save(target)
    return source, target, data["expected"]


def test_native_catalog_reads_plugins_and_matches_previous_catalog(tmp_path):
    source, target, expected = catalog_plugins(tmp_path)
    actual = inf.build_catalog(source, target, "1.7.26.13")
    assert actual == expected
    assert json.dumps(actual) == json.dumps(expected)


def test_native_catalog_does_not_accept_a_master_record_as_converted(tmp_path):
    source, target, _ = catalog_plugins(tmp_path, missing_owned_quest=True)
    with pytest.raises(ValueError, match="Converted QUST 865FA8 is missing"):
        inf.build_catalog(source, target)


@pytest.mark.parametrize("center_data", [struct.pack("<3f", 1, 2, 3), struct.pack("<6f", float("nan"), 2, 3, 0, 0, 0)])
def test_native_catalog_rejects_incomplete_or_nonfinite_centers(tmp_path, center_data):
    source, target, _ = catalog_plugins(tmp_path, center_data=center_data)
    with pytest.raises(ValueError, match="No FO76 location qualifies"):
        inf.build_catalog(source, target)


def lossless(character: int, width: int, height: int, argb: bytes) -> bytes:
    return struct.pack("<HBHH", character, 5, width, height) + zlib.compress(argb)


def decode_png(data: bytes) -> tuple[int, int, bytes]:
    assert data.startswith(b"\x89PNG\r\n\x1a\n")
    width, height = struct.unpack(">II", data[16:24])
    idat, pos = b"", 8
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        if kind == b"IDAT":
            idat += data[pos + 8:pos + 8 + length]
        pos += 12 + length
    rows = zlib.decompress(idat)
    stride = width * 4 + 1
    return width, height, b"".join(rows[y * stride + 1:(y + 1) * stride] for y in range(height))


def test_lossless_bitmap_is_unpremultiplied_to_png_and_rejects_bad_input():
    argb = bytes([128, 64, 32, 0, 0, 0, 0, 0, 255, 10, 20, 30])
    character, width, height, image = inf.lossless_png(lossless(82, 3, 1, argb))
    assert (character, width, height) == (82, 3, 1)
    assert decode_png(image) == (3, 1, bytes([128, 64, 0, 128, 0, 0, 0, 0, 10, 20, 30, 255]))
    with pytest.raises(ValueError, match="format"):
        inf.lossless_png(struct.pack("<HBHH", 1, 3, 1, 1) + zlib.compress(b"\0" * 4))
    with pytest.raises(ValueError, match="bytes"):
        inf.lossless_png(lossless(1, 2, 2, b"\0" * 8))


def matrix(scale: float) -> MATRIX:
    return MATRIX(scale_x=scale, scale_y=scale, rotate_skew_0=0.0, rotate_skew_1=0.0, translate_x=0, translate_y=0)


def place_object3(depth: int, character: int, scale: float, class_name: str | None = None) -> bytes:
    writer = BitWriter()
    writer.write_ui8(0x06)
    writer.write_ui8(0x08 if class_name else 0)
    writer.write_ui16(depth)
    if class_name:
        writer.write_string(class_name)
    writer.write_ui16(character)
    matrix(scale).write(writer)
    return writer.getvalue()


def test_place_object3_decodes_character_and_matrix():
    depth, character, placed = inf.place_object3(place_object3(3, 88, 0.5, class_name="Cloud"))
    assert (depth, character) == (3, 88)
    assert inf.matrix_scale(placed) == pytest.approx(0.5, abs=1e-3)
    assert inf.place_object3(bytes([0x01, 0, 1, 0])) is None


def sprite(frames: list[dict], count: int | None = None):
    return NS(timeline=NS(frames=[NS(placements=placements) for placements in frames], frame_count=count or len(frames)))


def placed(character: int, scale: float = 1.0, name: str | None = None):
    return NS(character_id=character, matrix=matrix(scale), name=name)


def shape(width: float, *bitmaps: int):
    return NS(bounds_px=(-width / 2, -width / 2, width / 2, width / 2),
              fill_styles=[NS(bitmap_id=bitmap) for bitmap in bitmaps])


def fog_doc():
    # 98 = MapCloud: a 200 px radius clip at scale 0.5 (radius 50) and a cloud group.
    # 90 is empty on its first frame and places 88 only through a raw PlaceObject3, as FO76's does.
    raw90 = NS(sprite_id=90, tags=[NS(tag_id=70, data=place_object3(1, 88, 2.0)), NS(tag_id=1)])
    return NS(
        symbols=[(98, inf.FOG_SYMBOL)],
        header=NS(fps=30),
        tags=[raw90],
        shapes={80: shape(200), 83: shape(100, 65535, 82), 87: shape(100, 86), 92: shape(100, 91)},
        sprites={
            98: sprite([{1: placed(81, 0.5, inf.RADIUS_CLIP), 5: placed(97)}], 2),
            81: sprite([{1: placed(80)}]),
            97: sprite([{1: placed(85), 3: placed(90), 11: placed(94)}]),
            85: sprite([{1: placed(84)}], 60),
            84: sprite([{1: placed(83)}]),
            90: sprite([{}, {1: NS(character_id=None, matrix=matrix(2.0), name=None)}], 90),
            88: sprite([{1: placed(87)}]),
            94: sprite([{}, {3: placed(92, 0.25)}], 120),
        })


def test_fog_layers_follow_the_symbol_and_require_the_radius_clip():
    layout = inf.fog_layers(fog_doc(), {82: b"", 86: b"", 91: b""})
    assert layout["radius"] == 50
    assert layout["layers"] == [
        {"bitmap": 82, "scale": 1.0, "period": 2.0},
        {"bitmap": 86, "scale": 2.0, "period": 3.0},
        {"bitmap": 91, "scale": 0.25, "period": 4.0}]

    doc = fog_doc()
    doc.symbols = []
    with pytest.raises(ValueError, match="does not export"):
        inf.fog_layers(doc, {82: b""})
    doc = fog_doc()
    doc.sprites[98] = sprite([{5: placed(97)}])
    with pytest.raises(ValueError, match="no RadiusCheck_mc"):
        inf.fog_layers(doc, {82: b""})


def test_bundle_writes_native_dds_and_removes_only_its_legacy_files(tmp_path):
    import hashlib
    import io
    from PIL import Image

    catalog = {"version": inf.SCHEMA_VERSION, "sites": []}
    fog = {"symbol": inf.FOG_SYMBOL, "layers": [{"file": "fog0.png", "scale": 1.0, "period": 2.0}]}
    pixels = io.BytesIO()
    Image.new("RGBA", (4, 4), (20, 18, 16, 128)).save(pixels, format="PNG")
    legacy = tmp_path / "PrismaUI_F4/views/B21_FullScreenMap/tales/infestations"
    legacy.mkdir(parents=True)
    for name in ("fog0.png", "fog1.png", "fog2.png", "presentation.json", "map-extension.js"):
        (legacy / name).write_text("stale", encoding="utf-8")
    (legacy / "unrelated.txt").write_text("keep", encoding="utf-8")
    manifest = inf.write_bundle(tmp_path, catalog, fog, {"fog0.png": pixels.getvalue()})
    bundle = tmp_path / inf.OUTPUT
    assert inf.OUTPUT.as_posix() == "F4SE/Plugins/B21_FullScreenMap/infestations"
    dds = (bundle / "fog0.dds").read_bytes()
    assert dds[:4] == b"DDS "
    assert struct.unpack_from("<I", dds, 128)[0] == 98  # DXGI_FORMAT_BC7_UNORM
    assert not (bundle / "map-extension.js").exists()
    assert json.loads((bundle / "presentation.json").read_text(encoding="utf-8")) == manifest
    assert manifest["version"] == 1
    assert manifest["fog"]["layers"][0]["file"] == "fog0.dds"
    assert manifest["fog"]["layers"][0]["scale"] == 1.0
    assert manifest["fog"]["layers"][0]["period"] == 2.0
    assert manifest["fog"]["layers"][0]["sha256"] == hashlib.sha256(dds).hexdigest()
    assert manifest["files"] == [{"path": f"{inf.OUTPUT.as_posix()}/fog0.dds", "sha256": hashlib.sha256(dds).hexdigest()}]
    assert json.loads((tmp_path / inf.CATALOG).read_text(encoding="utf-8")) == inf.to_document(catalog)
    assert inf.CATALOG.as_posix() == "F4SE/Plugins/B21_TalesFromAppalachia/Infestations/SeventySix.esm.json"
    assert list(legacy.iterdir()) == [legacy / "unrelated.txt"]


def test_workflow_phase_passes_plugins_and_skips_softly(tmp_path, monkeypatch):
    from bacup_lib.workflows.unified import _convert_fo76_infestations

    request = NS(source_game="fo76", target_game="fo4")
    mod = tmp_path / "converted"
    ctx = NS(mod_path=mod, source_data_dir=tmp_path / "source",
             source_plugin_path=tmp_path / "source.esm", output_plugin_name="SeventySix.esm")
    calls = []

    def fake(source_root, output_mod, **plugins):
        calls.append((source_root, output_mod, plugins))
        return {"catalog": {"sites": [], "factions": [], "mutations": [], "regions": [], "skipped": {}},
                "presentation": {"files": [{"path": "a"}, {"path": "b"}]}}

    monkeypatch.setattr(inf, "convert_infestations", fake)
    assert _convert_fo76_infestations(request, ctx) == 3
    assert calls == [(ctx.source_data_dir, mod, {
        "source_plugin": ctx.source_plugin_path, "converted_plugin": mod / "SeventySix.esm"})]

    stale = [mod / inf.CATALOG, mod / inf.OUTPUT / "presentation.json", mod / inf.LEGACY_OUTPUT / "presentation.json"]
    for path in stale:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("{}", encoding="utf-8")

    def broken(*_args, **_kwargs):
        raise ValueError("no sites")

    monkeypatch.setattr(inf, "convert_infestations", broken)
    assert _convert_fo76_infestations(request, ctx) == 0
    assert not any(path.exists() for path in stale)
    request.source_game = "skyrimse"
    assert _convert_fo76_infestations(request, ctx) == 0
    assert len(calls) == 1


def test_document_uses_formkeys_and_validates_against_its_schema():
    from bacup_lib.tests.catalog_schema_validation import validate_catalog

    catalog = {"version": 1, "plugin": "SeventySix.esm", "source": {"quest": "865FA8", "data_version": "1.7"},
               "tuning": {"expire_seconds": 18000.0, "marker_radius": 1.0, "boss_kill_fraction": 0.8, "remaining_fraction": 0.5},
               "forms": {"quest": "865FA8", "started_message": "90B5EB"},
               "reward": {"xp_global": "888541", "boss_rank": 4}, "mutations": ["5C6BE9"],
               "factions": [{"key": "Cultist", "keyword": "8A4151", "mob": "85A38B", "boss": "85A38C"}],
               "regions": [{"key": "Forest", "location": "095004", "toggle": "8A5FA4"}],
               "sites": [{"location": "004142", "editor_id": "Loc", "region": "095004", "world": "25DA15",
                          "center": [1.0, 2.0, 3.0], "marker": "00415D", "clear": ["0563A9"], "boss": ["29E9AA"]}],
               "skipped": {"no_boss": 1}, "fog_sha256": ["ab"]}
    document = inf.to_document(catalog)
    assert document["sites"][0]["location"] == "SeventySix.esm:004142"
    assert document["sites"][0]["clear"] == ["SeventySix.esm:0563A9"]
    assert document["settings"]["reward"]["boss_rank"] == 4
    assert document["mutations"] == [{"form": "SeventySix.esm:5C6BE9"}]
    validate_catalog("Infestations", document)
