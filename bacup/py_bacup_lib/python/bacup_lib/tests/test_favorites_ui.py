import re
import struct
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import favorites_ui as ui

FO76_INTERFACE = Path(__file__).resolve().parents[5] / "extracted/fo76/interface"


def tag(code: int, payload: bytes) -> bytes:
    return struct.pack("<HI", code << 6 | 63, len(payload)) + payload


def place2(depth: int, character: int) -> bytes:
    return tag(26, struct.pack("<BHH", 0x02, depth, character))


def place_class(depth: int, class_name: bytes) -> bytes:
    return tag(70, struct.pack("<BBH", 0x20, 0x08, depth) + class_name + b"\0" + b"ItemList_mc\0")


def sprite(character: int, *children: bytes) -> bytes:
    return struct.pack("<HH", character, 1) + b"".join(children)


def movie(*tags: bytes) -> bytes:
    body = bytes([0b00001000, 0]) + b"\x00\x18\x01\x00" + b"".join(tags) + tag(1, b"") + tag(0, b"")
    return b"FWS\x0e" + struct.pack("<I", len(body) + 8) + body


def test_ring_disk_is_the_lowest_first_frame_shape():
    wheel = sprite(10, place2(4, 7), place_class(3, b"Shared.AS3.BSButtonHintBar"), place2(2, 5), place2(1, 9),
                   tag(1, b""), place2(0, 6), tag(0, b""))
    assert ui.ring_disk(wheel, {5, 6, 7}) == 5
    with pytest.raises(ValueError, match="disk"):
        ui.ring_disk(sprite(10, place2(1, 9), tag(1, b""), tag(0, b"")), {5})


def test_class_placements_are_dropped_from_the_inventory_panel_only():
    panel = sprite(3, place2(1, 1), place_class(6, b"MenuListComponent"), place2(7, 2), tag(1, b""), tag(0, b""))
    other = sprite(4, place_class(6, b"MenuListComponent"), tag(1, b""), tag(0, b""))
    result = ui.swf_tags(ui.without_class_placements(movie(tag(39, panel), tag(39, other)), 3))
    sprites = {struct.unpack_from("<H", payload)[0]: ui.sprite_tags(payload) for code, payload in result if code == 39}
    assert [code for code, _ in sprites[3]] == [26, 26, 1, 0]
    assert [code for code, _ in sprites[4]] == [70, 1, 0]


def test_bridge_renders_the_parity_contract():
    symbols = {"RadialMenu_fla.radialBackground_mc_2", "RadialMenu_fla.radialCenterGroup_mc_3",
               "RadialMenu_fla.radialTab_39", "PistolIcon"}
    bridges, classes = ui.bridge_sources(symbols, {"UI_Pistol": "PistolIcon"})
    assert set(bridges) == set(ui.CLASSES)
    assert classes["tab"] == "RadialMenu_fla.radialTab_39"
    source = bridges["RadialMenu"]
    assert "__B21_" not in source and "BACUP_ICON" not in source
    assert "public var WheelVersion:uint = 3;" in source
    for member in ("public function SetInventory(rows:Array)", "public function SetActions(value:Object)",
                   "public function Navigate(action:String)", "BGSCodeObj.assignQuickkey(uint(id), uint(slot))",
                   "BGSCodeObj.inspectQuickkey(uint(selected))", "BGSCodeObj.placeSurvivalTent()",
                   "BGSCodeObj.requestInventory()", 'action == "Click"', 'action == "Change"', 'action == "Inspect"',
                   'action == "Tent"', "new RadialMenu_fla.radialTab_39()"):
        assert member in source, member
    # Every optional native callback is checked before use, for movies paired with an older DLL.
    for callback in ("requestInventory", "assignQuickkey", "inspectQuickkey", "placeSurvivalTent", "PlaySound"):
        assert f"BGSCodeObj.{callback} != null" in source, callback


