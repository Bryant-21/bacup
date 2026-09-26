import io
import struct

import pytest
from PIL import Image

from bacup_lib import reputation_map_ui as ui
from bacup_lib.quest_area_ui import tag
from creation_lib.swf.tags import PlaceObject2Tag
from creation_lib.swf.types import MATRIX


def test_freezing_nested_timeline_retains_source_transform_and_replacement():
    first = PlaceObject2Tag(1, 20, MATRIX(scale_x=0.5, scale_y=1), name="face")
    moved = PlaceObject2Tag(1, matrix=MATRIX(scale_x=1, scale_y=1), move=True)
    replaced = PlaceObject2Tag(1, 21, move=True)
    timeline = struct.pack("<HH", 10, 3) + b"".join([
        tag(26, first.to_bytes()), tag(1, b""), tag(26, moved.to_bytes()), tag(1, b""),
        tag(26, replaced.to_bytes()), tag(1, b""), tag(0, b"")])
    frozen = ui.freeze(timeline, 3)
    row, = ui.frame_objects(frozen, 1)
    assert row.character_id == 21 and row.name == "face" and not row.move
    assert row.matrix.scale_x == 1
    assert ui.frame_objects(timeline, 1)[0].matrix.scale_x == 0.5
    with pytest.raises(ValueError, match="no frame"):
        ui.freeze(timeline, 4)


def test_empty_frame_and_removal_are_preserved():
    timeline = struct.pack("<HH", 1, 3) + tag(1, b"") + tag(26, PlaceObject2Tag(2, 3).to_bytes()) + tag(1, b"")
    timeline += tag(28, struct.pack("<H", 2)) + tag(1, b"") + tag(0, b"")
    assert ui.frame_objects(ui.freeze(timeline, 1), 1) == []
    assert len(ui.frame_objects(ui.freeze(timeline, 2), 1)) == 1
    assert ui.frame_objects(ui.freeze(timeline, 3), 1) == []


def test_loose_bc7_assets_keep_alpha_and_uv_without_stretching(tmp_path, monkeypatch):
    png = io.BytesIO()
    Image.new("RGBA", (9, 7), (30, 70, 90, 204)).save(png, format="PNG")
    monkeypatch.setattr(ui, "source_assets", lambda _: ({"socialreputationwidget.swf": b"local source"},
        {"rowHeight": 119}, {"header": png.getvalue()}))
    result = ui.convert_reputation_map_ui(tmp_path / "source", tmp_path / "converted")
    assert result == ui.convert_reputation_map_ui(tmp_path / "source", tmp_path / "converted")
    image = result["images"]["header"]
    assert image["uv"] == [9 / 12, 7 / 8]
    path = tmp_path / "converted" / result["files"][0]["path"]
    assert path.is_relative_to(tmp_path / "converted" / ui.OUTPUT)
    with Image.open(path) as pixels:
        assert pixels.size == (12, 8)
        assert abs(pixels.getpixel((2, 2))[3] - 204) <= 1
        assert pixels.getpixel((11, 7))[3] <= 1
    assert {p.suffix for p in (tmp_path / "converted").rglob("*") if p.is_file()} == {".dds", ".json"}


def test_empty_artwork_is_rejected(tmp_path, monkeypatch):
    png = io.BytesIO()
    Image.new("RGBA", (4, 4)).save(png, format="PNG")
    monkeypatch.setattr(ui, "source_assets", lambda _: ({}, {}, {"header": png.getvalue()}))
    with pytest.raises(ValueError, match="Empty reputation"):
        ui.convert_reputation_map_ui(tmp_path, tmp_path / "converted")


def test_emblem_removes_source_cream_without_changing_alpha_or_black_outlines():
    image = Image.new("RGBA", (3, 1))
    image.putdata([(255, 255, 203, 173), (0, 0, 0, 255), (128, 128, 102, 0)])
    png = io.BytesIO()
    image.save(png, format="PNG")
    with Image.open(io.BytesIO(ui.neutralize_emblem(png.getvalue()))) as neutral:
        assert [neutral.getpixel((x, 0)) for x in range(3)] == [(255, 255, 255, 173), (0, 0, 0, 255), (128, 128, 128, 0)]
