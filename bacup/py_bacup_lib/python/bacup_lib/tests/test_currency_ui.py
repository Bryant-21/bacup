import struct

from bacup_lib import currency_ui as ui
from bacup_lib.legendary_perks_ui import swf_tags


def movie(dependencies=(), text_fields=()):
    tags = [(71, name.encode() + b"\0\1\0\0\0") for name in dependencies]
    tags += [(37, data) for data in text_fields]
    tags += [(2, b"artwork"), (26, b"old HUD placement"), (82, b"original code"), (1, b""), (0, b"")]
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(
        struct.pack("<HI", (code << 6) | 63, len(data)) + data for code, data in tags)
    return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body


def source(tmp_path, dependencies=()):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / "hudmenu.swf").write_bytes(movie(dependencies))
    (interface / "currencyiconlibrary.swf").write_bytes(movie())
    (interface / "challengerewardiconlibrary.swf").write_bytes(movie())
    for path in ui.SOUNDS:
        target = interface.parent / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(b"source audio " + path.name.encode())
    return interface


def test_source_art_dependency_closure_and_repeatability(tmp_path, monkeypatch):
    interface = source(tmp_path, ["Fonts_EN.swf"])
    (interface / "fonts_en.swf").write_bytes(movie(["currencyiconlibrary.swf"]))
    seen = []

    def replace(movie_data, replacements, dependencies):
        tags = swf_tags(movie_data)
        assert not any(code in (4, 26, 70) for code, _ in tags)
        assert (2, b"artwork") in tags
        assert set(replacements) == {"HUDMenu", "HUDCurrencyUpdatesWidget"}
        assert all(p.is_file() for p in replacements.values())
        seen.append(True)
        return movie_data

    monkeypatch.setattr(ui, "replace_classes", replace)
    output = tmp_path / "converted/data"
    first = ui.convert_currency_ui(interface.parent, output)
    second = ui.convert_currency_ui(interface.parent, output)
    assert len(seen) == 2
    assert first == second
    assert len(first["files"]) == 4
    assert (output / ui.OUTPUT / "challengerewardiconlibrary.swf").read_bytes() == (interface / "challengerewardiconlibrary.swf").read_bytes()
    assert (output / ui.OUTPUT / "currencyiconlibrary.swf").read_bytes() == (interface / "currencyiconlibrary.swf").read_bytes()
    assert not (output / ui.OUTPUT / "hudmenu.swf").exists()
    assert (output / ui.OUTPUT / "currencyhud.swf").is_file()
    assert len(first["audio"]) == 4
    for path in ui.SOUNDS:
        assert (output / path).read_bytes() == (interface.parent / path).read_bytes()
