import json
import struct
import zlib
from types import SimpleNamespace as NS

import pytest

from bacup_lib import infestations as inf
from creation_lib.swf.types import MATRIX, BitWriter


def ref(object_id: int):
    return {"reference": {"plugin": "SeventySix.esm", "object_id": f"{object_id:06X}"}}


class Source:
    def __init__(self):
        self.records, self.by_name = {}, {}

    def add(self, object_id, signature, editor_id, **fields):
        grouped = {key: value if isinstance(value, list) else [value] for key, value in fields.items()}
        self.records[object_id] = (signature, editor_id, grouped)
        self.by_name[(signature, editor_id)] = object_id

    def lookup(self, signature, editor_id):
        object_id = self.by_name[(signature, editor_id)]
        return object_id, self.records[object_id][2]

    def fields(self, object_id):
        return self.records[object_id]

    def ids(self, signature):
        return [object_id for object_id, record in self.records.items() if record[0] == signature]


class Converted:
    def __init__(self, source: Source, missing=(), positions=None):
        self.source, self.missing, self.positions = source, set(missing), positions or {}

    def record(self, object_id):
        if object_id in self.missing:
            return None
        if object_id in self.positions:
            x, y, z = self.positions[object_id]
            return "REFR", {"DATA": [{"PositionRotationPositionX": x, "PositionRotationPositionY": y,
                                      "PositionRotationPositionZ": z}]}
        if object_id not in self.source.records:
            return ("REFR", {}) if object_id >= 0x700000 else None
        signature, _, fields = self.source.records[object_id]
        signature = "MISC" if signature == "CNCY" else signature
        return signature, {"LVLO": [{}]} if signature == "LVLN" else fields


LCRT = {"LocationCenterMarker": 0x01F40F, "MapMarkerRefType": 0x02271F, "Boss": 0x358545,
        "LocationClearActor": 0x003956}
WORLD, OTHER_WORLD = 0x25DA15, 0x000100


def special(kind: str, ref_id: int, world: int = WORLD):
    return {"MasterSpecialReferencesLocRefType": ref(LCRT[kind]), "MasterSpecialReferencesRef": ref(ref_id),
            "MasterSpecialReferencesWorldCell": ref(world)}


def location_refs(base: int, clear: int = 4, world: int = WORLD, boss: bool = True):
    rows = [special("LocationCenterMarker", base, world), special("MapMarkerRefType", base + 1, world)]
    if boss:
        rows.append(special("Boss", base + 2, world))
    rows += [special("LocationClearActor", base + 10 + index, world) for index in range(clear)]
    return rows


def source_data():
    source = Source()
    source.add(0x865FA8, "QUST", inf.QUEST, VirtualMachineAdapter={"Scripts": [{
        "ScriptName": inf.MASTER_SCRIPT, "Properties": [
            {"propertyName": "MobKillPercentToSpawnBoss", "Value": 0.8},
            {"propertyName": "MobKillPercentToShowRemainingObjective", "Value": 0.5}]}]})
    for offset, (key, name) in enumerate(inf.TUNING_GLOBALS.items()):
        source.add(0x8A5600 + offset, "GLOB", name, Value={"expire_seconds": 18000.0}.get(key, 35000.0))
    for offset, (signature, name) in enumerate([*inf.FORMS.values(), *inf.REWARD.values()]):
        source.add(0x890000 + offset, signature, name)
    for name, object_id in LCRT.items():
        source.add(object_id, "LCRT", name)
    source.add(WORLD, "WRLD", inf.WORLD)
    regions = []
    for offset, (_, location, toggle) in enumerate(inf.REGIONS):
        source.add(0x010000 + offset, "LCTN", location)
        source.add(0x8A5FA1 + offset, "GLOB", toggle)
        regions.append(0x010000 + offset)
    keywords = []
    for offset, keyword in enumerate(("HTO_FactionSelection_Scorched", "SDOW_HTO_FactionSelection_SlasherShadow")):
        source.add(0x8A4150 + offset, "KYWD", keyword)
        mob, boss = inf.faction_lists(keyword)
        source.add(0x85A300 + offset * 2, "LVLN", mob)
        source.add(0x85A301 + offset * 2, "LVLN", boss)
        keywords.append(ref(0x8A4150 + offset))
    source.add(0x8A4152, "FLST", inf.FACTION_LIST, FormID=keywords)
    source.add(0x5C6BE9, "SPEL", "Mutation_A")
    source.add(0x5C6BEA, "SPEL", "Mutation_Missing")
    source.add(0x897400, "FLST", inf.MUTATION_LIST, FormID=[ref(0x5C6BE9), ref(0x5C6BEA)])
    # A parent chain: the site sits under a sub-location of the Forest region.
    source.add(0x020000, "LCTN", "ForestSubRegion", ParentLocation=ref(regions[0]))
    source.add(0x030000, "LCTN", "GoodSite", ParentLocation=ref(0x020000),
               MasterSpecialReferences=[location_refs(0x700000, clear=5)])
    source.add(0x030001, "LCTN", "FewActors", ParentLocation=ref(regions[1]),
               MasterSpecialReferences=[location_refs(0x710000, clear=3)])
    source.add(0x030002, "LCTN", "OtherWorld", ParentLocation=ref(regions[1]),
               MasterSpecialReferences=[location_refs(0x720000, world=OTHER_WORLD)])
    source.add(0x030003, "LCTN", "NoBoss", ParentLocation=ref(regions[2]),
               MasterSpecialReferences=[location_refs(0x730000, boss=False)])
    source.add(0x030004, "LCTN", "Unconverted", ParentLocation=ref(regions[3]),
               MasterSpecialReferences=[location_refs(0x740000)])
    source.add(0x030005, "LCTN", "NoRegion", MasterSpecialReferences=[location_refs(0x750000)])
    return source


