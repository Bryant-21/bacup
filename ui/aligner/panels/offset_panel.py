"""Offset panel — X/Y/Z sliders, FOV mult, and camera mode toggle."""
from __future__ import annotations

from imgui_bundle import imgui

from ui.aligner.scope_camera import CameraMode


class OffsetPanel:
    """Sliders for scope alignment offsets."""

    def __init__(self, app):
        self._app = app
        self.window_name = "Offsets##aligner"

    def draw(self):
        camera = self._app.camera

        imgui.begin(self.window_name)

        is_scope = camera.mode == CameraMode.SCOPE_VIEW
        if imgui.button("Scope View" if not is_scope else "Orbit View"):
            camera.mode = CameraMode.ORBIT if is_scope else CameraMode.SCOPE_VIEW
        imgui.same_line()
        mode_label = "SCOPE VIEW" if is_scope else "ORBIT"
        color = imgui.ImVec4(0.4, 0.9, 0.4, 1.0) if is_scope else imgui.ImVec4(0.6, 0.6, 0.6, 1.0)
        imgui.text_colored(color, mode_label)

        imgui.separator()
        imgui.spacing()

        # Offset sliders (only meaningful in scope view, but always editable)
        imgui.text("Camera Offsets")
        changed_x, camera.offset_x = imgui.slider_float(
            "X (left/right)", camera.offset_x, -20.0, 20.0, "%.2f",
        )
        changed_y, camera.offset_y = imgui.slider_float(
            "Y (forward/back)", camera.offset_y, -20.0, 20.0, "%.2f",
        )
        changed_z, camera.offset_z = imgui.slider_float(
            "Z (up/down)", camera.offset_z, -20.0, 20.0, "%.2f",
        )

        imgui.spacing()
        imgui.text("Field of View")
        changed_fov, camera.fov_deg = imgui.slider_float(
            "Base FOV", camera.fov_deg, 65.0, 105.0, "%.1f°",
        )
        changed_mult, camera.fov_mult = imgui.slider_float(
            "FOV Mult", camera.fov_mult, 0.1, 10.0, "%.4f",
        )
        effective_fov = camera.fov_deg / camera.fov_mult
        imgui.text_disabled(f"Effective: {effective_fov:.1f}° ({camera.fov_mult:.2f}x zoom)")

        imgui.spacing()
        if imgui.button("Reset All"):
            camera.reset_offsets()

        imgui.end()
