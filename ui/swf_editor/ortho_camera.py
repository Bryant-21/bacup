"""2D orthographic camera with pan and zoom for the SWF canvas.

Coordinate space: SWF canvas (550x400). The camera maps this to
the viewport with pan offset and zoom level.
"""
from __future__ import annotations

from dataclasses import dataclass

import glm


@dataclass
class OrthoCamera:
    """2D pan/zoom camera for the SWF editor canvas."""

    # Pan offset in canvas pixels
    pan_x: float = 0.0
    pan_y: float = 0.0

    # Zoom level (1.0 = 100%)
    zoom: float = 1.0
    zoom_min: float = 0.25
    zoom_max: float = 16.0

    # Canvas size (SWF native)
    canvas_width: int = 550
    canvas_height: int = 400

    # Viewport size (ImGui region)
    viewport_width: int = 800
    viewport_height: int = 600

    # Snap
    snap_to_grid: bool = False
    grid_size: int = 10

    def get_projection(self) -> glm.mat4:
        """Get the orthographic projection matrix."""
        hw = self.viewport_width / (2.0 * self.zoom)
        hh = self.viewport_height / (2.0 * self.zoom)
        cx = self.canvas_width / 2.0 - self.pan_x / self.zoom
        cy = self.canvas_height / 2.0 - self.pan_y / self.zoom
        return glm.ortho(cx - hw, cx + hw, cy + hh, cy - hh, -1.0, 1.0)

    def get_view_matrix(self) -> glm.mat4:
        """View matrix (identity for 2D -- projection handles everything)."""
        return glm.mat4(1.0)

    def screen_to_canvas(self, sx: float, sy: float) -> tuple[float, float]:
        """Convert screen/viewport coordinates to canvas coordinates."""
        cx = (sx - self.viewport_width / 2.0) / self.zoom + self.canvas_width / 2.0 - self.pan_x / self.zoom
        cy = (sy - self.viewport_height / 2.0) / self.zoom + self.canvas_height / 2.0 - self.pan_y / self.zoom
        return cx, cy

    def canvas_to_screen(self, cx: float, cy: float) -> tuple[float, float]:
        """Convert canvas coordinates to screen/viewport coordinates."""
        sx = (cx - self.canvas_width / 2.0 + self.pan_x / self.zoom) * self.zoom + self.viewport_width / 2.0
        sy = (cy - self.canvas_height / 2.0 + self.pan_y / self.zoom) * self.zoom + self.viewport_height / 2.0
        return sx, sy

    def do_zoom(self, delta: float, mouse_x: float, mouse_y: float) -> None:
        """Zoom toward/away from mouse position.

        mouse_x, mouse_y: viewport-relative coordinates (0,0 = top-left of image widget).
        """
        old_zoom = self.zoom
        self.zoom *= 1.1 ** delta
        self.zoom = max(self.zoom_min, min(self.zoom_max, self.zoom))

        # Adjust pan to keep the point under the cursor stationary.
        # Convert mouse from viewport-relative to offset-from-center.
        cx = mouse_x - self.viewport_width / 2.0
        cy = mouse_y - self.viewport_height / 2.0
        zoom_ratio = self.zoom / old_zoom
        self.pan_x = cx - (cx - self.pan_x) * zoom_ratio
        self.pan_y = cy - (cy - self.pan_y) * zoom_ratio

    def do_pan(self, dx: float, dy: float) -> None:
        """Pan by screen-space delta."""
        self.pan_x += dx
        self.pan_y += dy

    def fit_canvas(self) -> None:
        """Reset view to fit the entire canvas in the viewport."""
        zoom_x = self.viewport_width / self.canvas_width
        zoom_y = self.viewport_height / self.canvas_height
        self.zoom = min(zoom_x, zoom_y) * 0.9  # 90% to show margin
        self.pan_x = 0.0
        self.pan_y = 0.0

    def handle_input(self, io) -> None:
        """Process ImGui IO for pan/zoom.

        Middle-drag = pan, scroll = zoom, H key = hand tool override.
        """
        if io.mouse_wheel != 0:
            self.do_zoom(io.mouse_wheel, io.mouse_pos.x, io.mouse_pos.y)

        if io.mouse_down[2]:  # middle button
            self.do_pan(io.mouse_delta.x, io.mouse_delta.y)
