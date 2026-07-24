from __future__ import annotations

from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.world_viewer.app import WorldViewerApp


_NS = "##world_viewer"


class WorldViewerWorkspace(BaseWorkspace):
    id = "world_viewer"
    name = "World Viewer"
    icon = "WRLD"
    user_guide_body = """
Load a worldspace to browse its cells and placed references in the 3D viewport.
Use the World panel to pick a worldspace and bounds, then inspect layers, selection, and render stats in the side panels.
"""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings=toolkit_settings)
        self.app = WorldViewerApp(toolkit_settings=toolkit_settings)
        self._app = self.app

    def get_dockable_windows(self):
        return [
            make_window(f"World{_NS}", "LeftDock"),
            make_window(f"Viewport{_NS}", "MainDockSpace"),
            make_window(f"Layers{_NS}", "RightDock"),
            make_window(f"Selection{_NS}", "RightDock"),
            make_window(f"Render{_NS}", "BottomDock"),
            make_window(f"Stats{_NS}", "BottomDock"),
        ]

    def initialize(self) -> None:
        self._bind_panels(
            {
                f"World{_NS}": self._draw_world_panel,
                f"Viewport{_NS}": self._draw_viewport,
                f"Layers{_NS}": self._draw_layers_panel,
                f"Selection{_NS}": self._draw_selection_panel,
                f"Render{_NS}": self._draw_render_panel,
                f"Stats{_NS}": self._draw_stats_panel,
            }
        )
        self._initialized = True

    def get_settings_defaults(self) -> dict:
        return {
            "game": "fo4",
            "plugin_paths": [],
            "data_paths": [],
            "archive_paths": [],
            "worldspace": "",
            "min_x": -1,
            "min_y": -1,
            "max_x": 1,
            "max_y": 1,
        }

    def apply_settings(self, settings: dict) -> None:
        self.app.apply_settings(settings)

    def collect_settings(self) -> dict:
        return self.app.collect_settings()

    def resolve_game_paths(self) -> dict:
        return self.app.resolve_game_paths()

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return

    def cleanup(self) -> None:
        self.app.cleanup()

    def _draw_world_panel(self) -> None:
        from ui.world_viewer.panels import world_panel

        if imgui.begin(f"World{_NS}"):
            world_panel.draw(self.app)
        imgui.end()

    def _draw_viewport(self) -> None:
        if imgui.begin(f"Viewport{_NS}"):
            summary = self.app.viewport_summary()
            bounds = summary["bounds"]
            title = summary["worldspace"] or "Unloaded"
            imgui.text(
                f"{title}  [{getattr(bounds, 'min_x', -1)}, {getattr(bounds, 'min_y', -1)}] "
                f"to [{getattr(bounds, 'max_x', 1)}, {getattr(bounds, 'max_y', 1)}]"
            )
            imgui.separator()
            for batch in summary["batches"]:
                if isinstance(batch, dict):
                    kind = batch.get("kind", "batch")
                    count = batch.get("instance_count", 0)
                    imgui.text(f"{kind}: {count}")
            counts = summary["counts"]
            visible_instances = counts.get("visible_instances")
            if visible_instances is not None:
                imgui.text(f"visible_instances: {visible_instances}")
            timings = summary["timings_ms"]
            culling_ms = timings.get("culling")
            if culling_ms is not None:
                imgui.text(f"culling: {culling_ms} ms")
            if summary["error_message"]:
                imgui.text_wrapped(summary["error_message"])
        imgui.end()

    def _draw_layers_panel(self) -> None:
        from ui.world_viewer.panels import layers_panel

        if imgui.begin(f"Layers{_NS}"):
            layers_panel.draw(self.app)
        imgui.end()

    def _draw_selection_panel(self) -> None:
        from ui.world_viewer.panels import selection_panel

        if imgui.begin(f"Selection{_NS}"):
            selection_panel.draw(self.app)
        imgui.end()

    def _draw_render_panel(self) -> None:
        from ui.world_viewer.panels import render_panel

        if imgui.begin(f"Render{_NS}"):
            render_panel.draw(self.app)
        imgui.end()

    def _draw_stats_panel(self) -> None:
        from ui.world_viewer.panels import stats_panel

        if imgui.begin(f"Stats{_NS}"):
            stats_panel.draw(self.app)
        imgui.end()
