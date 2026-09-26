import hashlib

import pytest

from creation_lib.swf import native_runtime

from bacup_lib import fishing_ui, keypad_ui
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.tests.test_legendary_perks_ui import swf


@pytest.mark.parametrize("ui,convert,menus", [
    (fishing_ui, fishing_ui.convert_fishing_ui, ["fishingmenu.swf"]),
    (keypad_ui, keypad_ui.convert_keypad_ui, ["keypadmenu.swf"]),
])
def test_converted_menu_and_imported_fonts_use_fo4_aliases(tmp_path, monkeypatch, ui, convert, menus):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text("$DO_TIME\tTime\n$FISHING_CAST\tCast\n", encoding="utf-8")
    fonts = [(b"$ChowderHead", b"$MAIN_Font"), (b"$Brush_Script_Std", b"$MAIN_Font"),
             (b"$Futura_Bold", b"$MAIN_Font_Bold"), (b"$BerlinDemi", b"$MAIN_Font_Bold"),
             (b"$MAIN_Font_Bold", b"$MAIN_Font_Bold")]
    texts = [b"\x01\x00\x08\x00\x01\x08" + font + b"\0\xc8\x00\0" for font, _ in fonts]
    tags = [(37, text) for text in texts] + [(2, b"art-$Futura_Bold\0"), (82, b"code-$ChowderHead\0")]
    original = swf(tags)
    for menu in menus:
        (interface / menu).write_bytes(swf([(71, b"library.swf\0\1\0\0\0")] + tags))
    (interface / "library.swf").write_bytes(original)

    monkeypatch.setattr(native_runtime, "replace_as3_classes", lambda data, *_: data)
    monkeypatch.setattr(native_runtime, "augment_as3_classes", lambda data, *_: data)
    monkeypatch.setattr(native_runtime, "patch_as3_method", lambda data, *_: data)
    output = tmp_path / "output"
    manifest = convert(interface.parent, output)
    expected = [text.replace(before + b"\0", after + b"\0") for text, (before, after) in zip(texts, fonts)]
    for entry in manifest["files"]:
        data = (output / entry["path"]).read_bytes()
        converted = swf_tags(data)
        assert [payload for code, payload in converted if code == 37] == expected
        assert (2, b"art-$Futura_Bold\0") in converted
        assert (82, b"code-$ChowderHead\0") in converted
        assert entry["sha256"] == hashlib.sha256(data).hexdigest()
    assert (interface / "library.swf").read_bytes() == original
