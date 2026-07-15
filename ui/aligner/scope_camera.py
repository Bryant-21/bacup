"""Dual-mode camera for scope alignment.

ORBIT mode: standard orbit camera for inspecting weapon+scope assembly.
SCOPE_VIEW mode: camera at skeleton Camera bone + user offsets, looking down aim axis.
"""
from __future__ import annotations

import enum

import glm

from creation_lib.renderer.camera import OrbitCamera


class CameraMode(enum.Enum):
    ORBIT = "orbit"
    SCOPE_VIEW = "scope_view"


class ScopeCamera:
    """Dual-mode camera for scope alignment."""

    # FO4 default 1st-person FOV (75° horizontal, 22mm lens equivalent)
    DEFAULT_FOV_DEG = 75.0

    def __init__(self):
        self._orbit = OrbitCamera()
        self._mode = CameraMode.ORBIT

        # Skeleton bone data (set by aligner app after loading skeleton)
        self.camera_bone_pos = glm.vec3(0, 0, 120.48)
        self.aim_direction = glm.vec3(0, 1, 0)

        # Scope-view offsets (user-adjustable via sliders)
        self.offset_x: float = 0.0
        self.offset_y: float = 0.0
        self.offset_z: float = 0.0

        # FOV base in degrees (FO4 range: 65–105, default 75)
        self.fov_deg: float = self.DEFAULT_FOV_DEG

        # FOV multiplier (scope zoom — applied on top of base FOV)
        self.fov_mult: float = 1.0

        # Clipping planes
        self.near: float = 0.1
        self.far: float = 10000.0

    @property
    def distance(self) -> float:
        """Orbit distance — delegated to inner OrbitCamera for gizmo compatibility."""
        return self._orbit.distance

    @distance.setter
    def distance(self, v: float):
        self._orbit.distance = v

    @property
    def mode(self) -> CameraMode:
        return self._mode

    @mode.setter
    def mode(self, value: CameraMode):
        self._mode = value

    @property
    def orbit(self) -> OrbitCamera:
        """Access the underlying orbit camera for input handling."""
        return self._orbit

    def get_eye_position(self) -> glm.vec3:
        if self._mode == CameraMode.ORBIT:
            return self._orbit.get_eye_position()
        # Scope view: camera bone + user offsets
        return self.camera_bone_pos + glm.vec3(
            self.offset_x, self.offset_y, self.offset_z
        )

    def get_view_matrix(self) -> glm.mat4:
        if self._mode == CameraMode.ORBIT:
            return self._orbit.get_view_matrix()
        eye = self.get_eye_position()
        target = eye + self.aim_direction
        up = glm.vec3(0, 0, 1)
        return glm.lookAt(eye, target, up)

    def get_projection_matrix(self, aspect: float) -> glm.mat4:
        if self._mode == CameraMode.ORBIT:
            return self._orbit.get_projection_matrix(aspect)
        # FovMult is a zoom factor: higher = more zoom = narrower FOV
        fov_rad = glm.radians(self.fov_deg / self.fov_mult)
        return glm.perspective(fov_rad, aspect, self.near, self.far)

    def handle_input(self, io):
        """Process imgui IO for orbit mode only."""
        if self._mode == CameraMode.ORBIT:
            self._orbit.handle_input(io)

    def frame_on_bounds(self, center, radius):
        """Frame orbit camera on bounding sphere."""
        self._orbit.frame_on_bounds(center, radius)

    def set_from_skeleton(self, skeleton_data: dict):
        """Set camera bone and aim data from skeleton loader output."""
        cx, cy, cz = skeleton_data["camera_pos"]
        self.camera_bone_pos = glm.vec3(cx, cy, cz)
        ax, ay, az = skeleton_data["aim_direction"]
        self.aim_direction = glm.vec3(ax, ay, az)

    def reset_offsets(self):
        """Reset all offsets to zero and FOV to defaults."""
        self.offset_x = 0.0
        self.offset_y = 0.0
        self.offset_z = 0.0
        self.fov_deg = self.DEFAULT_FOV_DEG
        self.fov_mult = 1.0
