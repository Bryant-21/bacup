from __future__ import annotations

import json
from pathlib import Path

from PIL import Image

from bacup_lib import fullscreen_map_markers as markers


def _icon() -> Image.Image:
    icon = Image.new("RGBA", (20, 10))
    icon.paste((255, 255, 203, 255), (0, 0, 10, 10))  # FO76 highlight -> bright band
    icon.paste((245, 205, 91, 255), (10, 0, 20, 10))  # FO76 amber -> mid band
    return icon


def test_icon_cells_split_the_fo76_palette_into_tint_bands() -> None:
    shape, shadow, dim, mid, bright = markers.icon_cells(_icon())
    assert all(cell.size == (128, 128) for cell in (shape, shadow, dim, mid, bright))
    assert shape.getchannel("A").getbbox() is not None
    assert shadow.getchannel("A").getbbox() is None
    assert dim.getchannel("A").getbbox() is None
    mid_box, bright_box = mid.getchannel("A").getbbox(), bright.getchannel("A").getbbox()
    assert mid_box and bright_box and bright_box[2] <= mid_box[0] + 2


def test_build_atlas_uses_the_map_atlas_layout() -> None:
    atlas, manifest = markers.build_atlas({"CabinMarker": _icon(), "QuarryMarker": _icon()})
    assert atlas.size == (24 * 128, 128)
    assert manifest["texture"] == "markers.dds" and manifest["cell"] == 128
    cabin = manifest["icons"]["CabinMarker"]
    assert cabin["tintable"] and len(cabin["bands"]) == 4
    assert cabin["shape"] == [0.0, 0.0, 1 / 24, 1.0]
    assert manifest["icons"]["QuarryMarker"]["shape"][0] == 5 / 24


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
