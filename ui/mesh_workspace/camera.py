"""Camera module — re-export of OrbitCamera + workspace framing helpers."""
from __future__ import annotations

import numpy as np
from creation_lib.renderer.camera import OrbitCamera


def frame_on_bounds(camera: OrbitCamera, verts: np.ndarray) -> None:
    """Frame the camera to fit a vertex bounding box."""
    if len(verts) == 0:
        return
    center = (verts.max(axis=0) + verts.min(axis=0)) / 2.0
    extent = np.linalg.norm(verts.max(axis=0) - verts.min(axis=0))
    import glm
    camera.target = glm.vec3(float(center[0]), float(center[1]), float(center[2]))
    camera.distance = max(float(extent) * 1.2, 1.0)
