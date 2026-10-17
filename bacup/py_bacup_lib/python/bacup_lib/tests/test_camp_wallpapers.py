from creation_lib.nif.nif_file import NifFile

import json
from types import SimpleNamespace

from bacup_lib.camp_wallpapers import adopt_base_fixtures, build_catalog, emit_camp_wallpapers
from bacup_lib.tests.catalog_schema_validation import hex_key, validate_catalog


def reference(object_id):
    return {"variant": "reference", "value": {"reference": {"plugin": "SeventySix.esm", "object_id": f"{object_id:06X}"}}}


def form_property(value, function=None):
    row = {"ValueType": 4, "Value1": value, "Value2": 3}
    if function is not None:
        row["FunctionType"] = function
    return row


class FakeReader:
    def __init__(self, records):
        self.records = records

    def lookup(self, signature, editor_id):
        for object_id, (found_signature, found_editor_id, fields) in self.records.items():
            if (found_signature, found_editor_id) == (signature, editor_id):
                return object_id, fields
        raise ValueError(editor_id)

    def fields(self, object_id):
        return self.records[object_id]


class FakeResolver:
    def __init__(self, missing=()):
        self.missing = set(missing)

    def resolve(self, signature, editor_id):
        return None if editor_id in self.missing else f"Converted.esm:{editor_id}"


def omod(editor_id, properties, art=None):
    fields = {"Data": [{"Properties": properties}]}
    if art is not None:
        fields["ArtObject"] = [{"reference": {"plugin": "SeventySix.esm", "object_id": f"{art:06X}"}}]
    return ("OMOD", editor_id, fields)


RECORDS = {
    0x10: ("LVLI", "ATX_workshop_LL_Wallpapers", {"LVLO": [reference(0x20), reference(0x21), reference(0x22), reference(0x23)]}),
    0x20: omod("Remove", [{"ValueType": 2, "Property": 2}], art=0x30),
    0x21: omod("Birds", [form_property(0x40, 2), form_property(0x50, 2)], art=0x31),
    0x22: omod("Caravan", [form_property(0x41), form_property(0x50, 2)]),
    0x23: omod("HalfMade", [form_property(0x42, 2)]),
    0x30: ("ARTO", "RemoveArt", {}),
    0x31: ("ARTO", "BirdsArt", {}),
    0x40: ("MSWP", "BirdsSwap", {}),
    0x41: ("MSWP", "CaravanSwap", {}),
    0x42: ("MSWP", "HalfMadeSwap", {}),
    0x50: ("MDSP", "Walls", {"InitialModel": ["Meshes/Architecture/Wall.nif", "Same.nif"],
                             "NewModel": ["Architecture\\Wall_Wallpaper.nif", "same.nif"]}),
}


def test_catalog_keeps_menu_order_with_remove_first_and_reads_set_or_add_forms():
    catalog = build_catalog(FakeReader(RECORDS), FakeResolver())
    assert [row["editor_id"] for row in catalog["wallpapers"]] == ["Remove", "Birds", "Caravan"]
    remove, birds, caravan = catalog["wallpapers"]
    assert remove == {"form": "Converted.esm:Remove", "editor_id": "Remove", "strip": True, "art": "Converted.esm:RemoveArt"}
    assert birds["material_swap"] == "Converted.esm:BirdsSwap" and birds["model_swap"] == "Walls"
    assert caravan["material_swap"] == "Converted.esm:CaravanSwap" and caravan["art"] is None
    assert catalog["model_swaps"] == {"Walls": [["architecture\\wall.nif", "architecture\\wall_wallpaper.nif"]]}


def test_unconverted_wallpapers_and_swaps_are_reported_not_listed():
    catalog = build_catalog(FakeReader(RECORDS), FakeResolver(missing={"Caravan", "BirdsSwap"}))
    assert [row["editor_id"] for row in catalog["wallpapers"]] == ["Remove"]
    assert catalog["missing_converted"] == ["BirdsSwap", "Caravan"]


