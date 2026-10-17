from __future__ import annotations

from ui.tools.materials.v22_upgrader import MaterialV22UpgraderTool, parse_worldspaces


def test_parse_worldspaces_splits_commas_and_spaces():
    assert parse_worldspaces("") == []
    assert parse_worldspaces(" Commonwealth, DLC03FarHarbor  NukaWorld,,") == [
        "Commonwealth", "DLC03FarHarbor", "NukaWorld"]


def test_options_default_writes_sidecars_for_every_worldspace():
    options = MaterialV22UpgraderTool()._options()
    assert options.terrain_sidecars is True
    assert options.terrain_worldspaces == []


def test_options_carry_worldspaces_and_no_sidecars():
    tool = MaterialV22UpgraderTool()
    tool._include_terrain = True
    tool._worldspaces = "Commonwealth"
    assert tool._options().terrain_worldspaces == ["Commonwealth"]
    tool._terrain_sidecars = False
    assert tool._options().terrain_sidecars is False
