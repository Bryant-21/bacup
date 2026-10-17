import hashlib
import json
import struct

import pytest

from creation_lib.swf import native_runtime

from bacup_lib import inspect_ui as ui
from bacup_lib.legendary_perks_ui import imports, swf_tags
from bacup_lib.tests.catalog_schema_validation import validate_catalog

FIELD = b"\x01\x00\x08\x00\x01\x08$ChowderHead\0\xc8\x00\0"
# FO76's heading field and the sprite that holds it: the card's panel art.
PANEL_FIELD = struct.pack("<H", 431) + b"\x08\x00\x01\x08" + ui.PANEL_LABEL + b"\0\xc8\x00\0"
PANEL_SPRITE = (struct.pack("<HH", 432, 1) + struct.pack("<H", (26 << 6) | 5)
                + b"\x02\x01\x00" + struct.pack("<H", 431))
ROOT_SPRITE = (struct.pack("<HH", 447, 1) + struct.pack("<H", (26 << 6) | 15)
               + b"\x22\x01\x00" + struct.pack("<H", 439) + b"Header_mc\0")


def symbol_class(entries):
    return struct.pack("<H", len(entries)) + b"".join(
        struct.pack("<H", character) + name.encode() + b"\0" for character, name in entries)


def movie(dependencies=(), fields=(), symbols=((447, "ExamineMenu"),), sprites=()):
    tags = [(69, b"\x08\x00\x00\x00")]
    tags += [(71, name.encode() + b"\0\1\0\0\0") for name in dependencies]
    tags += [(37, field) for field in fields]
    tags += [(39, sprite) for sprite in sprites]
    tags += [(2, b"artwork"), (82, b"original code")]
    if symbols:
        tags.append((76, symbol_class(symbols)))
    tags += [(26, b"BaseInstance placement"), (1, b""), (0, b"")]
    body = b"\x08\x00\x00\x1e\x01\x00" + b"".join(
        struct.pack("<HI", (code << 6) | 63, len(data)) + data for code, data in tags)
    return b"FWS\x0f" + struct.pack("<I", len(body) + 8) + body


def source(tmp_path, keys=ui.TRANSLATION_KEYS):
    interface = tmp_path / "source/interface"
    interface.mkdir(parents=True)
    (interface / "examinemenu.swf").write_bytes(movie(
        ["fonts_en.swf", "BSButtonHintBar.swf", "PerksLibrary_Small.swf", "CurrencyIconLibrary.swf"],
        [FIELD, PANEL_FIELD], sprites=[PANEL_SPRITE, ROOT_SPRITE]))
    (interface / "fonts_en.swf").write_bytes(movie(fields=[FIELD], symbols=()))
    (interface / "currencyiconlibrary.swf").write_bytes(movie(["fonts_en.swf"], symbols=()))
    # Never valid movies: the converter must not follow the workbench-only imports.
    (interface / "perkslibrary_small.swf").write_bytes(b"perks")
    (interface / "bsbuttonhintbar.swf").write_bytes(b"hints")
    lines = "".join(f"{key}\t{key.strip('$').title()}\r\n" for key in keys if key != "$acc") + "$acc\tV.A.T.S. Accuracy\r\n"
    (interface / "translate_en.txt").write_bytes(b"\xff\xfe" + lines.encode("utf-16-le"))
    for language, text in (("de", "V.A.T.S.-Präzision"), ("zhhant", "V.A.T.S.準度"), ("ko", "V.A.T.S. 명중률")):
        (interface / f"translate_{language}.txt").write_bytes(
            b"\xff\xfe" + f"$dmg\tx\r\n$acc\t{text}\r\n".encode("utf-16-le"))
    return interface


@pytest.fixture
def compiler(monkeypatch):
    calls = []

    def replace(data, sources, dependencies):
        calls.append((data, sources, list(dependencies)))
        return data

    monkeypatch.setattr(native_runtime, "replace_as3_classes", replace)
    monkeypatch.setattr(native_runtime, "rename_as3_classes", lambda data, prefix, keep: data)
    monkeypatch.setattr(native_runtime, "patch_as3_method", lambda data, *args: data)
    return calls


