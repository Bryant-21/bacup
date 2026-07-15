"""Preview panel — XPBD cloth simulation controls.

Play/pause/reset/step buttons, wind/gravity/iterations sliders,
FPS counter, and accuracy badge. Drives native cloth simulation via
havok_native.cloth_simulate_from_blob.
"""
from __future__ import annotations

import json
import logging
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

import numpy as np
from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.preview_panel")


@dataclass
class _NativeSimState:
    """Thin wrapper providing a ClothSolver-compatible native interface."""
    blob: bytes
    num_particles: int
    fixed_count: int
    positions: np.ndarray  # (N, 3) float32, updated each step
    prev_positions: np.ndarray  # (N, 3) float32, native Verlet history
    velocities: np.ndarray  # (N, 3) float32, derived for velocity overlay
    # Approximate fixed mask: particles that don't move from T=0 positions
    fixed_mask: np.ndarray  # (N,) bool
    frame_count: int = 0
    _initial_positions: np.ndarray = field(default_factory=lambda: np.empty((0, 3), dtype=np.float32))

    def _build_config(self, substeps: int, constraint_iters: int, gravity_z: float,
                      wind_x: float, wind_y: float, wind_z: float, damping: float) -> str:
        return json.dumps({
            "substeps": substeps,
            "constraint_iterations": constraint_iters,
            "gravity": [0.0, 0.0, gravity_z],
            "wind": [wind_x, wind_y, wind_z],
            "damping": damping,
        })

    def step(self, substeps: int, constraint_iters: int, gravity_z: float,
             wind_x: float, wind_y: float, wind_z: float, damping: float) -> None:
        from creation_lib._native import havok_native  # noqa: PLC0415
        next_frame = self.frame_count + 1
        config_json = self._build_config(substeps, constraint_iters, gravity_z, wind_x, wind_y, wind_z, damping)
        try:
            if callable(getattr(havok_native, "cloth_step_from_blob_state", None)):
                result = json.loads(havok_native.cloth_step_from_blob_state(
                    self.blob,
                    json.dumps(self.positions.tolist()),
                    json.dumps(self.prev_positions.tolist()),
                    config_json,
                ))
                new_positions = np.array(result["positions"], dtype=np.float32)
                new_prev_positions = np.array(result["prev_positions"], dtype=np.float32)
            else:
                result = json.loads(havok_native.cloth_simulate_from_blob(self.blob, next_frame, config_json))
                new_positions = np.array(result["positions"], dtype=np.float32)
                new_prev_positions = self.positions.copy()
        except Exception as exc:
            _log.warning("cloth simulation failed at frame %d: %s", next_frame, exc)
            raise

        self.velocities = new_positions - new_prev_positions
        self.prev_positions = new_prev_positions
        self.positions = new_positions
        self.frame_count = next_frame

    def reset(self) -> None:
        self.frame_count = 0
        self.positions = self._initial_positions.copy()
        self.prev_positions = self._initial_positions.copy()
        self.velocities = np.zeros_like(self.positions)


