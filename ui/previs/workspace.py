from __future__ import annotations

from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.previs.app import PrevisApp

_NS = "##previs"


class PrevisWorkspace(BaseWorkspace):
    id = "previs"
    name = "Previs and Precombines"
    icon = "PV"
    user_guide_body = """
Check a load order (MO2 profile, ModBox21 profile, or one plugin with its masters) for cells whose precombines or previs
the load order has broken, outdated or disabled. Each cell lists what must be rebuilt and which plugin caused it.
Nothing is written: this is the native, Creation-Kit-free check that the repair patch builds on.
"""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings=toolkit_settings)
        self.app = PrevisApp(toolkit_settings=toolkit_settings)
        self._app = self.app

    def get_dockable_windows(self):
        return [
            make_window(f"Load Order{_NS}", "LeftDock"),
            make_window(f"Findings{_NS}", "MainDockSpace"),
            make_window(f"Details{_NS}", "RightDock"),
        ]

    def initialize(self) -> None:
        self._bind_panels({
            f"Load Order{_NS}": self._draw_source,
            f"Findings{_NS}": self._draw_findings,
            f"Details{_NS}": self._draw_details,
        })
        self._initialized = True

    def get_settings_defaults(self) -> dict:
        return {
            "source_kind": "mo2",
            "mo2_profile_dir": "",
            "modbox_library": "",
            "modbox_profile_id": "",
            "plugin_path": "",
            "against": [],
            "include_base_conflicts": False,
            "restore_disabled": False,
            "unprecombined_enabled": False,
            "unprecombined_threshold": 5,
            "output_dir": "",
            "patch_name": "",
            "scope": "broken",
            "into_plugin": False,
            "plugin_type": "esp",
            "pack": True,
            "small_model_cull": False,
        }

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

    def _draw_source(self) -> None:
        from ui.previs.panels import source_panel
        if imgui.begin(f"Load Order{_NS}"):
            source_panel.draw(self.app)
        imgui.end()

    def _draw_findings(self) -> None:
        from ui.previs.panels import findings_panel
        if imgui.begin(f"Findings{_NS}"):
            findings_panel.draw(self.app)
        imgui.end()

    def _draw_details(self) -> None:
        from ui.previs.panels import details_panel
        if imgui.begin(f"Details{_NS}"):
            details_panel.draw(self.app)
        imgui.end()