def test_converts_movie_libraries_manifest_and_translations(tmp_path, compiler):
    interface = source(tmp_path)
    curve_path = interface.parent / "misc/curvetables/json/Armor/ArmorMods/Unique/Armor_LastBastion_AccuracyBonus.json"
    curve_path.parent.mkdir(parents=True)
    curve = [{"x": 0, "y": 12}, {"x": 1, "y": 0}]
    curve_path.write_text(json.dumps({"curve": curve}), encoding="utf-8")
    output = tmp_path / "data"
    manifest = ui.convert_inspect_ui(interface.parent, output)
    generated = output.parent / "F4SE/Plugins/B21_TalesFromAppalachia"
    bonuses = json.loads((generated / "EquipmentBonuses/SeventySix.esm.json").read_text(encoding="utf-8"))
    assert bonuses == {"version": 1, "curves": {"lastBastion": {"points": curve}}}
    validate_catalog("EquipmentBonuses", bonuses)
    assert json.loads((generated / "EquipmentBonusIcons.json").read_text(encoding="utf-8")) == {
        "schema_version": 1, "icons": {}}

    (_, meter_sources, _), (data, sources, dependencies) = compiler
    assert "SetCondition" in meter_sources["ExamineMenu"]
    assert list(sources) == ["ExamineMenu", "ItemCard_ItemHealthEntry"]
    assert "public class ExamineMenu" in sources["ExamineMenu"]
    tags = swf_tags(data)
    assert (26, b"BaseInstance placement") not in tags and (2, b"artwork") in tags
    assert (76, symbol_class([(0, "ExamineMenu"), (432, ui.PANEL_CLASS), (439, ui.FOOTER_CLASS)])) in tags
    assert imports(data) == ["fonts_en.swf", "CurrencyIconLibrary.swf"]
    assert len(dependencies) == 2

    folder = output / ui.OUTPUT
    assert imports((folder / "inspectcard.swf").read_bytes()) == [
        "fonts_en.swf", "CurrencyIconLibrary.swf"]
    assert sorted(path.name for path in folder.iterdir()) == [
        "b21_inspect_fonts.swf", "bartercard.swf", "conditionmeter.swf", "conversion.json",
        "currencyiconlibrary.swf", "fonts_en.swf", "inspectcard.swf"]
    for name in ("inspectcard.swf", "fonts_en.swf"):
        fields = [payload for code, payload in swf_tags((folder / name).read_bytes()) if code == 37]
        assert fields[0] == FIELD.replace(b"$ChowderHead\0", b"$MAIN_Font\0")
    assert [payload for code, payload in swf_tags((folder / "inspectcard.swf").read_bytes())
            if code == 37][1:] == [PANEL_FIELD]
    assert manifest["menu"] == "Interface/B21/TalesFromAppalachia/Inspect/inspectcard.swf"
    assert manifest["health_entry_sha256"] == hashlib.sha256(ui.HEALTH_ENTRY.read_bytes()).hexdigest()
    for entry in manifest["files"]:
        generated = output / entry["path"]
        assert hashlib.sha256(generated.read_bytes()).hexdigest() == entry["sha256"]
        import_base = generated.parent
        for imported in imports(generated.read_bytes()):
            assert (import_base / imported.lower()).is_file(), f"{generated.name}: {imported} does not resolve"
    assert json.loads((folder / "conversion.json").read_text(encoding="utf-8")) == manifest
    table = (ui.translation_path(output)).read_bytes().decode("utf-16")
    assert "$ATTACKMODE\tAttackmode" in table and "$CURRENT MODS\tCurrent Mods" in table
    assert "$StackWeight\tStackweight" in table
    # FO76 relabels FO4's own accuracy row, so each language keeps FO76's text for it.
    assert "$acc\tV.A.T.S. Accuracy" in table
    localized = {path.name: path.read_bytes().decode("utf-16") for path in ui.translation_path(output).parent.glob("*.txt")}
    assert localized["B21_TalesFromAppalachia_de.txt"] == "$acc\tV.A.T.S.-Präzision\r\n"
    assert localized["B21_TalesFromAppalachia_cn.txt"] == "$acc\tV.A.T.S.準度\r\n"
    assert set(localized) == {"B21_TalesFromAppalachia_en.txt", "B21_TalesFromAppalachia_de.txt",
                              "B21_TalesFromAppalachia_cn.txt"}

    assert ui.convert_inspect_ui(interface.parent, output) == manifest
