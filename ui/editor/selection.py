"""Selection system for the NIF editor.

Two-phase ray picking for accurate click-to-select:
1. Broad phase: ray-sphere intersection (fast culling)
2. Narrow phase: ray-triangle intersection (precise, Möller-Trumbore)

Each pickable shape has a bounding sphere (world space) and cached
world-space triangle data. On click, we unproject a ray from the camera,
cull with spheres, then test actual triangles for the closest hit.
"""
from __future__ import annotations
from dataclasses import dataclass
import logging
import math

import glm
import numpy as np

from creation_lib.renderer.scene_renderer import SceneNode

_log = logging.getLogger("nif_editor.selection")

# Small epsilon for ray intersection tests
_EPS = 1e-6


@dataclass(frozen=True)
class CollisionShapeSelection:
    """Selection address for a virtual shape inside bhkPhysicsSystem data."""

    nif_id: str
    block_id: int
    body_id: int
    shape_index: int | None


def _ray_sphere_t(origin: glm.vec3, direction: glm.vec3,
                  center: glm.vec3, radius: float) -> float | None:
    """Return the smallest positive t for a ray-sphere intersection, or None."""
    oc = origin - center
    a = glm.dot(direction, direction)
    b = 2.0 * glm.dot(oc, direction)
    c = glm.dot(oc, oc) - radius * radius
    discriminant = b * b - 4.0 * a * c
    if discriminant < 0:
        return None
    sqrt_d = math.sqrt(discriminant)
    t1 = (-b - sqrt_d) / (2.0 * a)
    t2 = (-b + sqrt_d) / (2.0 * a)
    if t1 > _EPS:
        return t1
    if t2 > _EPS:
        return t2
    return None


def _ray_triangles_t(origin: np.ndarray, direction: np.ndarray,
                     world_verts: np.ndarray, tris: np.ndarray) -> float | None:
    """Vectorized Möller-Trumbore ray-triangle intersection.

    Tests all triangles at once using numpy. Returns the smallest positive t
    for the closest hit, or None if no triangle is hit.

    Args:
        origin: Ray origin as (3,) float64 array.
        direction: Ray direction as (3,) float64 array (normalized).
        world_verts: World-space vertex positions, shape (N, 3).
        tris: Triangle index array, shape (M, 3) with dtype uint32.
    """
    if len(tris) == 0 or len(world_verts) == 0:
        return None

    # Gather triangle vertices: (M, 3) each
    v0 = world_verts[tris[:, 0]]
    v1 = world_verts[tris[:, 1]]
    v2 = world_verts[tris[:, 2]]

    edge1 = v1 - v0  # (M, 3)
    edge2 = v2 - v0  # (M, 3)

    # h = direction × edge2
    h = np.cross(direction, edge2)  # (M, 3)
    a = np.einsum('ij,ij->i', edge1, h)  # (M,) dot products

    # Filter near-parallel triangles
    valid = np.abs(a) > _EPS
    if not np.any(valid):
        return None

    f = np.zeros_like(a)
    f[valid] = 1.0 / a[valid]

    s = origin - v0  # (M, 3)
    u = f * np.einsum('ij,ij->i', s, h)  # (M,)

    # u must be in [0, 1]
    valid &= (u >= 0.0) & (u <= 1.0)
    if not np.any(valid):
        return None

    q = np.cross(s, edge1)  # (M, 3)
    v = f * np.einsum('ij,ij->i', q, direction[np.newaxis] * np.ones_like(q))

    # v must be in [0, 1] and u+v <= 1
    valid &= (v >= 0.0) & ((u + v) <= 1.0)
    if not np.any(valid):
        return None

    t = f * np.einsum('ij,ij->i', edge2, q)

    # Only positive t (in front of camera)
    valid &= t > _EPS

    if not np.any(valid):
        return None

    return float(np.min(t[valid]))