class PreviewPanel:
    """Simulation preview controls panel."""

    def __init__(self, app: ClothMakerApp):
        self.app = app
        self.solver: _NativeSimState | None = None

        # Playback state
        self.playing: bool = False
        self.frame_count: int = 0
        self._last_step_time: float = 0.0
        self._step_ms: float = 0.0  # last step wall-clock time in ms
        self._fps_samples: list[float] = []

        # Solver config overrides (UI sliders)
        self._substeps: int = 2
        self._constraint_iters: int = 12
        self._gravity_z: float = -686.7
        self._wind_x: float = 0.0
        self._wind_y: float = 0.0
        self._wind_z: float = 0.0
        self._damping: float = 0.999
        self._show_velocities: bool = False
        self._velocity_scale: float = 0.05

    def _ensure_solver(self) -> bool:
        """Build native sim state from the current scene's cloth blob."""
        if self.solver is not None:
            return True

        if not self.app.scene.loaded:
            return False

        try:
            from creation_lib._native import havok_native, nif_core_native  # noqa: PLC0415
            blob = nif_core_native.cloth_extract_blob(
                Path(self.app.scene.nif_path).read_bytes()
            )
            # Get T=0 positions to initialize state
            result0 = json.loads(havok_native.cloth_simulate_from_blob(blob, 0, None))
            initial_pos = np.array(result0["positions"], dtype=np.float32)
            n_particles = result0["n_particles"]
            fixed_count = result0["fixed_count"]
            # Approximate fixed mask: estimate from T=1 positions
            result1 = json.loads(havok_native.cloth_simulate_from_blob(blob, 1, None))
            pos1 = np.array(result1["positions"], dtype=np.float32)
            diff = np.linalg.norm(pos1 - initial_pos, axis=1)
            fixed_mask = diff < 0.01

            state = _NativeSimState(
                blob=blob,
                num_particles=n_particles,
                fixed_count=fixed_count,
                positions=initial_pos.copy(),
                prev_positions=initial_pos.copy(),
                velocities=np.zeros_like(initial_pos),
                fixed_mask=fixed_mask,
            )
            state._initial_positions = initial_pos.copy()
            self.solver = state
            _log.info("Built native sim state: %d particles (%d fixed)", n_particles, fixed_count)
            return True
        except Exception as e:
            _log.error("Failed to build native sim state: %s", e, exc_info=True)
            return False

    def _sync_config(self) -> None:
        """No-op: config is passed per-step via _NativeSimState.step()."""

    def draw(self) -> None:
        visible, _ = imgui.begin("Preview##cloth_maker")
        if not visible:
            imgui.end()
            return

        if not self.app.scene.loaded:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "No cloth loaded. Import a NIF first.",
            )
            imgui.end()
            return

        self._draw_playback_controls()
        imgui.spacing()
        self._draw_solver_params()
        imgui.spacing()
        self._draw_display_options()
        imgui.spacing()
        self._draw_stats()

        # Auto-step if playing
        if self.playing:
            self._step_solver()

        imgui.end()

    def _draw_playback_controls(self) -> None:
        imgui.separator_text("Playback")

        if self.playing:
            if imgui.button("Pause", imgui.ImVec2(80, 0)):
                self.playing = False
        else:
            if imgui.button("Play", imgui.ImVec2(80, 0)):
                if self._ensure_solver():
                    self.playing = True

        imgui.same_line()
        if imgui.button("Step", imgui.ImVec2(80, 0)):
            if self._ensure_solver():
                self._step_solver()

        imgui.same_line()
        if imgui.button("Reset", imgui.ImVec2(80, 0)):
            self._reset_solver()

        imgui.same_line()
        if imgui.button("Rebuild", imgui.ImVec2(80, 0)):
            self._rebuild_solver()

    def _draw_solver_params(self) -> None:
        imgui.separator_text("Solver")

        # Config changes take effect on the next step (passed via _NativeSimState.step())
        _, self._substeps = imgui.slider_int(
            "Substeps##preview", self._substeps, 1, 16,
        )

        _, self._constraint_iters = imgui.slider_int(
            "Constraint Iterations##preview", self._constraint_iters, 1, 20,
        )

        _, self._gravity_z = imgui.slider_float(
            "Gravity Z##preview", self._gravity_z, -2000.0, 0.0, "%.1f",
        )

        _, self._damping = imgui.slider_float(
            "Damping##preview", self._damping, 0.9, 1.0, "%.4f",
        )

        imgui.separator_text("Wind")

        _, self._wind_x = imgui.slider_float(
            "Wind X##preview", self._wind_x, -500.0, 500.0, "%.1f",
        )

        _, self._wind_y = imgui.slider_float(
            "Wind Y##preview", self._wind_y, -500.0, 500.0, "%.1f",
        )

        _, self._wind_z = imgui.slider_float(
            "Wind Z##preview", self._wind_z, -500.0, 500.0, "%.1f",
        )

    def _draw_display_options(self) -> None:
        imgui.separator_text("Display")

        changed, self.app._deform_mesh_enabled = imgui.checkbox(
            "Deform Mesh", self.app._deform_mesh_enabled,
        )
        if changed and not self.app._deform_mesh_enabled:
            # Toggled off — restore original mesh immediately
            self.app.reset_mesh_deformation()

        _, self._show_velocities = imgui.checkbox(
            "Show Velocity Arrows", self._show_velocities,
        )

        if self._show_velocities:
            _, self._velocity_scale = imgui.slider_float(
                "Arrow Scale", self._velocity_scale, 0.001, 0.5, "%.3f",
            )

    def _draw_stats(self) -> None:
        imgui.separator_text("Statistics")

        if self.solver is not None:
            n = self.solver.num_particles
            imgui.text(f"Particles: {n}")
            imgui.text(f"Frame: {self.frame_count}")
            imgui.text(f"Step time: {self._step_ms:.1f} ms")

            # FPS estimate
            if len(self._fps_samples) > 0:
                avg_ms = sum(self._fps_samples) / len(self._fps_samples)
                if avg_ms > 0:
                    fps = 1000.0 / avg_ms
                    imgui.text(f"Solver FPS: {fps:.0f}")

                    # Accuracy badge
                    if fps >= 50:
                        imgui.same_line()
                        imgui.text_colored(
                            imgui.ImVec4(0.2, 0.9, 0.2, 1.0), "[OK]",
                        )
                    elif fps >= 30:
                        imgui.same_line()
                        imgui.text_colored(
                            imgui.ImVec4(0.9, 0.9, 0.2, 1.0), "[SLOW]",
                        )
                    else:
                        imgui.same_line()
                        imgui.text_colored(
                            imgui.ImVec4(0.9, 0.2, 0.2, 1.0), "[TOO SLOW]",
                        )

            # NaN check
            if self.solver.positions.size > 0 and np.any(np.isnan(self.solver.positions)):
                imgui.text_colored(
                    imgui.ImVec4(1.0, 0.0, 0.0, 1.0),
                    "WARNING: NaN detected in positions!",
                )
        else:
            imgui.text_disabled("Solver not initialized")
            imgui.text_disabled("Click Play or Step to start")

    def _step_solver(self) -> None:
        """Advance the solver one frame and push positions to scene."""
        if self.solver is None:
            return

        t0 = time.perf_counter()
        try:
            self.solver.step(
                substeps=self._substeps,
                constraint_iters=self._constraint_iters,
                gravity_z=self._gravity_z,
                wind_x=self._wind_x,
                wind_y=self._wind_y,
                wind_z=self._wind_z,
                damping=self._damping,
            )
        except Exception as exc:
            self.playing = False
            self.app.status_text = f"Simulation failed: {exc}"
            return
        t1 = time.perf_counter()

        self._step_ms = (t1 - t0) * 1000.0
        self._fps_samples.append(self._step_ms)
        if len(self._fps_samples) > 60:
            self._fps_samples.pop(0)

        self.frame_count = self.solver.frame_count

        # Push updated positions back into the scene's particle data
        pd = self.app.scene.particle_data
        if pd is not None:
            solver_pos = self.solver.positions
            if solver_pos.shape[0] == pd.positions.shape[0]:
                pd.positions = solver_pos.copy()
            else:
                # Size mismatch — copy what fits
                n = min(solver_pos.shape[0], pd.positions.shape[0])
                pd.positions[:n] = solver_pos[:n]

        self.app.deform_mesh(self.solver.positions)

        self.app.status_text = (
            f"Sim frame {self.frame_count} — "
            f"{self._step_ms:.1f} ms/step — "
            f"{self.solver.num_particles} particles"
        )

    def _reset_solver(self) -> None:
        """Reset solver to initial positions."""
        self.playing = False
        # Restore original mesh vertices before re-extracting overlay data
        self.app.reset_mesh_deformation()
        if self.solver is not None:
            if self.app.scene.loaded:
                self.app.scene._extract_overlay_data()
            self.solver.reset()
            self.frame_count = 0
            self._fps_samples.clear()
            self._step_ms = 0.0
            self.app.status_text = "Simulation reset"

    def _rebuild_solver(self) -> None:
        """Rebuild solver from scratch (e.g. after parameter changes)."""
        self.playing = False
        self.app.reset_mesh_deformation()
        self.solver = None
        self.frame_count = 0
        self._fps_samples.clear()
        self._step_ms = 0.0
        if self._ensure_solver():
            self.app.status_text = "Solver rebuilt"
