import struct
import zlib

from creation_lib.swf import native_runtime

from bacup_lib import daily_ops_ui as ui


def movie(imports=(), code=b"original"):
    tags = []
    for name in imports:
        data = name.encode() + b"\0\x01\x00\x00\x00"
        tags.append(struct.pack("<HI", (71 << 6) | 63, len(data)) + data)
    tags.append(struct.pack("<HI", (82 << 6) | 63, len(code)) + code)
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(tags) + b"\0\0"
    return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body


def test_dependency_closure_and_translation_preservation(tmp_path, monkeypatch):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / ui.MENU).write_bytes(movie(["Icons.swf"]))
    (interface / "hudmenu.swf").write_bytes(movie())
    (interface / "vocharacteranim.swf").write_bytes(movie())
    (interface / "icons.swf").write_bytes(movie(["FONTS_EN.swf"]))
    (interface / "fonts_en.swf").write_bytes(movie())
    (interface / "translate_en.txt").write_text("$DO_TIME\t{0}:{1}\n$EXIT\tExit\n$Unused\tNo\n", encoding="utf-16")
    output = tmp_path / "output"
    translations = ui.translation_path(output)
    translations.parent.mkdir(parents=True)
    translations.write_text("$LegendaryPerks\tPerks\n$DO_TIME\told\n", encoding="utf-16")
    monkeypatch.setattr(ui, "build_radio_movie", lambda _: movie(code=b"radio"))

    def compile_bridge(data, sources, dependencies):
        assert sources
        return data.replace(b"original", b"replaced")

    monkeypatch.setattr(native_runtime, "replace_as3_classes", compile_bridge)
    first = ui.convert_daily_ops_ui(tmp_path / "source", output)
    second = ui.convert_daily_ops_ui(tmp_path / "source", output)
    assert first == second
    assert len(first["files"]) == 5
    assert first["source_classes"] == ["VOFlyoutManager"]
    assert first["quest_owner"] == "StatusHUD"
    assert (output / ui.OUTPUT / ui.HUD).is_file()
    assert not (output / ui.OUTPUT / "hudmenu.swf").exists()
    text = translations.read_text(encoding="utf-16")
    assert text.count("$DO_TIME\t") == 1
    assert "$LegendaryPerks\tPerks" in text
    assert "$Unused" not in text
    assert (output / ui.OUTPUT / "icons.swf").read_bytes() == (interface / "icons.swf").read_bytes()


def test_radio_bitmap_fills_keep_their_image_resources():
    bitmap = struct.pack("<HBHH", 3, 5, 1, 1) + zlib.compress(b"\xff\xff\xff\xff")
    shape = struct.pack("<H", 2) + b"\x08\x00\x01\x41\x03\x00\x00\x00\x10\x00"
    placement = b"\x22\x01\x00\x02\x00DOVOWaveForm_mc\x00"
    panel = b"\x22\x02\x00\x02\x00DOVOFlyoutGraphic_mc\x00"
    sprite = struct.pack("<HH", 1, 1) + ui.tag(26, placement) + ui.tag(26, panel) + ui.tag(1, b"") + ui.tag(0, b"")
    symbols = b"\x01\x00\x01\x00VOFlyoutManager\x00"
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(
        ui.tag(code, payload) for code, payload in ((36, bitmap), (32, shape), (39, sprite),
                                                   (76, symbols), (1, b""), (0, b"")))
    source = b"FWS\x11" + struct.pack("<I", len(body) + 8) + body

    converted = ui.build_radio_movie(source)

    assert (36, bitmap) in ui.swf_tags(converted)
