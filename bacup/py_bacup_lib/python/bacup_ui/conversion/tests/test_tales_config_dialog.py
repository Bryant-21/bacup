from __future__ import annotations

from bacup_lib import tales_config as tc
from bacup_ui.conversion.widgets.tales_config_dialog import TalesConfigDialog


def test_dialog_starts_from_installed_ini_and_saves_each_edit(tmp_path):
    ini = tmp_path / tc.INI_RELATIVE_PATH
    ini.parent.mkdir(parents=True)
    ini.write_text("[General]\niVersion=2\n[HUD]\nbCrosshair=0\n", encoding="utf-8")
    saved: list[dict[str, str]] = []
    dialog = TalesConfigDialog(saved.append)

    dialog.open(tmp_path, {"HUD/bQuickLoot": "0"})

    crosshair = next(s for s in tc.load_schema().settings() if s.id == "HUD/bCrosshair")
    assert tc.read_toggle(dialog._ini, crosshair) is False
    assert dialog._ini["hud"]["bquickloot"] == "0"

    dialog._write(crosshair, "1")

    assert saved[-1] == {"HUD/bQuickLoot": "0", "HUD/bCrosshair": "1"}
    assert ini.read_text(encoding="utf-8").endswith("bCrosshair=0\n")


def test_picking_an_own_color_selects_it_but_reset_does_not(tmp_path):
    saved: list[dict[str, str]] = []
    dialog = TalesConfigDialog(saved.append)
    dialog.open(tmp_path, {"PipBoy/iQuickBoyColor": "0"})
    own = next(s for s in tc.load_schema().settings() if s.id == "PipBoy/iColor")

    dialog._write(own, own.default)
    assert saved[-1] == {"PipBoy/iQuickBoyColor": "0", "PipBoy/iColor": "7BAAD9"}

    dialog._write(own, "E4E6A4", picked=True)
    assert saved[-1] == {"PipBoy/iQuickBoyColor": "1", "PipBoy/iColor": "E4E6A4"}
    assert tc.read_color(dialog._ini, own) == 0xE4E6A4
    assert dialog._ini["pipboy"]["iquickboycolor"] == "1"


def test_search_lists_matches_from_every_page_under_page_and_group(tmp_path):
    dialog = TalesConfigDialog(lambda edits: None)
    dialog.open(tmp_path, {})

    dialog._search = "area loot"
    groups = dict(dialog._groups())
    assert [s.key for s in groups["Looting › Area Loot"]] == [
        "bEnabled", "bQuickLoot", "bHighlight", "bPlayerKillsOnly", "fDistance", "fPollSeconds"]

    dialog._search = "zzqx"
    assert dialog._groups() == []

    dialog._search, dialog._page = "", 0
    assert [title for title, _ in dialog._groups()] == ["Status HUD", "HUD Rewards"]
