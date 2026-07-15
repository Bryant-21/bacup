"""ToolWorkspace — base workspace that wraps a single BaseTool as a docked panel."""

from __future__ import annotations

import logging

from imgui_bundle import imgui

from creation_lib.ui.widgets.user_guide import UserGuide
from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.tools.base import BaseTool

_log = logging.getLogger("toolkit.tool_workspace")


class ToolWorkspace(BaseWorkspace):
    """Wraps a BaseTool as a first-class workspace with a single docked panel.

    Subclasses must define class-level attributes:
        name: str          — Display name (e.g. "DDS Inspector")
        icon: str          — Short activity bar label (e.g. "DDS")
        id: str            — Unique key (e.g. "dds_inspector")
        tool_class: type   — The BaseTool subclass to instantiate
    """

    tool_class: type[BaseTool]

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)
        self._tool: BaseTool = self.tool_class()

    # -- Workspace protocol --

    def get_dockable_windows(self):
        return [make_window(f"{self.name}##{self.id}", "MainDockSpace")]

    def get_required_addons(self) -> dict:
        return {}

    def get_user_guide(self) -> UserGuide | None:
        description = str(getattr(self._tool, "description", "") or "").strip()
        body = "\n".join(
            [
                f"# {self.name}",
                "",
                description or f"Use {self.name} from this workspace.",
                "",
                "## Quick Start",
                "",
                "1. Fill in the required fields in the tool panel.",
                "2. Choose the input and output paths requested by the tool.",
                "3. Run the tool and watch the status area for progress, errors, and results.",
                "",
                "## Output",
                "",
                "Results, warnings, and errors appear below the tool controls.",
            ]
        )
        return UserGuide(
            title=f"{self.name} User Guide",
            body=body,
            window_id=f"user_guide_{self.id}",
        )

    def initialize(self) -> None:
        if self._toolkit_settings:
            ws = self._toolkit_settings.get_workspace_settings(self.id)
            saved = ws.get(self._tool.tool_id, {})
            if saved:
                self._tool.apply_settings(saved)
        self._tool.initialize()
        self._bind_dockable_windows()
        self._initialized = True

    def _bind_dockable_windows(self) -> None:
        self._bind_panels({f"{self.name}##{self.id}": self._draw_panel})

    def _draw_panel(self) -> None:
        if imgui.begin(f"{self.name}##{self.id}"):
            self._tool.draw_content()
            self._draw_status()
        imgui.end()

    def _draw_status(self) -> None:
        """Render progress/error/result chrome below the tool content."""
        tool = self._tool
        if tool._running:
            imgui.spacing()
            imgui.separator()
            imgui.progress_bar(tool._progress, imgui.ImVec2(-1, 0), tool._status_msg)
            if imgui.button(f"Cancel##{self.id}"):
                tool._cancel_requested = True
        if tool._error_msg:
            imgui.spacing()
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.3, 0.3, 1.0))
            imgui.text_wrapped(tool._error_msg)
            imgui.pop_style_color()
            if imgui.small_button(f"Dismiss##{self.id}"):
                tool._error_msg = ""
        if tool._result_msg and not tool._running:
            imgui.spacing()
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.3, 1.0, 0.3, 1.0))
            imgui.text_wrapped(tool._result_msg)
            imgui.pop_style_color()

    def cleanup(self) -> None:
        self._tool.cleanup()

    # -- Settings --

    def get_settings_defaults(self) -> dict:
        return {self._tool.tool_id: self._tool.get_default_settings()}

    def apply_settings(self, settings: dict) -> None:
        saved = settings.get(self._tool.tool_id, {})
        if saved:
            self._tool.apply_settings(saved)

    def collect_settings(self) -> dict:
        return {self._tool.tool_id: self._tool.collect_settings()}