class HexResolver(FakeResolver):
    def resolve(self, signature, editor_id):
        return hex_key("SeventySix.esm", editor_id)


def test_emitted_document_matches_its_schema(monkeypatch, tmp_path):
    from bacup_lib import camp_wallpapers
    native = SimpleNamespace(plugin_handle_load_index=lambda *args, **kwargs: 1, plugin_handle_close=lambda handle: None)
    monkeypatch.setattr(camp_wallpapers, "load_esp_native", lambda: native)
    monkeypatch.setattr(camp_wallpapers, "plugin_masters", lambda *args: [])
    monkeypatch.setattr(camp_wallpapers, "NativeReader", lambda *args: FakeReader(RECORDS))
    monkeypatch.setattr(camp_wallpapers, "NativeResolver", lambda *args: HexResolver())
    emit_camp_wallpapers(tmp_path / "src" / "SeventySix.esm", tmp_path / "SeventySix.esm", tmp_path)
    path = tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/CampWallpapers/SeventySix.esm.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    validate_catalog("CampWallpapers", document)
    assert document["version"] == 1 and list(document) == ["version", "wallpapers", "model_swaps"]
    assert [row["editor_id"] for row in document["wallpapers"]] == ["Remove", "Birds", "Caravan"]
    assert document["wallpapers"][0]["strip"] is True and "remove" not in document["wallpapers"][0]


def node(nif, name, parent=0, translation=(0.0, 0.0, 0.0)):
    block = nif.add_block("NiNode", {"Name": name, "Scale": 1.0,
                                     "Translation": dict(zip("xyz", translation))})
    root = nif.blocks[parent]
    root.set_field("Children", [*root.get_field("Children"), block.block_id])
    root.set_field("Num Children", len(root.get_field("Children")))
    return block.block_id


def collide(nif, target, system, body):
    block = nif.add_block("bhkNPCollisionObject", {"Target": target, "Data": system, "Body ID": body})
    nif.blocks[target].set_field("Collision Object", block.block_id)
    return block.block_id


def base_wall(carrier_offset=(0.0, 0.0, 0.0)):
    nif = NifFile.new("FO4")
    bsx = nif.add_block("BSXFlags", {"Name": "BSX", "Integer Data": 10}).block_id
    cpa = nif.add_block("BSConnectPoint::Parents", {"Name": "CPA", "Num Connect Points": 1, "Connect Points": [
        {"Parent": "WorkshopConnectPoints", "Name": "P-1", "Rotation": {"w": 1.0, "x": 0.0, "y": 0.0, "z": 0.0},
         "Translation": {"x": 0.0, "y": 128.0, "z": 0.0}, "Scale": 1.0}]}).block_id
    nif.blocks[0].set_field("Extra Data List", [bsx, cpa])
    nif.blocks[0].set_field("Num Extra Data List", 2)
    system = nif.add_block("bhkPhysicsSystem").block_id
    collide(nif, node(nif, "WallMesh", translation=carrier_offset), system, 0)
    collide(nif, node(nif, "c_NavMeshCutter001"), system, 1)
    node(nif, "WorkshopConnectPoints")
    return nif


def wallpaper_wall():
    nif = NifFile.new("FO4")
    bsx = nif.add_block("BSXFlags", {"Name": "BSX", "Integer Data": 130}).block_id
    nif.blocks[0].set_field("Extra Data List", [bsx])
    nif.blocks[0].set_field("Num Extra Data List", 1)
    system = nif.add_block("bhkPhysicsSystem").block_id
    collide(nif, 0, system, 0)
    node(nif, "wp_GEO_Wall:0")
    return nif


def named(nif, name):
    return next(index for index, block in enumerate(nif.blocks) if block.get_field("Name") == name)


