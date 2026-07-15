"""SceneSettings — shared scene configuration for all mesh workspaces.

Stores background color, grid visibility, lighting preset, navigation
style, and light type.  Provides serialization (to_dict / from_dict)
and an apply_to() method that pushes values into a live workspace app.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ui.mesh_workspace.base_app import MeshWorkspaceBase

# Canonical defaults — keep in sync with NIF editor's SettingsPanel
_DEFAULT_BG = [0.18, 0.18, 0.20]
_DEFAULT_LIGHTING = "studio"
_DEFAULT_LIGHT_TYPE = "standard"
_DEFAULT_NAV = "3dsmax"


@dataclass
class SceneSettings:
    """Shared scene appearance settings across mesh workspaces."""

    bg_color: list[float] = field(default_factory=lambda: list(_DEFAULT_BG))
    grid_visible: bool = True
    lighting_preset: str = _DEFAULT_LIGHTING
    light_type: str = _DEFAULT_LIGHT_TYPE
    nav_style: str = _DEFAULT_NAV

    # -- Serialization -------------------------------------------------------

    def to_dict(self) -> dict:
        return {
            "bg_color": list(self.bg_color),
            "grid_visible": self.grid_visible,
            "lighting_preset": self.lighting_preset,
            "light_type": self.light_type,
            "nav_style": self.nav_style,
        }

    @classmethod
    def from_dict(cls, data: dict) -> SceneSettings:
        return cls(
            bg_color=list(data.get("bg_color", _DEFAULT_BG)),
            grid_visible=data.get("grid_visible", True),
            lighting_preset=data.get("lighting_preset", _DEFAULT_LIGHTING),
            light_type=data.get("light_type", _DEFAULT_LIGHT_TYPE),
            nav_style=data.get("nav_style", _DEFAULT_NAV),
        )

    # -- Apply to live app ---------------------------------------------------

    def apply_to(self, app: MeshWorkspaceBase) -> None:
        """Push current settings into a live workspace app instance."""
        # Lighting preset
        lighting = getattr(app, "lighting", None)
        if lighting is not None:
            from creation_lib.renderer.lighting import LIGHTING_PRESETS, LIGHT_TYPE_PRESETS
            if self.lighting_preset in LIGHTING_PRESETS:
                lighting.set_preset(self.lighting_preset)
            if self.light_type in LIGHT_TYPE_PRESETS:
                lighting.set_light_type(self.light_type)

        # Grid visibility
        renderer = getattr(app, "renderer", None)
        if renderer is not None:
            renderer.grid_visible = self.grid_visible

        # Background color — stored on renderer for use during clear()
        if renderer is not None:
            renderer.bg_color = tuple(self.bg_color)

        # Navigation style
        camera = getattr(app, "camera", None)
        if camera is not None and hasattr(camera, "set_nav_style"):
            camera.set_nav_style(self.nav_style)