class SelectionManager:
    def __init__(self):
        self._selected: SceneNode | None = None
        self._selected_nif_id: str | None = None
        self._selected_block_id: int | None = None
        self._selected_block_id_override: int | None = None
        self._selected_collision_shape: CollisionShapeSelection | None = None
        self._callbacks: list = []
        # Store only node references — read bound_center/bound_radius live
        # so picking stays correct after gizmo moves update world_transform.
        self._nodes: list[SceneNode] = []

    def register_bounds(self, node: SceneNode):
        """Register a SceneNode's bounding sphere for picking."""
        self._nodes.clear()
        self._register_recursive(node)
        _log.debug("Registered %d pickable meshes:", len(self._nodes))
        for n in self._nodes:
            _log.debug("  %s (block %d): center=(%.1f,%.1f,%.1f) radius=%.1f",
                       n.name, n.block_id,
                       n.bound_center.x, n.bound_center.y, n.bound_center.z,
                       n.bound_radius)

    def _register_recursive(self, node: SceneNode):
        if node.mesh and node.bound_radius > 0:
            self._nodes.append(node)
        for child in node.children:
            self._register_recursive(child)

    def register_extra_nodes(self, nodes: list[SceneNode]):
        """Register additional pickable nodes (e.g. lights) that have no mesh.

        These use sphere-only picking (bound_center + bound_radius).
        Call after register_bounds() since that clears the node list.
        Replaces any existing extra nodes with the same block_id to avoid
        accumulating stale duplicates on rebuild.
        """
        # Build set of block_ids being added so we can remove stale duplicates
        new_ids = {(n.nif_id, n.block_id) for n in nodes}
        self._nodes = [
            n for n in self._nodes
            if n.mesh is not None or (n.nif_id, n.block_id) not in new_ids
        ]
        self._nodes.extend(nodes)

    def clear(self):
        """Clear all registered bounds and selection."""
        self._nodes.clear()
        self._selected = None
        self._selected_block_id = None
        self._selected_nif_id = None
        self._selected_collision_shape = None

    def _get_world_verts(self, node: SceneNode) -> np.ndarray | None:
        """Get world-space vertex positions for a node's mesh."""
        local_verts = getattr(node, '_local_verts', None)
        if local_verts is None or len(local_verts) == 0:
            return None
        # Transform local verts to world space
        wt = node.world_transform
        # Build 4x4 numpy matrix from glm mat4 (column-major)
        m = np.array([
            [wt[0][0], wt[1][0], wt[2][0], wt[3][0]],
            [wt[0][1], wt[1][1], wt[2][1], wt[3][1]],
            [wt[0][2], wt[1][2], wt[2][2], wt[3][2]],
            [wt[0][3], wt[1][3], wt[2][3], wt[3][3]],
        ], dtype=np.float64)
        # Homogeneous transform: (N,3) -> (N,4) -> multiply -> (N,3)
        ones = np.ones((len(local_verts), 1), dtype=np.float64)
        hom = np.hstack([local_verts.astype(np.float64), ones])  # (N, 4)
        world = (m @ hom.T).T  # (N, 4)
        return world[:, :3]

    def pick(self, mouse_x: float, mouse_y: float,
             viewport_pos, viewport_size, camera):
        """Cast a ray from mouse position and find closest hit.

        Two-phase approach:
        1. Broad phase: ray-sphere to find candidate meshes
        2. Narrow phase: ray-triangle on candidates for precise picking
        Falls back to sphere-only if a node lacks triangle data.
        """
        # Convert mouse to NDC
        ndc_x = (mouse_x - viewport_pos.x) / viewport_size.x * 2.0 - 1.0
        ndc_y = 1.0 - (mouse_y - viewport_pos.y) / viewport_size.y * 2.0

        # Unproject near and far points
        aspect = viewport_size.x / max(viewport_size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)
        inv_vp = glm.inverse(proj * view)

        near_ndc = glm.vec4(ndc_x, ndc_y, -1.0, 1.0)
        far_ndc = glm.vec4(ndc_x, ndc_y, 1.0, 1.0)
        near_world = inv_vp * near_ndc
        far_world = inv_vp * far_ndc
        near_world /= near_world.w
        far_world /= far_world.w

        origin = glm.vec3(near_world)
        direction = glm.normalize(glm.vec3(far_world) - glm.vec3(near_world))

        # Phase 1: Broad phase — collect all sphere hits
        # Read bound_center/bound_radius live from nodes (not cached copies)
        sphere_hits: list[tuple[SceneNode, float]] = []
        for node in self._nodes:
            t = _ray_sphere_t(origin, direction, node.bound_center, node.bound_radius)
            if t is not None:
                sphere_hits.append((node, t))

        if not sphere_hits:
            self.deselect()
            return

        # Phase 2: Narrow phase — ray-triangle on sphere-hit nodes
        origin_np = np.array([origin.x, origin.y, origin.z], dtype=np.float64)
        dir_np = np.array([direction.x, direction.y, direction.z], dtype=np.float64)

        best_t = float("inf")
        best_node = None

        for node, sphere_t in sphere_hits:
            tris = getattr(node, '_local_tris', None)
            if tris is None or len(tris) == 0:
                # Fallback: use sphere t if no triangle data
                if sphere_t < best_t:
                    best_t = sphere_t
                    best_node = node
                continue

            world_verts = self._get_world_verts(node)
            if world_verts is None:
                if sphere_t < best_t:
                    best_t = sphere_t
                    best_node = node
                continue

            tri_t = _ray_triangles_t(origin_np, dir_np, world_verts, tris)
            if tri_t is not None and tri_t < best_t:
                best_t = tri_t
                best_node = node

        if best_node:
            self.select(best_node)
        else:
            # Sphere was hit but no triangles — deselect (clicked empty space
            # inside the bounding sphere but not on actual geometry)
            self.deselect()

    def select(self, node: SceneNode):
        """Select a scene node."""
        old = self._selected
        self._selected = node
        self._selected_block_id = node.block_id
        self._selected_nif_id = node.nif_id
        self._selected_collision_shape = None
        _log.debug("Selected: %s (block %d, nif %s)", node.name, node.block_id, node.nif_id)
        if node is not old:
            self._notify(self._selected_nif_id, self._selected_block_id)

    def select_by_id(self, nif_id: str, block_id: int):
        """Select a node by (nif_id, block_id) pair."""
        for node in self._nodes:
            if node.nif_id == nif_id and node.block_id == block_id:
                self.select(node)
                return
        self._selected = None
        self._selected_nif_id = nif_id
        self._selected_block_id = block_id
        self._selected_collision_shape = None
        self._notify(nif_id, block_id)

    def select_collision_shape(
        self,
        nif_id: str,
        block_id: int,
        body_id: int,
        shape_index: int | None,
    ):
        """Select a decoded shape that has no standalone NIF block."""
        self._selected = None
        self._selected_nif_id = nif_id
        self._selected_block_id = block_id
        self._selected_collision_shape = CollisionShapeSelection(
            nif_id=nif_id,
            block_id=block_id,
            body_id=body_id,
            shape_index=shape_index,
        )
        self._notify(nif_id, block_id)

    def select_by_block_id(self, block_id: int):
        """Legacy: select by block_id only. Ambiguous in multi-NIF mode."""
        _log.warning("select_by_block_id() is ambiguous in multi-NIF mode — "
                     "use select_by_id(nif_id, block_id) instead")
        self._selected_block_id_override = block_id
        for node in self._nodes:
            if node.block_id == block_id:
                self.select(node)
                return
        # Block not in spheres (not a mesh) — still track and notify
        self._selected = None
        self._selected_block_id = block_id
        self._selected_nif_id = None
        self._selected_collision_shape = None
        self._notify(None, block_id)

    def deselect(self):
        """Clear selection."""
        self._selected = None
        self._selected_block_id = None
        self._selected_nif_id = None
        self._selected_block_id_override = None
        self._selected_collision_shape = None
        self._notify(None, None)

    def _notify(self, nif_id: str | None, block_id: int | None):
        """Fire selection-changed callbacks with (nif_id, block_id)."""
        for cb in self._callbacks:
            cb(nif_id, block_id)

    def on_selection_changed(self, callback):
        """Register a callback for selection changes.

        Callback signature: cb(nif_id: str | None, block_id: int | None).
        """
        self._callbacks.append(callback)

    @property
    def selected(self) -> SceneNode | None:
        return self._selected

    @property
    def selected_nif_id(self) -> str | None:
        return self._selected_nif_id

    @property
    def selected_block_id(self) -> int | None:
        if self._selected_block_id_override is not None:
            bid = self._selected_block_id_override
            self._selected_block_id_override = None
            return bid
        return self._selected_block_id

    @property
    def selected_collision_shape(self) -> CollisionShapeSelection | None:
        return self._selected_collision_shape
