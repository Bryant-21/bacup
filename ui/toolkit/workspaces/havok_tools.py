"""Havok tool workspaces — Annotation Extractor, HKX Packer, HKX Converter."""

from imgui_bundle import imgui, icons_fontawesome_6 as fa

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.animation.annotation_extractor import AnnotationExtractorTool
from ui.tools.animation.hkx_packer import HKXPackerTool
from ui.tools.animation.hkx_viewer import HKXViewerTool
from ui.tools.animation.havok_converter import HavokConverterTool


class AnnotationExtractorWorkspace(ToolWorkspace):
    name = "Annotation Extractor"
    icon = "ANN"
    id = "annotation_extract"
    tool_class = AnnotationExtractorTool


class HKXPackerWorkspace(ToolWorkspace):
    name = "HKX Packer"
    icon = "HKX"
    id = "hkx_packer"
    tool_class = HKXPackerTool


class HKXViewerWorkspace(ToolWorkspace):
    name = "HKX Viewer"
    icon = "HKV"
    id = "hkx_viewer"
    tool_class = HKXViewerTool

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        def _btn(icon: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        if _btn(fa.ICON_FA_FOLDER_OPEN):
            path = self._tool.open_file_dialog()
            if path:
                self._tool.open_file(path)
        imgui.set_item_tooltip("Open HKX/XML")

        imgui.same_line()

        no_doc = not getattr(self._tool, "_xml_text", "")
        if no_doc:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_FLOPPY_DISK):
            path = self._tool.save_file_dialog()
            if path:
                self._tool.save_file(path)
        imgui.set_item_tooltip("Save As HKX/XML")
        if no_doc:
            imgui.end_disabled()

    def draw_menu(self) -> None:
        if imgui.begin_menu("File"):
            if imgui.menu_item("Open...", "Ctrl+O", False)[0]:
                path = self._tool.open_file_dialog()
                if path:
                    self._tool.open_file(path)
            can_save = bool(getattr(self._tool, "_xml_text", ""))
            if not can_save:
                imgui.begin_disabled()
            if imgui.menu_item("Save As...", "Ctrl+S", False)[0]:
                path = self._tool.save_file_dialog()
                if path:
                    self._tool.save_file(path)
            if not can_save:
                imgui.end_disabled()
            imgui.end_menu()
        if self._view_helper:
            self._view_helper.draw([f"{self.name}##{self.id}"])

    def open_file(self, path: str) -> None:
        self._tool.open_file(path)


class HKXConverterWorkspace(ToolWorkspace):
    name = "HKX Converter"
    icon = "HKC"
    id = "hkx_converter"
    tool_class = HavokConverterTool
