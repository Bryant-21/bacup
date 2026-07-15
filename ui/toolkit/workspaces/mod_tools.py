"""Mod tool workspaces — SubGraph Maker, archive tools, Archlist Creator, Folder Renamer, Modlist Merger."""

from __future__ import annotations

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.animation.subgraph_maker import SubGraphMakerTool
from ui.tools.assets.bsa_extractor import BSAExtractorTool
from ui.tools.assets.archlist_creator import ArchlistCreatorTool
from ui.tools.assets.folder_renamer import FolderRenamerTool
from ui.tools.assets.mass_bsa import MassBSATool
from ui.tools.assets.modlist_merger import ModlistMergerTool


class SubGraphMakerWorkspace(ToolWorkspace):
    name = "SubGraph Maker"
    icon = "SGM"
    id = "subgraph_maker"
    tool_class = SubGraphMakerTool


class BSAExtractorWorkspace(ToolWorkspace):
    name = "BSA Extractor"
    icon = "BSA"
    id = "bsa_extractor"
    tool_class = BSAExtractorTool

    def initialize(self) -> None:
        # BSAExtractorTool needs direct access to toolkit settings for game path resolution
        if self._toolkit_settings:
            self._tool._toolkit_settings = self._toolkit_settings
        super().initialize()


class MassBSAWorkspace(ToolWorkspace):
    name = "Mass BSA"
    icon = "BSA+"
    id = "mass_bsa"
    tool_class = MassBSATool

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        from imgui_bundle import imgui, icons_fontawesome_6 as fa

        if icon_font:
            imgui.push_font(icon_font, icon_font.legacy_size)
        clicked = imgui.button(getattr(fa, "ICON_FA_FOLDER_OPEN", "Open"))
        if icon_font:
            imgui.pop_font()
        imgui.set_item_tooltip("Open MO2 mods folder")
        if clicked:
            self._tool.open_folder_dialog()


class ArchlistCreatorWorkspace(ToolWorkspace):
    name = "Archlist Creator"
    icon = "ARC"
    id = "archlist_creator"
    tool_class = ArchlistCreatorTool


class FolderRenamerWorkspace(ToolWorkspace):
    name = "Folder Renamer"
    icon = "FLD"
    id = "folder_renamer"
    tool_class = FolderRenamerTool


class ModlistMergerWorkspace(ToolWorkspace):
    name = "Modlist Merger"
    icon = "MLG"
    id = "modlist_merger"
    tool_class = ModlistMergerTool
