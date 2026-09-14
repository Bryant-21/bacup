"""NIF tool workspaces — Collision Generator and NIF to FBX."""

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.meshes.collision_generator import CollisionGeneratorTool
from ui.tools.meshes.fbx_exporter import NifToFbxTool
from ui.tools.meshes.nif_validation_tool import NifValidationReportTool


class NIFValidationReportWorkspace(ToolWorkspace):
    name = "NIF Validator"
    icon = "VAL"
    id = "nif_validation_report"
    tool_class = NifValidationReportTool

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)
        self._tool._toolkit_settings = toolkit_settings


class NIFCollisionWorkspace(ToolWorkspace):
    name = "NIF Collision Generator"
    icon = "COL"
    id = "nif_collision"
    tool_class = CollisionGeneratorTool


class NIFToFBXWorkspace(ToolWorkspace):
    name = "NIF to FBX"
    icon = "FBX"
    id = "nif_fbx"
    tool_class = NifToFbxTool
