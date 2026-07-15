"""NIF tool workspaces — Collision Generator and NIF to FBX."""

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.meshes.collision_generator import CollisionGeneratorTool
from ui.tools.meshes.fbx_exporter import NifToFbxTool


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
