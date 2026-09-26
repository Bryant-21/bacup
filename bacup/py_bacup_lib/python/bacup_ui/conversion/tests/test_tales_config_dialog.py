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
