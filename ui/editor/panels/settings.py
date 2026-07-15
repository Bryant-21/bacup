"""Settings panel — popup window for editor configuration.

Accessible from View > Settings in the menu bar.
Provides navigation style, appearance, and data path management.
"""

import json
from pathlib import Path

from imgui_bundle import imgui

# Persistent settings file next to nifeditor.log
_SETTINGS_PATH = Path(__file__).resolve().parents[1] / "editor_settings.json"


def _load_settings() -> dict:
    """Load persistent settings from disk."""
    if _SETTINGS_PATH.exists():
        try:
            return json.loads(_SETTINGS_PATH.read_text(encoding="utf-8"))
        except Exception:
            pass
    return {}


def _save_settings(data: dict):
    """Save persistent settings to disk."""
    try:
        _SETTINGS_PATH.write_text(
            json.dumps(data, indent=2), encoding="utf-8"
        )
    except Exception:
        pass


class SettingsPanel:
    """imgui settings popup for editor preferences."""

    _NAV_LABELS = [
        "3ds Max  (Alt+MMB=orbit, MMB=pan)",
        "Blender  (MMB=orbit, Shift+MMB=pan)",
        "Default  (Ctrl+LMB=orbit, MMB=pan)",
    ]
    _NAV_KEYS = ["3dsmax", "blender", "default"]

    _OUTLINE_LABELS = ["Wireframe", "Glow", "None"]
    _OUTLINE_KEYS = ["wireframe", "glow", "none"]

    _LIGHTING_LABELS = ["Studio", "Dramatic", "Outdoor"]
    _LIGHTING_KEYS = ["studio", "dramatic", "outdoor"]

    # Light type (bulb color) — imported from lighting module
    from creation_lib.renderer.lighting import LIGHT_TYPE_KEYS, LIGHT_TYPE_LABELS
    _LIGHT_TYPE_KEYS = LIGHT_TYPE_KEYS
    _LIGHT_TYPE_LABELS = LIGHT_TYPE_LABELS

    def __init__(self, app):
        self.app = app
        self._visible = False
        self.window_name = "Settings"

        # Load persisted settings
        saved = _load_settings()

        # --- Shared scene settings (delegated to SceneSettings) ---
        from ui.mesh_workspace.scene_settings import SceneSettings
        self._scene_settings = SceneSettings(
            bg_color=list(saved.get("bg_color", [0.18, 0.18, 0.20])),
            grid_visible=saved.get("grid_visible", True),
            lighting_preset=saved.get("lighting_preset", "studio"),
            light_type=saved.get("light_type", "standard"),
            nav_style=saved.get("nav_style", "3dsmax"),
        )

        # Derive combo indices from scene settings
        self._nav_style_idx = (
            self._NAV_KEYS.index(self._scene_settings.nav_style)
            if self._scene_settings.nav_style in self._NAV_KEYS else 0
        )
        self._lighting_preset_idx = (
            self._LIGHTING_KEYS.index(self._scene_settings.lighting_preset)
            if self._scene_settings.lighting_preset in self._LIGHTING_KEYS else 0
        )
        self._light_type_idx = (
            self._LIGHT_TYPE_KEYS.index(self._scene_settings.light_type)
            if self._scene_settings.light_type in self._LIGHT_TYPE_KEYS else 0
        )

        # Apply shared settings on startup
        if hasattr(app, 'camera'):
            app.camera.set_nav_style(self._scene_settings.nav_style)
        self._toggle_grid(self._scene_settings.grid_visible)
        if hasattr(app, 'lighting'):
            app.lighting.set_preset(self._scene_settings.lighting_preset)
            app.lighting.set_light_type(self._scene_settings.light_type)
        # Push bg_color to renderer
        renderer = getattr(app, 'renderer', None)
        if renderer is not None:
            renderer.bg_color = tuple(self._scene_settings.bg_color)

        # --- Editor-specific settings ---
        self._fps_visible = saved.get("fps_visible", True)

        # Selection outline style
        saved_outline = saved.get("outline_style", "glow")
        self._outline_style_idx = (
            self._OUTLINE_KEYS.index(saved_outline)
            if saved_outline in self._OUTLINE_KEYS else 1
        )

        # Selection outline color (RGB 0-1)
        self._outline_color = saved.get("outline_color", [0.3, 0.6, 1.0])

        # File watching
        self._nif_reload_prompt = saved.get("nif_reload_prompt", True)

        # Lighting tuning preset — restore from saved settings
        from .toolbar_tbr import LIGHTING_PRESET_NAMES, apply_lighting_preset
        saved_tuning = saved.get("lighting_tuning",
                                 saved.get("tbr_preset", "Standard"))
        if saved_tuning in LIGHTING_PRESET_NAMES:
            tuning_idx = LIGHTING_PRESET_NAMES.index(saved_tuning)
        else:
            tuning_idx = 0
        app._lighting_tuning_idx = tuning_idx
        apply_lighting_preset(app, LIGHTING_PRESET_NAMES[tuning_idx])

    @property
    def scene_settings(self):
        """Access the underlying SceneSettings dataclass."""
        return self._scene_settings

    @property
    def _bg_color(self) -> list[float]:
        return self._scene_settings.bg_color

    @_bg_color.setter
    def _bg_color(self, value: list[float]):
        self._scene_settings.bg_color = value

    @property
    def _grid_visible(self) -> bool:
        return self._scene_settings.grid_visible

    @_grid_visible.setter
    def _grid_visible(self, value: bool):
        self._scene_settings.grid_visible = value

    @property
    def nif_reload_prompt(self) -> bool:
        return self._nif_reload_prompt

    @property
    def outline_style(self) -> str:
        return self._OUTLINE_KEYS[self._outline_style_idx]

    @property
    def outline_color(self) -> list[float]:
        return self._outline_color

    def show(self):
        self._visible = True

    def toggle(self):
        self._visible = not self._visible

    def draw(self):
        """Draw the settings popup window."""
        if not self._visible:
            return

        # Center the popup on first appearance
        io = imgui.get_io()
        imgui.set_next_window_size(imgui.ImVec2(500, 550), imgui.Cond_.first_use_ever)
        imgui.set_next_window_pos(
            imgui.ImVec2(io.display_size.x / 2, io.display_size.y / 2),
            imgui.Cond_.first_use_ever,
            imgui.ImVec2(0.5, 0.5),
        )

        flags = imgui.WindowFlags_.no_docking
        expanded, self._visible = imgui.begin("Settings##popup", True, flags)
        if not expanded:
            imgui.end()
            return

        # -- Controls --
        if imgui.collapsing_header("Controls", imgui.TreeNodeFlags_.default_open.value):
            changed, self._nav_style_idx = imgui.combo(
                "Navigation Style", self._nav_style_idx, self._NAV_LABELS
            )
            if changed:
                key = self._NAV_KEYS[self._nav_style_idx]
                if hasattr(self.app, 'camera'):
                    self.app.camera.set_nav_style(key)
                self._persist()

            # Show a quick reference for the active style
            nav = self._NAV_KEYS[self._nav_style_idx]
            imgui.spacing()
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0), "Quick Reference:")
            if nav == "3dsmax":
                self._hint("Alt + MMB Drag", "Orbit")
                self._hint("MMB Drag", "Pan")
                self._hint("Scroll", "Zoom")
            elif nav == "blender":
                self._hint("MMB Drag", "Orbit")
                self._hint("Shift + MMB Drag", "Pan")
                self._hint("Scroll", "Zoom")
            else:
                self._hint("Ctrl + LMB Drag", "Orbit")
                self._hint("MMB Drag", "Pan")
                self._hint("Scroll", "Zoom")
            self._hint("LMB Click", "Select")
            self._hint("RMB Click", "Context Menu")
            self._hint("Alt + LMB Drag", "Rotate Light")
            if nav == "blender":
                self._hint("G", "Move Gizmo")
                self._hint("R", "Rotate Gizmo")
                self._hint("S", "Scale Gizmo")
            else:
                self._hint("W", "Move Gizmo")
                self._hint("E", "Rotate Gizmo")
                self._hint("R", "Scale Gizmo")

        # -- Lighting --
        if imgui.collapsing_header("Lighting", imgui.TreeNodeFlags_.default_open.value):
            changed, self._lighting_preset_idx = imgui.combo(
                "Lighting Preset", self._lighting_preset_idx, self._LIGHTING_LABELS
            )
            if changed:
                key = self._LIGHTING_KEYS[self._lighting_preset_idx]
                if hasattr(self.app, 'lighting'):
                    self.app.lighting.set_preset(key)
                self._persist()

            changed, self._light_type_idx = imgui.combo(
                "Light Type", self._light_type_idx, self._LIGHT_TYPE_LABELS
            )
            if changed:
                key = self._LIGHT_TYPE_KEYS[self._light_type_idx]
                if hasattr(self.app, 'lighting'):
                    self.app.lighting.set_light_type(key)
                self._persist()

        # -- Appearance --
        if imgui.collapsing_header("Appearance"):
            col = imgui.ImVec4(self._bg_color[0], self._bg_color[1], self._bg_color[2], 1.0)
            changed, col = imgui.color_edit3("Background", col)
            if changed:
                self._bg_color = [col.x, col.y, col.z]
                renderer = getattr(self.app, 'renderer', None)
                if renderer is not None:
                    renderer.bg_color = tuple(self._bg_color)
                self._persist()

            imgui.spacing()
            changed, self._outline_style_idx = imgui.combo(
                "Selection Outline", self._outline_style_idx, self._OUTLINE_LABELS
            )
            if changed:
                self._persist()

            if self.outline_style != "none":
                col = imgui.ImVec4(self._outline_color[0], self._outline_color[1], self._outline_color[2], 1.0)
                changed, col = imgui.color_edit3("Outline Color", col)
                if changed:
                    self._outline_color = [col.x, col.y, col.z]
                    self._persist()

        # -- File Watching --
        if imgui.collapsing_header("File Watching", imgui.TreeNodeFlags_.default_open.value):
            changed, self._nif_reload_prompt = imgui.checkbox(
                "Prompt before reloading changed NIF files", self._nif_reload_prompt
            )
            if changed:
                self._persist()
            imgui.text_colored(
                imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
                "  When disabled, NIF files reload automatically.",
            )

        imgui.end()

    @staticmethod
    def _hint(key: str, action: str):
        """Draw a key -> action hint line."""
        imgui.text_colored(imgui.ImVec4(0.9, 0.9, 0.6, 1.0), f"  {key}")
        imgui.same_line(200)
        imgui.text(action)

    def _persist(self):
        """Save current settings to disk."""
        # Sync combo indices back into the shared SceneSettings dataclass
        self._scene_settings.nav_style = self._NAV_KEYS[self._nav_style_idx]
        self._scene_settings.lighting_preset = self._LIGHTING_KEYS[self._lighting_preset_idx]
        self._scene_settings.light_type = self._LIGHT_TYPE_KEYS[self._light_type_idx]

        existing = _load_settings()
        from .toolbar_tbr import LIGHTING_PRESET_NAMES
        tuning_idx = getattr(self.app, '_lighting_tuning_idx', 0)
        tuning_name = (LIGHTING_PRESET_NAMES[tuning_idx]
                       if tuning_idx < len(LIGHTING_PRESET_NAMES) else "Standard")
        # Merge shared scene settings + editor-specific settings
        existing.update(self._scene_settings.to_dict())
        existing.update({
            "fps_visible": self._fps_visible,
            "outline_style": self._OUTLINE_KEYS[self._outline_style_idx],
            "outline_color": self._outline_color,
            "lighting_tuning": tuning_name,
            "nif_reload_prompt": self._nif_reload_prompt,
        })
        # Remove old key on save
        existing.pop("tbr_preset", None)
        _save_settings(existing)

    def _toggle_grid(self, visible: bool):
        renderer = getattr(self.app, 'renderer', None)
        if renderer:
            renderer.grid_visible = visible
