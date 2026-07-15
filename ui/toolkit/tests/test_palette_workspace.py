import sys
from unittest.mock import MagicMock

def test_palette_workspace_has_required_attributes():
    from ui.toolkit.workspaces.palette_workspace import PaletteWorkspace
    ws = PaletteWorkspace()
    assert ws.name == "Palette"
    assert ws.icon == "PAL"
    assert ws.id == "palette"

def test_palette_workspace_get_dockable_windows():
    from ui.toolkit.workspaces.palette_workspace import PaletteWorkspace
    ws = PaletteWorkspace()
    windows = ws.get_dockable_windows()
    assert len(windows) >= 1

def test_palette_workspace_settings_defaults():
    from ui.toolkit.workspaces.palette_workspace import PaletteWorkspace
    ws = PaletteWorkspace()
    defaults = ws.get_settings_defaults()
    assert "last_source_path" in defaults
    assert "n_zones" in defaults
    assert defaults["n_zones"] == 6
