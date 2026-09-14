"""Standalone PlayStation converter workspace."""

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.playstation_converter import PlayStationConverterTool


class PlayStationConverterWorkspace(ToolWorkspace):
    name = "PlayStation Converter"
    icon = "PS"
    id = "playstation_converter"
    tool_class = PlayStationConverterTool
