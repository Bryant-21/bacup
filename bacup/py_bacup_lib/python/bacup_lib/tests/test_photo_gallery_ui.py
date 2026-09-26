import hashlib
import json
import struct

from bacup_lib import photo_gallery_ui as ui
from creation_lib.swf import native_runtime


def movie(script=b"stock", artwork=b"source-vector-art"):
    tags = []
    for code, payload in ((82, script), (2, artwork)):
        tags.append(struct.pack("<HI", (code << 6) | 63, len(payload)) + payload)
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(tags) + b"\0\0"
    return b"FWS\x0f" + struct.pack("<I", len(body) + 8) + body


def setup_source(tmp_path):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / ui.MENU).write_bytes(movie())
    (interface / "translate_en.txt").write_text(
        "$MainMenuPhotoGallery\tPHOTO GALLERY\n"
        "$MainMenuPhotoGalleryTooltip\tView photos you've taken\n"
        "$PhotomodeOutOfSpace\tUnused\n", encoding="utf-8")
    return interface


def test_converter_preserves_source_movie_and_emits_scaleform_and_map_contract(tmp_path, monkeypatch):
    interface = setup_source(tmp_path)
    source = (interface / ui.MENU).read_bytes()
    contract = json.loads((ui.RESOURCES / "presentation.json").read_text())
    generated_symbol = contract["generated_symbol_stems"][0] + "777"
    monkeypatch.setattr(native_runtime, "abc_class_names", lambda _: [contract["document_class"]])
    monkeypatch.setattr(
        native_runtime,
        "list_symbols",
        lambda _: list(enumerate([*contract["symbols"], generated_symbol])),
    )
    monkeypatch.setattr(ui, "remap_menu_fonts", lambda data: data)
    output = tmp_path / "converted"
    result = ui.convert_photo_gallery_ui(interface.parent, output)
    converted = output / ui.SCALEFORM_OUTPUT / ui.MENU
    assert (interface / ui.MENU).read_bytes() == source
    assert converted.read_bytes() == source
    assert result["source_sha256"] == hashlib.sha256(source).hexdigest()
    assert result["presentation_preserved"] is True
    assert generated_symbol in result["symbols"]
    assert json.loads((output / ui.MAP_OUTPUT / "presentation.json").read_text()) == result
    translations = (output / ui.TRANSLATIONS).read_text(encoding="utf-16")
    assert "$MainMenuPhotoGallery\tPHOTO GALLERY" in translations
    assert "$PhotomodeOutOfSpace" not in translations