def test_hint_text_keys_resolve_from_a_translation_table(tmp_path):
    source = ui.bridge_sources({"RadialMenu_fla.radialBackground_mc_2", "RadialMenu_fla.radialCenterGroup_mc_3",
                                "RadialMenu_fla.radialTab_39"}, {})[0]["RadialMenu"]
    used = set(re.findall(r'"(\$[A-Za-z0-9_/]+)"', source))
    fo4_keys = {"$SELECT", "$INSPECT", "$BACK"}
    tales_keys = {"$B21_TFA_SurvivalTent"}
    assert used == set(ui.TEXT_KEYS) | fo4_keys | tales_keys
    packaged = (Path(ui.__file__).with_name("data") / "translations/B21_TalesFromAppalachia_en.txt").read_text(encoding="utf-8")
    assert all(f"\n{key}\t" in packaged for key in tales_keys)
    interface = tmp_path / "interface"
    interface.mkdir()
    rows = [f"{key}\t{key[1:]}" for key in ui.TEXT_KEYS] + ["$Use\tUSE"]
    (interface / "translate_en.txt").write_bytes("\r\n".join(rows).encode("utf-16"))
    assert ui.pick_translations(interface, ui.TEXT_KEYS, "Favorites wheel") == rows[:-1]


def test_real_wheel_compiles_the_parity_contract():
    if not (FO76_INTERFACE / ui.MENU).is_file():
        pytest.skip("FO76 source data unavailable")
    from creation_lib.swf import native_runtime

    source = (FO76_INTERFACE / ui.MENU).read_bytes()
    characters = {name: character for character, name in native_runtime.list_symbols(source)}
    symbol, disk = ui.wheel_symbol_and_disk(source)
    assert symbol == characters["RadialMenu"] and disk not in characters.values()
    converted = ui.keep_imports(ui.wheel_timeline(source), ("fonts_en.swf",))
    converted = ui.with_button_hints(ui.without_class_placements(converted, characters["SecureTradePlayerInventory"]))
    wheel = next(payload for code, payload in ui.swf_tags(converted)
                 if code == 39 and struct.unpack_from("<H", payload)[0] == symbol)
    assert ui.ring_disk(wheel, {disk}) == disk
    bridges, _ = ui.bridge_sources(set(characters), {})
    dependencies = [(FO76_INTERFACE / name).read_bytes() for name in ui.imports(converted)]
    converted = ui.verify_script_only(converted, native_runtime.replace_as3_classes(converted, bridges, dependencies))
    traits = {row["name"] for row in native_runtime.abc_class_outline(converted, "RadialMenu")["instance_traits"]}
    assert {"WheelVersion", "SetInventory", "SetActions", "Navigate", "ProcessUserEvent", "PointAt", "SetDetails",
            "SetWeaponIcons", "RingDisk", "ButtonHintBar_mc", "PlayerInventory_mc", "radialTab", "HitLayer_mc"} <= traits
    assert native_runtime.abc_class_outline(converted, "B21ButtonHints")["super"] == "flash.display.Sprite"


def test_workflow_only_runs_for_fo76_to_fo4(tmp_path, monkeypatch):
    from bacup_lib.workflows import unified

    (tmp_path / "source/interface").mkdir(parents=True)
    (tmp_path / "source/interface" / ui.MENU).write_bytes(b"source marker")
    calls = []
    monkeypatch.setattr(ui, "convert_favorites_ui", lambda source, output, **kwargs: calls.append((source, output, kwargs["source_plugin"], kwargs["target_data_dir"])) or {"files": [1, 2, 3]})
    context = SimpleNamespace(source_data_dir=tmp_path / "source", mod_path=tmp_path / "mod", source_plugin_path=tmp_path / "SeventySix.esm",
                              target_data_dir=tmp_path / "target")
    assert unified._convert_fo76_favorites_ui(SimpleNamespace(source_game="fo76", target_game="fo4"), context) == 3
    assert calls == [(tmp_path / "source", tmp_path / "mod/data", context.source_plugin_path, context.target_data_dir)]
    for source, target in [("skyrimse", "fo4"), ("fo76", "skyrimse"), ("fnv", "fo4")]:
        assert unified._convert_fo76_favorites_ui(SimpleNamespace(source_game=source, target_game=target), context) == 0
    assert len(calls) == 1
