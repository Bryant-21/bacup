import json
import struct
import zlib

from bacup_lib import casino_ui as ui
from bacup_lib.legendary_perks_ui import swf_tags
from creation_lib.swf import native_runtime


def swf(tags):
    body = b"\x08\x00\x00\x1e\x01\x00"
    for code, payload in tags + [(0, b"")]:
        body += struct.pack("<HI", code << 6 | 63, len(payload)) + payload
    return b"CWS\x29" + struct.pack("<I", len(body) + 8) + zlib.compress(body)


def source(tmp_path):
    root = tmp_path / "source"
    interface = root / "interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text(
        "$CASINO_HEADER_ROULETTE\tPICK A SPOT TO PLACE A BET\n$PLACE_BET\tPLACE BET\n", encoding="utf-16")
    imports = {
        "casinomenu.swf": "roulettegame.swf",
        "roulettegame.swf": "fonts_en.swf",
        "casinohudwidget.swf": "fonts_en.swf",
        "casinofanfare.swf": None,
        "fonts_en.swf": None,
    }
    for name, dependency in imports.items():
        tags = [(82, b"script"), (2, (name + " art").encode())]
        if dependency:
            tags.insert(0, (71, dependency.encode() + b"\0\1\0\0\0"))
        (interface / name).write_bytes(swf(tags))
    curves = root / "misc/curvetables/json/misc/expeditions"
    curves.mkdir(parents=True)
    for count in (3, 5):
        (curves / f"xpd_ac_slotmachinechances_{count}tumbler.json").write_text(
            json.dumps({"curve": [{"x": 0, "y": 0.5}, {"x": 1, "y": 1.0}]}))
    return root


def test_conversion_preserves_art_closure_result_movies_catalog_and_translations(tmp_path, monkeypatch):
    monkeypatch.setattr(native_runtime, "augment_as3_classes",
        lambda data, classes, dependencies: swf([
            (code, b"bridge" if code == 82 else payload)
            for code, payload in swf_tags(data) if code
        ]))
    monkeypatch.setattr(native_runtime, "patch_as3_method", lambda data, *args: data)
    root = source(tmp_path)
    output = tmp_path / "out"
    report = ui.convert_casino_ui(root, output)
    paths = {entry["path"] for entry in report["files"]}
    assert ui.OUTPUT.joinpath("casinohudwidget.swf").as_posix() in paths
    assert ui.OUTPUT.joinpath("casinofanfare.swf").as_posix() in paths
    assert swf_tags((output / ui.OUTPUT / ui.MENU).read_bytes())[-2] == (2, b"casinomenu.swf art")
    assert json.loads((output / ui.OUTPUT / "casino_catalog.json").read_text())["schema_version"] == 1
    assert "$CASINO_HEADER_ROULETTE" in (ui.translation_path(output)).read_text(encoding="utf-16")
