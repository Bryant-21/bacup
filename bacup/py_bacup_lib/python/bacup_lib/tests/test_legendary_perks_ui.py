import json
import struct
import zlib

import pytest

from creation_lib.swf import native_runtime

from bacup_lib import legendary_perks_ui as ui


def swf(tags):
    body = b"\x08\x00\x00\x1e\x01\x00"
    for code, payload in tags + [(0, b"")]:
        body += struct.pack("<HI", code << 6 | 63, len(payload)) + payload
    return b"CWS\x25" + struct.pack("<I", len(body) + 8) + zlib.compress(body)


def source(tmp_path, dependency="library.swf"):
    root = tmp_path / "source"
    interface = root / "interface"
    interface.mkdir(parents=True)
    (interface / "translate_en.txt").write_text("$LegendaryPerks\tLegendary perks\n", encoding="utf-16")
    (interface / ui.MENU).write_bytes(swf([(71, dependency.encode() + b"\0\1\0\0\0"), (82, b"source-code"), (9, b"\0\0\0")]))
    (interface / "library.swf").write_bytes(swf([(82, b"library-code")]))
    return root


def compiler(data, sources, dependencies):
    assert set(sources) == {"LegendaryPerksMenu_fla.MainTimeline"}
    return swf([(code, b"bridge-code" if code == 82 else payload)
                for code, payload in ui.swf_tags(data) if code != 0])


@pytest.fixture(autouse=True)
def native_copy_patch(monkeypatch):
    def patch(data, class_name, method, pattern, replacement):
        assert (class_name, method) == ("LegendaryPerksMenu", "populateUpgradeModal")
        assert pattern[-1] == ["callproperty", "CloneObject", 1]
        assert replacement[-1] == ["callproperty", "B21CopyCard", 1]
        return data
    monkeypatch.setattr(native_runtime, "patch_as3_method", patch)


def test_converter_preserves_dependencies_and_art_and_emits_translations(tmp_path, monkeypatch):
    root = source(tmp_path)
    monkeypatch.setattr(native_runtime, "replace_as3_classes", compiler)
    output = tmp_path / "converted/data"
    manifest = ui.convert_legendary_perk_ui(root, output)
    assert len(manifest["files"]) == 2
    assert (output / ui.OUTPUT / "library.swf").read_bytes() == (root / "interface/library.swf").read_bytes()
    assert (ui.translation_path(output)).read_text(encoding="utf-16") == "$LegendaryPerks\tLegendary perks\n"
    assert json.loads((output / ui.OUTPUT / "conversion.json").read_text()) == manifest
    assert (root / "interface" / ui.MENU).read_bytes() != (output / ui.OUTPUT / ui.MENU).read_bytes()
    assert ui.convert_legendary_perk_ui(root, output) == manifest
