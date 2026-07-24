from __future__ import annotations

from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.lodgen.app import LodgenApp


_NS = "##lodgen"


class LodgenWorkspace(BaseWorkspace):
    id = "lodgen"
    name = "LOD Generator"
    icon = "LOD"
    user_guide_body = """
Generate terrain, object, and tree LOD for a worldspace from the World, Terrain, Object, and Tree panels.
Configure the output settings, then run generation from the Generate panel and monitor progress.
"""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings=toolkit_settings)
        self.app = LodgenApp(toolkit_settings=toolkit_settings)
        self._app = self.app

    def get_dockable_windows(self):
        return [
            make_window(f"World{_NS}", "LeftDock"),
            make_window(f"Terrain{_NS}", "MainDockSpace"),
            make_window(f"Object{_NS}", "MainDockSpace"),
            make_window(f"Tree{_NS}", "MainDockSpace"),
            make_window(f"Generate{_NS}", "RightDock"),
        ]

    def initialize(self) -> None:
        self._bind_panels({
            f"World{_NS}": self._draw_world,
            f"Terrain{_NS}": self._draw_terrain,
            f"Object{_NS}": self._draw_object,
            f"Tree{_NS}": self._draw_tree,
            f"Generate{_NS}": self._draw_generate,
        })
        self._initialized = True

    def get_settings_defaults(self) -> dict:
        return {"game": "fo4", "output_dir": "", "worldspace": ""}

    def apply_settings(self, settings: dict) -> None:
        self.app.apply_settings(settings)

    def collect_settings(self) -> dict:
        return self.app.collect_settings()

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return
        self.app.poll()

    def cleanup(self) -> None:
        self.app.cleanup()

    def _draw_world(self) -> None:
        from ui.lodgen.panels import world_panel
        if imgui.begin(f"World{_NS}"):
            world_panel.draw(self.app)
        imgui.end()

    def _draw_terrain(self) -> None:
        from ui.lodgen.panels import terrain_panel
        if imgui.begin(f"Terrain{_NS}"):
            terrain_panel.draw(self.app)
        imgui.end()

    def _draw_object(self) -> None:
        from ui.lodgen.panels import object_panel
        if imgui.begin(f"Object{_NS}"):
            object_panel.draw(self.app)
        imgui.end()

    def _draw_tree(self) -> None:
        from ui.lodgen.panels import tree_panel
        if imgui.begin(f"Tree{_NS}"):
            tree_panel.draw(self.app)
        imgui.end()

    def _draw_generate(self) -> None:
        from ui.lodgen.panels import presets_panel
        if imgui.begin(f"Generate{_NS}"):
            presets_panel.draw(self.app)
        imgui.end()
