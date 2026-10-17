from pathlib import Path
import re

import pytest

from bacup_lib import inventory_lists_ui as ui
from bacup_lib.inventory_icon_art import ICONS, catalog, glyph_class
from creation_lib.swf import native_runtime as native


@pytest.mark.parametrize("name", ["bartermenu.swf", "containermenu.swf", "pipboymenu.swf", "pipboy_invpage.swf"])
def test_stock_inventory_menus_patch_both_selection_accessors(name):
    stock = Path(__file__).resolve().parents[5] / "extracted/fo4/interface" / name
    if not stock.is_file():
        pytest.skip("FO4 interface data unavailable")
    movie = ui.patch_inventory_list(stock.read_bytes())
    getters = native.abc_disassemble(movie, ui.LIST, "get selectedIndex")
    setters = native.abc_disassemble(movie, ui.LIST, "set selectedIndex")
    assert len(getters) == len(setters) == 1
    assert any("CallProperty B21GetSelectedIndex (0)" in line for line in getters[0]["code"])
    assert any("CallPropVoid B21SelectEngineIndex (1)" in line for line in setters[0]["code"])
    classes = native.abc_class_names(movie)
    assert all(glyph_class(icon["name"]) in classes for icon in catalog())
    if name == "pipboy_invpage.swf":
        assert native.abc_disassemble(movie, "InvListEntry", "get B21TextFieldWidth")
        assert native.abc_disassemble(movie, "InvListEntry", "set B21TextFieldWidth")
    assert native.roundtrip_ok(movie)


def test_icon_drawer_tables_follow_the_glyph_catalog():
    icons = catalog()
    assert [icon["id"] for icon in icons] == list(range(len(icons)))
    assert all((ICONS / f"{icon['name']}.svg").is_file() for icon in icons)
    source = (ui.RESOURCES / "B21InventoryIcon.as").read_text(encoding="utf-8")
    glyphs = re.search(r"glyphs:Array = \[(.*?)\];", source, re.S).group(1)
    colors = re.search(r"colors:Array = \[(.*?)\];", source, re.S).group(1)
    assert re.findall(r'"(\w+)"', glyphs) == [glyph_class(icon["name"]) for icon in icons]
    assert re.findall(r"0x([0-9a-f]{6})", colors) == [icon["color"][1:].lower() for icon in icons]
