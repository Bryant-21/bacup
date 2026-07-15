"""Workspace protocol and layout types for the toolkit host."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Protocol, runtime_checkable

from imgui_bundle import hello_imgui


@runtime_checkable
class Workspace(Protocol):
    """Interface that every toolkit workspace must implement."""

    name: str       # Display name shown in activity bar tooltip
    icon: str       # Single character or short label for activity bar
    id: str         # Unique key used in settings (e.g. "nif", "behavior")

    def get_dockable_windows(self) -> list[hello_imgui.DockableWindow]:
        """Return dockable windows for this workspace.

        Panel names MUST use ##<id> suffix for uniqueness:
          "Properties##nif", "Properties##behavior"
        Do NOT include shared panels (AI Chat, Log) — the host manages those.
        """
        ...

    def get_required_addons(self) -> dict:
        """Return immapp AddOnsParams flags this workspace needs.

        Example: {"with_node_editor": True}
        The host merges all workspace addons before immapp.run().
        """
        ...

    def initialize(self) -> None:
        """One-time setup after first frame (GL context available).

        Create GPU resources, compile shaders, load initial data.
        MUST call _bind_dockable_windows() to wire panel draw methods into
        the DockableWindow gui_functions registered in get_dockable_windows().
        """
        ...

    def draw_menu(self) -> None:
        """Render workspace-specific menu bar items.

        Called INSIDE the host's begin/end_main_menu_bar().
        Emit begin_menu()/end_menu() only — NOT begin/end_main_menu_bar().
        """
        ...

    def draw(self) -> None:
        """Per-frame updates: floating panels, modal dialogs, keybindings, polling.

        MUST early-return if self.active is False.
        MUST NOT process keybindings when inactive.
        MUST NOT draw docked panels here — those are drawn via DockableWindow
        gui_functions bound in _bind_dockable_windows().
        """
        ...

    def on_activate(self) -> None:
        """Workspace becomes active — show panels, resume watchers."""
        ...

    def on_deactivate(self) -> None:
        """Workspace loses focus — hide panels, pause expensive work.

        MUST NOT destroy GPU resources.
        """
        ...

    def cleanup(self) -> None:
        """App exit — release GPU resources, close DB connections."""
        ...

    def get_settings_defaults(self) -> dict:
        """Default settings for this workspace's section."""
        ...

    def apply_settings(self, settings: dict) -> None:
        """Apply settings loaded from toolkit_settings.json."""
        ...

    def collect_settings(self) -> dict:
        """Return current settings to be persisted."""
        ...

    def draw_settings(self) -> None:
        """Render workspace-specific settings UI inside the global Settings window.

        Called by SettingsWindow when the workspace's section is active.
        Default: no-op (workspace has no configurable settings).
        """
        ...

    def has_toolbar(self) -> bool:
        """Return True if this workspace provides a top icon toolbar.

        Default is False — workspaces that don't override this get no toolbar.
        The toolkit host uses getattr fallback, so this method is optional.
        """
        ...

    def draw_toolbar(self, icon_font=None) -> None:
        """Draw icon buttons for the top edge toolbar.

        Only called when has_toolbar() returns True.
        Must only call imgui button/icon functions — no begin/end window.
        icon_font: if provided, push it around each imgui.button() call only,
                   never around set_item_tooltip (which renders text).
        """
        ...
