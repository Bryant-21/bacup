import struct

from creation_lib.swf import native_runtime

from bacup_lib import quest_area_ui as ui
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.tests.test_legendary_perks_ui import swf

BACKDROP = bytes((24, 28, 29))


def short_tag(code, payload=b""):
    return struct.pack("<H", code << 6 | len(payload)) + payload


def text(character, flags=b"\x00\x80"):
    return struct.pack("<H", character) + b"\x08\x00" + flags + b"$Futura_Bold\0\x40\x01"


def sprite(character, *placed):
    tags = b"".join(short_tag(26, b"\x02\x01\x00" + struct.pack("<H", child)) for child in placed)
    return struct.pack("<HH", character, 1) + tags + short_tag(1) + short_tag(0)


def symbols(*rows):
    return struct.pack("<H", len(rows)) + b"".join(struct.pack("<H", cid) + name.encode() + b"\0" for cid, name in rows)


def source(tmp_path, text_flags=b"\x00\x80", keys="$INSIDE_SEARCH_AREA\tINSIDE OBJECTIVE AREA\n$Other\tNo\n",
           symbol=ui.SYMBOL_STEM + "777"):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text(keys, encoding="utf-16")
    tags = [(69, b"\x08\0\0\0"), (2, b"\x05\x00unrelated art"), (32, b"\x0a\x00ring" + BACKDROP + b"\xa5art"),
            (37, text(11, text_flags)), (39, sprite(12, 11)), (39, sprite(20, 10, 12)),
            (82, b"FO76 HUD code"), (76, symbols((20, symbol), (0, "HUDMenu"))), (26, b"stage placement"), (1, b"")]
    (interface / ui.SOURCE).write_bytes(swf(tags))
    return interface.parent


def test_movie_keeps_only_the_clip_closure_under_unique_classes(tmp_path):
    output = tmp_path / "output"
    manifest = ui.convert_quest_area_ui(source(tmp_path), output)
    data = (output / ui.OUTPUT / ui.MOVIE).read_bytes()
    tags = swf_tags(data)

    defined = sorted(struct.unpack_from("<H", payload)[0] for code, payload in tags if code in (32, 37, 39))
    assert defined == [10, 11, 12, 20]
    assert [payload for code, payload in tags if code == 32] == [b"\x0a\x00ring" + BACKDROP + b"\xa5art"]
    assert manifest["backdrop_fills_cleared"] == 0
    assert not any(code in (2, 26) for code, _ in tags)
    assert [code for code, _ in tags][-3:] == [76, 1, 0]
    assert [payload for code, payload in tags if code == 37] == [text(11).replace(b"$Futura_Bold", b"$MAIN_Font_Bold")]
    assert ui.symbol_ids(tags) == {ui.CLIP_CLASS: 20, ui.ROOT_CLASS: 0}
    assert native_runtime.unbacked_symbol_classes(data) == []
    assert manifest["symbol"] == ui.SYMBOL_STEM + "777"
    assert manifest["files"][0]["path"] == (ui.OUTPUT / ui.MOVIE).as_posix()

    translations = (ui.translation_path(output)).read_bytes().decode("utf-16")
    assert "$INSIDE_SEARCH_AREA\tINSIDE OBJECTIVE AREA" in translations
    assert "$Other" not in translations
    assert ui.convert_quest_area_ui(source(tmp_path / "again"), output) == manifest
