from __future__ import annotations

import json
from pathlib import Path

from PIL import Image

from bacup_lib.fullscreen_map import export_legacy_world_maps, export_terrain_world_maps


def test_legacy_wrld_icon_becomes_map_pack(monkeypatch, tmp_path: Path) -> None:
    from creation_lib.esp.model import Record
    from creation_lib.esp.plugin import Plugin

    source_path = tmp_path / "FalloutNV.esm"
    plugin = Plugin.new(source_path.name, game="fnv", masters=[])
    try:
        world = Record("WRLD", 0x000DA726)
        world.add_subrecord("EDID", b"WastelandNV\0")
        world.add_subrecord("FULL", b"Mojave Wasteland\0")
        world.add_subrecord("ICON", b"Interface\\worldmap\\wasteland_nv_1024_no_map.dds\0")
        plugin.add_record(world)
        plugin.save(source_path)
    finally:
        plugin.close()

    converted = (
        tmp_path
        / "data"
        / "Textures"
        / "Interface"
        / "worldmap"
        / "wasteland_nv_1024_no_map.dds"
    )
    converted.parent.mkdir(parents=True)
    converted.write_bytes(b"dds")

    def fake_load_image(path: str, mode: str = "RGBA") -> Image.Image:
        assert Path(path) == converted
        assert mode == "RGBA"
        return Image.new("RGBA", (8, 4), (10, 20, 30, 255))

    monkeypatch.setattr("creation_lib.dds.io.load_image", fake_load_image)

    assert export_legacy_world_maps(
        mod_root=tmp_path, source_plugins=[source_path], source_game="fnv"
    ) == {"wastelandnv"}
    pack = tmp_path / "PrismaUI_F4" / "views" / "B21_FullScreenMap" / "maps" / "wastelandnv"
    manifest = json.loads((pack / "map.json").read_text(encoding="utf-8"))
    assert manifest["worldspace"] == "WastelandNV"
    assert manifest["title"] == "Mojave Wasteland"
    assert manifest["calibration"] == {"mode": "mnam"}
    with Image.open(pack / "map.png") as image:
        assert image.size == (8, 4)


def test_terrain_lod_tiles_become_north_up_map_pack(monkeypatch, tmp_path: Path) -> None:
    terrain = tmp_path / "data" / "Textures" / "terrain" / "Tamriel"
    terrain.mkdir(parents=True)
    colors = {
        "Tamriel.32.-32.0.dds": (255, 0, 0, 255),
        "Tamriel.32.0.0.dds": (0, 255, 0, 255),
        "Tamriel.32.-32.-32.dds": (0, 0, 255, 255),
        "Tamriel.32.0.-32.dds": (255, 255, 0, 255),
    }
    for name in colors:
        (terrain / name).write_bytes(b"dds")
    # A finer level and a normal tile must not be mixed into the selected level.
    (terrain / "Tamriel.16.0.0.dds").write_bytes(b"dds")
    (terrain / "Tamriel.32.0.0_msn.dds").write_bytes(b"dds")

    def fake_load_image(path: str, mode: str = "RGBA") -> Image.Image:
        assert mode == "RGBA"
        return Image.new("RGBA", (2, 2), colors.get(Path(path).name, (1, 1, 1, 255)))

    monkeypatch.setattr("creation_lib.dds.io.load_image", fake_load_image)

    assert export_terrain_world_maps(mod_root=tmp_path) == {"tamriel"}
    pack = tmp_path / "PrismaUI_F4" / "views" / "B21_FullScreenMap" / "maps" / "tamriel"
    manifest = json.loads((pack / "map.json").read_text(encoding="utf-8"))
    assert manifest == {
        "worldspace": "Tamriel",
        "title": "TAMRIEL",
        "image": "map.png",
        "discovery": "native",
        "calibration": {
            "mode": "frame",
            "nwCellX": -32,
            "nwCellY": 31,
            "seCellX": 31,
            "seCellY": -32,
            "x0": 0,
            "y0": 0,
            "x1": 4,
            "y1": 4,
        },
    }
    with Image.open(pack / "map.png") as image:
        assert image.size == (4, 4)
        assert image.getpixel((0, 0)) == (255, 0, 0, 255)
        assert image.getpixel((3, 0)) == (0, 255, 0, 255)
        assert image.getpixel((0, 3)) == (0, 0, 255, 255)
        assert image.getpixel((3, 3)) == (255, 255, 0, 255)