def converted_data(source):
    positions = {0x700000: (100.0, -200.0, 5.0), 0x740000: (1.0, 2.0, 3.0)}
    # The converter drops two clear actors of the unconverted site, leaving three.
    return Converted(source, missing={0x5C6BEA, 0x74000A, 0x74000B}, positions=positions)


def test_catalog_selects_only_eligible_converted_sites():
    source = source_data()
    catalog = inf.build_catalog(source, converted_data(source), "1.7.26.13")

    assert catalog["source"] == {"quest": "865FA8", "data_version": "1.7.26.13"}
    assert catalog["tuning"]["boss_kill_fraction"] == 0.8
    assert catalog["tuning"]["remaining_fraction"] == 0.5
    assert catalog["tuning"]["expire_seconds"] == 18000.0
    assert catalog["reward"]["boss_rank"] == 4
    assert catalog["mutations"] == ["5C6BE9"]
    assert [row["key"] for row in catalog["regions"]] == [key for key, _, _ in inf.REGIONS]
    assert [site["editor_id"] for site in catalog["sites"]] == ["GoodSite"]
    site = catalog["sites"][0]
    assert site["region"] == "010000" and site["world"] == "25DA15"
    assert site["center"] == [100.0, -200.0, 5.0]
    assert site["marker"] == "700001" and site["boss"] == ["700002"]
    assert site["clear"] == [f"{0x70000A + index:06X}" for index in range(5)]
    assert catalog["skipped"] == {"few_clear_actors": 1, "no_boss": 1, "not_converted": 1, "outside_world": 2}
    assert catalog["factions"] == [
        {"key": "Scorched", "keyword": "8A4150", "mob": "85A300", "boss": "85A301"},
        {"key": "SlasherShadow", "keyword": "8A4151", "mob": "85A302", "boss": "85A303"}]
    assert inf.faction_lists("SDOW_HTO_FactionSelection_SlasherShadow") == (
        "SDOW_HTO_LChar_Faction_Slasher", "SDOW_HTO_LChar_Faction_Slasher_Boss")


def test_catalog_rejects_missing_quest_empty_faction_list_and_bad_kill_fraction():
    source = source_data()
    with pytest.raises(ValueError, match="865FA8"):
        inf.build_catalog(source, Converted(source, missing={0x865FA8}))

    class EmptyLists(Converted):
        def record(self, object_id):
            found = super().record(object_id)
            return ("LVLN", {}) if found and found[0] == "LVLN" else found

    with pytest.raises(ValueError, match="empty"):
        inf.build_catalog(source, EmptyLists(source, positions={0x700000: (0.0, 0.0, 0.0)}))

    source.records[0x865FA8][2]["VirtualMachineAdapter"][0]["Scripts"][0]["Properties"][0]["Value"] = 1.5
    with pytest.raises(ValueError, match="kill fraction"):
        inf.build_catalog(source, converted_data(source))


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


def test_bundle_writes_catalog_art_and_extension(tmp_path):
    catalog = {"version": inf.SCHEMA_VERSION, "sites": []}
    fog = {"symbol": inf.FOG_SYMBOL, "layers": [{"file": "fog0.png", "scale": 1.0, "period": 2.0}]}
    manifest = inf.write_bundle(tmp_path, catalog, fog, {"fog0.png": b"png"})
    bundle = tmp_path / inf.OUTPUT
    assert (bundle / "fog0.png").read_bytes() == b"png"
    assert (bundle / "map-extension.js").read_text(encoding="utf-8").startswith("(function")
    assert json.loads((bundle / "presentation.json").read_text(encoding="utf-8")) == manifest
    assert manifest["version"] == 1 and manifest["fog"] == fog
    assert [row["path"] for row in manifest["files"]] == [
        f"{inf.OUTPUT.as_posix()}/fog0.png", f"{inf.OUTPUT.as_posix()}/map-extension.js"]
    assert json.loads((tmp_path / inf.CATALOG).read_text(encoding="utf-8")) == catalog


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

    stale = [mod / inf.CATALOG, mod / inf.OUTPUT / "presentation.json"]
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