def test_wallpaper_mesh_takes_the_base_connect_points_cutters_and_their_collision():
    base, wallpaper = base_wall(), wallpaper_wall()
    assert adopt_base_fixtures(base, wallpaper) == (["connect points", "navmesh cutters"], [])
    root = wallpaper.blocks[0]
    extras = [wallpaper.blocks[index].type_name for index in root.get_field("Extra Data List")]
    assert extras == ["BSXFlags", "BSConnectPoint::Parents"] and root.get_field("Num Extra Data List") == 2
    children = [wallpaper.blocks[index].get_field("Name") for index in root.get_field("Children")]
    assert children == ["wp_GEO_Wall:0", "WorkshopConnectPoints", "c_NavMeshCutter001"]
    assert root.get_field("Num Children") == 3
    collisions = [index for index, block in enumerate(wallpaper.blocks) if block.type_name == "bhkNPCollisionObject"]
    systems = [index for index, block in enumerate(wallpaper.blocks) if block.type_name == "bhkPhysicsSystem"]
    assert len(collisions) == 2 and len(systems) == 1
    wall = root.get_field("Collision Object")
    assert wallpaper.blocks[wall].get_field("Target") == 0 and wallpaper.blocks[wall].get_field("Body ID") == 0
    cutter = named(wallpaper, "c_NavMeshCutter001")
    cutter_collision = wallpaper.blocks[cutter].get_field("Collision Object")
    assert wallpaper.blocks[cutter_collision].get_field("Target") == cutter
    assert {wallpaper.blocks[index].get_field("Data") for index in collisions} == set(systems)
    assert wallpaper.blocks[named(wallpaper, "BSX")].get_field("Integer Data") == 10
    assert adopt_base_fixtures(base, wallpaper) == ([], [])


def test_cutters_whose_collision_cannot_move_to_the_root_are_reported():
    base, wallpaper = base_wall(carrier_offset=(0.0, 4.0, 0.0)), wallpaper_wall()
    assert adopt_base_fixtures(base, wallpaper) == (["connect points"], ["navmesh cutters"])
    assert not any("navmeshcut" in str(block.get_field("Name")).lower() for block in wallpaper.blocks)
    assert wallpaper.blocks[named(wallpaper, "BSX")].get_field("Integer Data") == 130


def test_workflow_prepares_wallpapers_only_for_fo76_to_fo4(monkeypatch, tmp_path):
    from types import SimpleNamespace
    from bacup_lib import camp_wallpapers
    from bacup_lib.workflows import unified
    calls = []
    result = {"wallpapers": [1, 2], "missing_converted": [], "missing_meshes": [], "unrepaired_meshes": [], "repaired_meshes": []}
    monkeypatch.setattr(camp_wallpapers, "emit_camp_wallpapers", lambda *args: calls.append(args) or result)
    ctx = SimpleNamespace(source_plugin_path=tmp_path / "SeventySix.esm", mod_path=tmp_path / "mod", output_plugin_name="SeventySix.esm")
    assert unified._prepare_fo76_camp_wallpapers(SimpleNamespace(source_game="fo76", target_game="fo4"), ctx) == 2
    assert calls == [(ctx.source_plugin_path, ctx.mod_path / "SeventySix.esm", ctx.mod_path)]
    assert unified._prepare_fo76_camp_wallpapers(SimpleNamespace(source_game="skyrimse", target_game="fo4"), ctx) == 0
    assert len(calls) == 1


def test_repaired_mesh_survives_a_save(tmp_path):
    base, wallpaper = base_wall(), wallpaper_wall()
    adopt_base_fixtures(base, wallpaper)
    path = tmp_path / "wall_wallpaper.nif"
    wallpaper.save(str(path))
    loaded = NifFile.load(str(path))
    assert loaded.blocks[named(loaded, "CPA")].get_field("Connect Points")[0]["Name"] == "P-1"
    assert loaded.blocks[loaded.blocks[0].get_field("Collision Object")].get_field("Target") == 0
