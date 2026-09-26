import hashlib
import io

from PIL import Image
from creation_lib.dds.io import load_dds

from bacup_lib import mission_map_ui as ui


def setup_source(tmp_path, monkeypatch, missing_symbol=False):
    source = tmp_path / "source"
    (source / "interface").mkdir(parents=True)
    for filename in ui.SPRITES:
        (source / "interface" / filename).write_bytes(filename.encode())
    textures = source / "textures/interface/expeditions"
    textures.mkdir(parents=True)
    for name in ui.EXPEDITIONS.values():
        Image.new("RGBA", (8, 6), (35, 45, 60, 190)).save(textures / name)

    def symbols(data):
        specs = ui.SPRITES[data.decode()]
        symbols = sorted({spec[1] for spec in specs if not (missing_symbol and spec[1] == "DOMode_Uplink2")})
        return [(index + 100, symbol) for index, symbol in enumerate(symbols)]

    def export(data, symbol, frame, scale):
        assert any(spec[1:] == (symbol, frame) for spec in ui.SPRITES[data.decode()])
        assert scale == 2.0
        png = io.BytesIO()
        Image.new("RGBA", (12, 10), (20, 90, 150, 128)).save(png, format="PNG")
        return png.getvalue()

    monkeypatch.setattr(ui.native_runtime, "list_symbols", symbols)
    monkeypatch.setattr(ui.native_runtime, "render_symbol_png", export)
    return source


def test_converts_only_local_artwork_with_reproducible_manifest(tmp_path, monkeypatch):
    source = setup_source(tmp_path, monkeypatch)
    output = tmp_path / "converted/data"
    first = ui.convert_mission_map_ui(source, output)
    assert first == ui.convert_mission_map_ui(source, output)
    assert len(first["files"]) == 14
    assert not any("vertibird" in row["path"] or "mapmenu.swf" in row["source"] for row in first["files"])
    for row in first["files"]:
        target = output / row["path"]
        assert target.is_relative_to(output / ui.OUTPUT)
        assert row["sha256"] == hashlib.sha256(target.read_bytes()).hexdigest()
        assert row["source_sha256"] == hashlib.sha256((source / row["source"]).read_bytes()).hexdigest()
        assert target.suffix == ".dds" and target.read_bytes().startswith(b"DDS ")
        with load_dds(str(target)) as pixels:
            assert pixels.mode == "RGBA" and pixels.size == (row["width"], row["height"])
            low, high = pixels.getextrema()[3]
            assert high - low <= 1 and any(abs(low - alpha) <= 1 for alpha in (128, 190))  # BC7 quantizes by ±1
    assert {p.suffix for p in output.rglob("*") if p.is_file()} == {".json", ".dds"}
