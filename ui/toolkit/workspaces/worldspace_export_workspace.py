"""Worldspace Export workspace."""

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.worldspace_export import WorldspaceExportTool


class WorldspaceExportWorkspace(ToolWorkspace):
    name = "Worldspace Export"
    icon = "WRLD"
    id = "worldspace_export"
    tool_class = WorldspaceExportTool
