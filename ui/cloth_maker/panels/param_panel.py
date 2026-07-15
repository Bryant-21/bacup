"""Parameter panel — cloth parameter sliders and material presets.

Right-side panel for editing cloth parameters via direct havok_native calls
(no ClothEditor wrapper — self._editor is just a bool "loaded" sentinel).
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.param_panel")

# Material presets — stiffness scale factors relative to vanilla
_PRESETS = {
    "Silk": {"mass": 0.02, "stiffness_factor": 0.3, "damping": 0.9995, "friction": 0.15, "gravity_z": -686.7},
    "Cotton": {"mass": 0.05, "stiffness_factor": 0.6, "damping": 0.9998, "friction": 0.25, "gravity_z": -686.7},
    "Linen": {"mass": 0.07, "stiffness_factor": 0.7, "damping": 0.9998, "friction": 0.30, "gravity_z": -686.7},
    "Denim": {"mass": 0.10, "stiffness_factor": 0.9, "damping": 0.9999, "friction": 0.40, "gravity_z": -686.7},
    "Leather": {"mass": 0.15, "stiffness_factor": 1.2, "damping": 0.9999, "friction": 0.50, "gravity_z": -686.7},
    "Heavy Wool": {"mass": 0.12, "stiffness_factor": 1.0, "damping": 0.9999, "friction": 0.35, "gravity_z": -686.7},
    "Chain": {"mass": 0.25, "stiffness_factor": 1.5, "damping": 0.9990, "friction": 0.10, "gravity_z": -686.7},
    "Rope": {"mass": 0.08, "stiffness_factor": 0.8, "damping": 0.9997, "friction": 0.20, "gravity_z": -686.7},
}


class ParamPanel:
    """Parameter editing panel with sliders and material presets."""

    def __init__(self, app: ClothMakerApp):
        self.app = app
        self._editor = None  # bool sentinel: True once cloth summary is loaded
        self._summary: dict | None = None
        self._dirty = True

        # Slider state (initialized from cloth data)
        self._mass: float = 0.1
        self._stiffness_scale: float = 1.0
        self._bend_stiffness_scale: float = 1.0
        self._stretch_stiffness_scale: float = 1.0
        self._damping: float = 0.9999
        self._friction: float = 0.2
        self._gravity_z: float = -686.7
        self._collision_tolerance: float = 14.0
        self._substeps: int = 3
        self._iterations: int = 1
        self._capsule_radius_scale: float = 1.0
        self._particle_radius: float = 1.4

        self._selected_preset: int = -1
        self._preset_names = list(_PRESETS.keys())

    def _ensure_editor(self) -> bool:
        """Refresh the cloth summary if cloth is loaded."""
        if not self.app.scene.loaded:
            self._editor = None
            self._summary = None
            return False

        if self._editor is None or self._dirty:
            try:
                from creation_lib._native import havok_native
                import json as _json
                self._summary = _json.loads(havok_native.cloth_summary_json(self.app.scene.blob))
                self._editor = True  # sentinel: summary is loaded
            except Exception as e:
                _log.warning("Failed to load cloth summary: %s", e)
                self._editor = None
                self._summary = None
                return False
            self._load_current_values()
            self._dirty = False
        return True

    def _load_current_values(self) -> None:
        """Initialize slider values from current cloth data."""
        if self._summary is None:
            return

        si = self._summary.get("simulation_info", {})
        grav = si.get("gravity", [0, 0, -686.7, 1])
        self._gravity_z = grav[2] if isinstance(grav, list) else -686.7
        self._damping = si.get("globalDampingPerSecond", 0.9999)
        self._collision_tolerance = si.get("collisionTolerance", 14.0)

        mass_range = self._summary.get("mass_range", (0, 0.1))
        self._mass = mass_range[1]  # max mass

        radius_range = self._summary.get("radius_range", (1.4, 1.4))
        self._particle_radius = radius_range[0]

        friction_range = self._summary.get("friction_range", (0.2, 0.2))
        self._friction = friction_range[0]

        op = self._summary.get("operator", {})
        self._substeps = op.get("substeps", 3)
        self._iterations = op.get("iterations", 1)

        # Reset scale factors
        self._stiffness_scale = 1.0
        self._bend_stiffness_scale = 1.0
        self._stretch_stiffness_scale = 1.0
        self._capsule_radius_scale = 1.0

    def draw(self) -> None:
        visible, _ = imgui.begin("Parameters##cloth_maker")
        if not visible:
            imgui.end()
            return

        if not self._ensure_editor():
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "No cloth loaded. Import a NIF first.",
            )
            imgui.end()
            return

        self._draw_presets()
        imgui.spacing()
        self._draw_particle_params()
        imgui.spacing()
        self._draw_constraint_params()
        imgui.spacing()
        self._draw_simulation_params()
        imgui.spacing()
        self._draw_capsule_params()
        imgui.spacing()
        self._draw_apply_button()

        imgui.end()

    def _draw_presets(self) -> None:
        imgui.separator_text("Material Presets")

        for i, name in enumerate(self._preset_names):
            is_selected = (i == self._selected_preset)
            if is_selected:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.2, 0.5, 0.2, 1.0))
            if imgui.button(f"{name}##{i}"):
                self._selected_preset = i
                self._apply_preset(name)
            if is_selected:
                imgui.pop_style_color()
            if (i + 1) % 4 != 0:
                imgui.same_line()

    def _apply_preset(self, name: str) -> None:
        """Apply a material preset to the slider values."""
        preset = _PRESETS[name]
        self._mass = preset["mass"]
        self._stiffness_scale = preset["stiffness_factor"]
        self._damping = preset["damping"]
        self._friction = preset["friction"]
        self._gravity_z = preset["gravity_z"]
        _log.info("Applied preset: %s", name)

    def _draw_particle_params(self) -> None:
        imgui.separator_text("Particles")

        changed, self._mass = imgui.slider_float(
            "Mass", self._mass, 0.001, 0.5, "%.4f",
        )

        changed, self._particle_radius = imgui.slider_float(
            "Radius", self._particle_radius, 0.1, 10.0, "%.2f",
        )

        changed, self._friction = imgui.slider_float(
            "Friction", self._friction, 0.0, 1.0, "%.3f",
        )

    def _draw_constraint_params(self) -> None:
        imgui.separator_text("Constraints")

        changed, self._stiffness_scale = imgui.slider_float(
            "Standard Stiffness", self._stiffness_scale, 0.01, 5.0, "%.2fx",
        )

        changed, self._stretch_stiffness_scale = imgui.slider_float(
            "Stretch Stiffness", self._stretch_stiffness_scale, 0.01, 5.0, "%.2fx",
        )

        changed, self._bend_stiffness_scale = imgui.slider_float(
            "Bend Stiffness", self._bend_stiffness_scale, 0.01, 5.0, "%.2fx",
        )

        imgui.text_disabled("Scale factors relative to current values")

    def _draw_simulation_params(self) -> None:
        imgui.separator_text("Simulation")

        changed, self._gravity_z = imgui.slider_float(
            "Gravity Z", self._gravity_z, -2000.0, 0.0, "%.1f",
        )

        changed, self._damping = imgui.slider_float(
            "Damping", self._damping, 0.990, 1.0, "%.6f",
        )

        changed, self._collision_tolerance = imgui.slider_float(
            "Collision Tolerance", self._collision_tolerance, 0.1, 50.0, "%.2f",
        )

        changed, self._substeps = imgui.slider_int(
            "Substeps", self._substeps, 1, 10,
        )

        changed, self._iterations = imgui.slider_int(
            "Solver Iterations", self._iterations, 1, 10,
        )

    def _draw_capsule_params(self) -> None:
        imgui.separator_text("Capsules")

        capsule_count = self._summary.get("capsule_count", 0) if self._summary else 0
        imgui.text(f"Capsules: {capsule_count}")

        changed, self._capsule_radius_scale = imgui.slider_float(
            "Radius Scale", self._capsule_radius_scale, 0.1, 3.0, "%.2fx",
        )
        imgui.text_disabled("Scale factor for all capsule radii")

    def _draw_apply_button(self) -> None:
        imgui.separator()
        imgui.spacing()

        if imgui.button("Apply Changes", imgui.ImVec2(-1, 30)):
            self._apply_all()

        imgui.spacing()
        imgui.text_disabled("Changes modify the in-memory cloth graph.")
        imgui.text_disabled("Export NIF to save to disk.")

    def _apply_all(self) -> None:
        """Apply all slider values to the cloth graph via native pyfunctions."""
        if self._editor is None:
            return

        self.app.push_undo("Apply parameters")
        try:
            from creation_lib._native import havok_native
            import json as _json

            scene = self.app.scene
            blob = scene.blob

            # Particles
            blob, _ = havok_native.cloth_set_particle_mass_all(blob, self._mass)
            blob, _ = havok_native.cloth_set_particle_radius_all(blob, self._particle_radius)
            blob, _ = havok_native.cloth_set_particle_friction_all(blob, self._friction)

            # Constraints
            if self._stiffness_scale != 1.0:
                blob, _ = havok_native.cloth_scale_stiffness(blob, "standard", self._stiffness_scale)
            if self._stretch_stiffness_scale != 1.0:
                blob, _ = havok_native.cloth_scale_stiffness(blob, "stretch", self._stretch_stiffness_scale)
            if self._bend_stiffness_scale != 1.0:
                blob, _ = havok_native.cloth_scale_stiffness(blob, "bend", self._bend_stiffness_scale)

            # Simulation info
            blob = havok_native.cloth_set_gravity(blob, [0.0, 0.0, self._gravity_z, 1.0])
            blob = havok_native.cloth_set_damping(blob, self._damping)
            blob = havok_native.cloth_set_collision_tolerance(blob, self._collision_tolerance)

            # Operator
            blob = havok_native.cloth_set_substeps(blob, self._substeps)
            blob = havok_native.cloth_set_solver_iterations(blob, self._iterations)

            # Capsules
            if self._capsule_radius_scale != 1.0:
                blob, _ = havok_native.cloth_scale_all_capsule_radii(blob, self._capsule_radius_scale)

            scene.refresh_from_blob(blob)

            # Refresh summary and reset scale factors
            self._summary = _json.loads(havok_native.cloth_summary_json(blob))
            self._stiffness_scale = 1.0
            self._stretch_stiffness_scale = 1.0
            self._bend_stiffness_scale = 1.0
            self._capsule_radius_scale = 1.0
            self._dirty = False

            # Invalidate solver so it rebuilds with new parameters
            if self.app.preview_panel and self.app.preview_panel.solver is not None:
                self.app.preview_panel.solver = None
                self.app.preview_panel.playing = False

            self.app.status_text = "Parameters applied"
            _log.info("Applied cloth parameter changes")

        except Exception as e:
            self.app.status_text = f"Apply error: {e}"
            _log.error("Failed to apply parameters: %s", e, exc_info=True)
