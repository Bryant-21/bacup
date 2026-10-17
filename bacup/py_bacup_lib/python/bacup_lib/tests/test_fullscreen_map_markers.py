from __future__ import annotations

import json
from pathlib import Path

from PIL import Image, ImageDraw

from bacup_lib import fullscreen_map_markers as markers


def _icon() -> Image.Image:
    icon = Image.new("RGBA", (20, 10))
    icon.paste((255, 255, 203, 255), (0, 0, 10, 10))  # FO76 highlight -> bright band
    icon.paste((245, 205, 91, 255), (10, 0, 20, 10))  # FO76 amber -> mid band
    return icon


def test_icon_cells_split_the_fo76_palette_into_tint_bands() -> None:
    shape, shadow, dim, mid, bright = markers.icon_cells(_icon())
    assert all(cell.size == (256, 256) for cell in (shape, shadow, dim, mid, bright))
    assert shape.getchannel("A").getbbox() is not None
    assert shadow.getchannel("A").getbbox() is None
    assert dim.getchannel("A").getbbox() is None
    mid_box, bright_box = mid.getchannel("A").getbbox(), bright.getchannel("A").getbbox()
    assert mid_box and bright_box and bright_box[2] <= mid_box[0] + 2


def test_build_atlas_uses_the_map_atlas_layout() -> None:
    atlas, manifest = markers.build_atlas({"CabinMarker": _icon(), "QuarryMarker": _icon()})
    assert atlas.size == (24 * 256, 256)
    assert manifest["texture"] == "markers.dds" and manifest["cell"] == 256
    cabin = manifest["icons"]["CabinMarker"]
    assert cabin["tintable"] and len(cabin["bands"]) == 4
    assert cabin["shape"] == [0.0, 0.0, 1 / 24, 1.0]
    assert manifest["icons"]["QuarryMarker"]["shape"][0] == 5 / 24


def test_band_boundaries_keep_antialiasing_inside_opaque_icons():
    icon = Image.new("RGBA", (896, 896), (210, 210, 210, 255))
    ImageDraw.Draw(icon).polygon([(0, 0), (895, 0), (0, 895)], fill=(255, 255, 255, 255))
    shape, _, _, _, bright = markers.icon_cells(icon)
    assert any(a == 255 and 0 < band < 255 for a, band in
               zip(shape.getchannel("A").tobytes(), bright.getchannel("A").tobytes()))


def test_atlas_preserves_white_rgb_under_transparent_mask_pixels():
    atlas, _ = markers.build_atlas({"CabinMarker": _icon()})
    assert atlas.getpixel((0, 0)) == (255, 255, 255, 0)


def test_export_writes_markers_whose_icon_rendered(monkeypatch, tmp_path: Path) -> None:
    library = tmp_path / "source" / markers.MARKER_LIBRARY_REL
    library.parent.mkdir(parents=True)
    library.write_bytes(b"swf")
    monkeypatch.setattr(markers, "marker_symbols", lambda _plugin: {0x3919E6: "BloodEagleMarker", 0x4165: "Missing"})
    monkeypatch.setattr(markers, "render_symbols", lambda data, names: ({"BloodEagleMarker": _icon()}, ["Missing"]))
    written: list[Path] = []
    monkeypatch.setattr("bacup_lib.fullscreen_map.write_ui_dds", lambda image, path, pad=False: written.append(path))

    result = markers.export_marker_icons(source_data_dir=tmp_path / "source", source_plugin=tmp_path / "SeventySix.esm",
                                         mod_root=tmp_path / "mod")

    pack = tmp_path / "mod" / markers.PACK_REL
    assert result == {"markers": 1, "icons": 1, "failed_symbols": ["Missing"]}
    assert json.loads((pack / "markers.json").read_text(encoding="utf-8"))["markers"] == [
        {"id": "3919E6", "sym": "BloodEagleMarker"}]
    assert "BloodEagleMarker" in json.loads((pack / "icons" / "atlas.json").read_text(encoding="utf-8"))["icons"]
    assert written == [pack / "icons" / "markers.dds"]


def test_native_marker_scan_requires_xmrk_and_a_complete_type(tmp_path):
    from creation_lib.esp import Plugin
    from bacup_lib.native_runtime import load_native_module

    path = tmp_path / "Markers.esm"
    with Plugin.new(path.name, game="fo76") as plugin:
        for form_id, fields in [(0x800, [("XMRK", b""), ("TNAM", b"\x05\0\0\0")]),
                                (0x801, [("TNAM", b"\x06\0")]),
                                (0x802, [("XMRK", b""), ("TNAM", b"\x05")]),
                                (0x803, [("XMRK", b""), ("TNAM", b"\x07\0")])]:
            record = plugin.new_record("REFR", form_id=form_id)
            for name, data in fields:
                record.add_subrecord(name, data)
            plugin.add_record(record)
        plugin.save(path)
    assert load_native_module().collect_map_marker_types(str(path)) == [(0x800, 5), (0x803, 7)]
