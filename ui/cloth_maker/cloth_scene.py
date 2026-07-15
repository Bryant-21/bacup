"""ClothScene — data model for the cloth maker workspace.

Holds the loaded cloth graph as raw blob bytes plus a parsed JSON dict
(from native cloth_inspect_full_json), extracted particle positions/masses/
radii as numpy arrays (for overlay rendering), and per-overlay display toggles.
"""
from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

import numpy as np

_log = logging.getLogger("cloth_maker.scene")


def _nif_bone_world_transforms(nif_path: str) -> dict[str, tuple[np.ndarray, np.ndarray]]:
    """Load bone world transforms from a NIF file.

    Walks the NiNode hierarchy and accumulates parent transforms to produce
    world-space (translation, rotation_3x3) for every named bone.

    Returns:
        dict mapping bone_name -> (translation (3,), rotation (3,3))
    """
    from creation_lib.nif.nif_file import NifFile

    nif = NifFile.load(nif_path)

    # Collect NiNode bones: block_id -> {name, translation, rotation}
    nif_bones: dict[int, dict] = {}
    parent_map: dict[int, int] = {}  # child_block_id -> parent_block_id

    for i, block in enumerate(nif.blocks):
        if block.type_name != "NiNode":
            continue
        name = block.get_field("Name")
        if not name:
            continue
        name = str(name)
        trans = block.get_field("Translation") or {}
        rot = block.get_field("Rotation") or {}
        nif_bones[i] = {
            "name": name,
            "translation": np.array([
                float(trans.get("x", 0)),
                float(trans.get("y", 0)),
                float(trans.get("z", 0)),
            ], dtype=np.float64),
            "rotation": np.array([
                [float(rot.get("m11", 1)), float(rot.get("m21", 0)), float(rot.get("m31", 0))],
                [float(rot.get("m12", 0)), float(rot.get("m22", 1)), float(rot.get("m32", 0))],
                [float(rot.get("m13", 0)), float(rot.get("m23", 0)), float(rot.get("m33", 1))],
            ], dtype=np.float64),
        }
        children = block.get_field("Children") or []
        if isinstance(children, list):
            for child_id in children:
                if isinstance(child_id, int) and child_id >= 0:
                    parent_map[child_id] = i

    # Compute world transforms via parent chain walk (cached)
    world_cache: dict[int, tuple[np.ndarray, np.ndarray]] = {}

    def _world(block_id: int) -> tuple[np.ndarray, np.ndarray]:
        if block_id in world_cache:
            return world_cache[block_id]
        bone = nif_bones.get(block_id)
        if bone is None:
            result = (np.zeros(3, dtype=np.float64), np.eye(3, dtype=np.float64))
            world_cache[block_id] = result
            return result
        local_t = bone["translation"]
        local_r = bone["rotation"]
        pid = parent_map.get(block_id, -1)
        if pid < 0 or pid not in nif_bones:
            world_cache[block_id] = (local_t.copy(), local_r.copy())
        else:
            pt, pr = _world(pid)
            wt = pr @ local_t + pt
            wr = pr @ local_r
            world_cache[block_id] = (wt, wr)
        return world_cache[block_id]

    # Build name -> world transform mapping
    result: dict[str, tuple[np.ndarray, np.ndarray]] = {}
    for block_id, bone in nif_bones.items():
        wt, wr = _world(block_id)
        result[bone["name"]] = (wt.astype(np.float32), wr.astype(np.float32))

    return result


def _havok_to_nif(positions: np.ndarray, z_offset: float = 0.0) -> np.ndarray:
    """Convert Havok-space positions to NIF-space.

    Havok cloth uses Z-up positive; NIF skin-space uses Z negative for height.
    The transform preserves Z ordering (so top stays top) and shifts into the
    mesh's Z range via the supplied offset.
    """
    positions[:, 2] = positions[:, 2] - z_offset
    return positions


def _havok_to_nif_vec(vec: np.ndarray, z_offset: float = 0.0) -> np.ndarray:
    """Convert a single Havok-space vector to NIF-space."""
    result = vec.copy()
    result[2] = result[2] - z_offset
    return result


def _resolve_collidable_bone(collidable_name: str, nif_bone_names: set[str]) -> str | None:
    """Resolve a collidable name like 'Collidable_LLeg_Thigh001' to a NIF bone name.

    Strips 'Collidable_' prefix and trailing digits, then tries:
    1. Exact match (core)
    2. Core + '_skin' suffix
    3. Case-insensitive match
    4. Partial substring match
    """
    import re
    core = re.sub(r"^Collidable_", "", collidable_name)
    core = re.sub(r"\d+$", "", core)

    # Exact match or with _skin suffix
    for suffix in ("_skin", ""):
        candidate = core + suffix
        if candidate in nif_bone_names:
            return candidate

    # Case-insensitive
    core_lower = core.lower()
    for bn in sorted(nif_bone_names):  # sorted for deterministic matching
        bn_lower = bn.lower()
        if bn_lower == core_lower or bn_lower == core_lower + "_skin":
            return bn

    # Partial match (prefer shorter names — closer to the core bone)
    matches = [(bn, len(bn)) for bn in nif_bone_names if core_lower in bn.lower()]
    if matches:
        matches.sort(key=lambda x: x[1])
        return matches[0][0]

    return None


@dataclass
class ParticleData:
    """Extracted particle arrays for GPU overlay rendering."""
    positions: np.ndarray  # (N, 3) float32
    masses: np.ndarray  # (N,) float32
    inv_masses: np.ndarray  # (N,) float32 — from HKX invMass field
    radii: np.ndarray  # (N,) float32
    is_fixed: np.ndarray  # (N,) bool — True for pinned particles


@dataclass
class ConstraintLink:
    """A single constraint link between two particles."""
    particle_a: int
    particle_b: int
    constraint_type: str  # "standard", "stretch", "bend", "localrange"
    stiffness: float  # 0.0-1.0 normalized


@dataclass
class CapsuleData:
    """Capsule collider for overlay rendering."""
    start: np.ndarray  # (3,) float32 — NIF world-space start
    end: np.ndarray  # (3,) float32 — NIF world-space end
    radius: float
    bone_name: str


@dataclass
class SphereData:
    """Sphere collider for overlay rendering."""
    center: np.ndarray  # (3,) float32
    radius: float
    bone_name: str


class ClothScene:
    """Manages loaded cloth data and overlay-ready arrays.

    Loading flow:
    1. load_from_nif(path) — extracts the HCL blob via nif_core_native.cloth_extract_blob
    2. Parses blob to a JSON dict via native cloth_inspect_full_json (display-only)
    3. Extracts particle positions/masses/radii into numpy arrays
    4. Extracts constraint links into a flat list
    5. Extracts capsule/sphere data for overlay rendering

    After any mutation (native cloth_* op that returns new blob bytes):
        scene.blob = new_blob
        scene.cloth_json = json.loads(havok_native.cloth_inspect_full_json(new_blob))
        scene._extract_overlay_data()
    """

    def __init__(self):
        self.nif_path: str = ""
        # Native-layer cloth state: raw HCL packfile bytes + parsed display JSON.
        self.blob: bytes | None = None
        self.cloth_json: dict | None = None

        # Extracted overlay data (populated by _extract_overlay_data)
        self.particle_data: ParticleData | None = None
        self.constraint_links: list[ConstraintLink] = []
        self.capsules: list[CapsuleData] = []
        self.spheres: list[SphereData] = []
        # 4-particle bend specs: (a, b, c, d, stiffness). a-b is the shared
        # edge, c and d are the tip vertices of the two adjacent triangles.
        # Used by the solver to build dihedral bend constraints — the flat
        # link list in ``constraint_links`` loses the c/d relationship.
        self.bend_quads: list[tuple[int, int, int, int, float]] = []

        # Bone world transforms from NIF (populated during load)
        self.bone_world_transforms: dict[str, tuple[np.ndarray, np.ndarray]] = {}

        # Havok→NIF Z offset (computed from mesh vertices during load)
        self._z_offset: float = 0.0

        # Display toggles
        self.show_particles: bool = True
        self.show_constraints: bool = True
        self.show_capsules: bool = True
        self.show_pins: bool = True

        # Overlay data version — incremented on every _extract_overlay_data() call
        # so overlays can detect stale data without relying on count/id checks
        self.data_version: int = 0

        # Particle-to-vertex mapping for mesh deformation during preview.
        # particle_to_vertex[particle_idx] = mesh_vertex_idx
        # Built during cloth generation (from region panel) or from loaded cloth.
        self.particle_to_vertex: np.ndarray | None = None  # (P,) int — particle→vertex
        self.vertex_to_particle: np.ndarray | None = None  # (V,) int — vertex→particle (-1=unmapped)

        # Selection
        self.selected_sim_cloth_idx: int = 0

    @property
    def loaded(self) -> bool:
        return self.blob is not None and self.cloth_json is not None

    @property
    def nif_loaded(self) -> bool:
        """True if a NIF is loaded (with or without cloth data)."""
        return self.nif_path != ""

    @property
    def active_sim_cloth(self) -> dict | None:
        """Return the active sim cloth JSON dict, or None."""
        if self.cloth_json is None:
            return None
        scds = self.cloth_json.get("sim_cloths", [])
        if 0 <= self.selected_sim_cloth_idx < len(scds):
            return scds[self.selected_sim_cloth_idx]
        return None

    def load_from_nif(self, nif_path: str) -> None:
        """Load a NIF file, with or without existing cloth data.

        If the NIF has BSClothExtraData, extracts the blob and parses it to
        a JSON dict via native cloth_inspect_full_json. Otherwise, loads just
        the NIF reference so cloth can be added later via region painting or
        template application.
        """
        from creation_lib._native import havok_native, nif_core_native  # noqa: PLC0415

        self.nif_path = nif_path
        self.selected_sim_cloth_idx = 0

        # Try loading existing cloth data — not required
        try:
            nif_bytes = Path(nif_path).read_bytes()
            blob = nif_core_native.cloth_extract_blob(nif_bytes)
            cloth_json = json.loads(havok_native.cloth_inspect_full_json(blob))
            if not cloth_json.get("sim_cloths"):
                _log.warning("HKX loaded but no sim cloths in %s", nif_path)
                self.blob = None
                self.cloth_json = None
            else:
                self.blob = blob
                self.cloth_json = cloth_json
        except Exception:
            _log.info("No cloth data in %s — loaded as bare NIF", nif_path)
            self.blob = None
            self.cloth_json = None

        try:
            self.bone_world_transforms = _nif_bone_world_transforms(nif_path)
        except Exception as e:
            _log.warning("Failed to load bone transforms from %s: %s", nif_path, e)
            self.bone_world_transforms = {}

        if self.cloth_json is not None:
            self._z_offset = self._compute_z_offset(nif_path)
            self._extract_overlay_data()
            _log.info("Loaded cloth from %s: %d sim cloths",
                      nif_path, len(self.cloth_json.get("sim_cloths", [])))
        else:
            self._z_offset = 0.0
            self.particle_data = None
            self.particle_to_vertex = None
            self.vertex_to_particle = None
            self.constraint_links = []
            self.capsules = []
            self.spheres = []
            self.data_version += 1

    def _compute_z_offset(self, nif_path: str) -> float:
        """Compute the Z offset to align Havok/bone world-space with NIF skin-space.

        Havok cloth particles and bone world transforms live in NIF world space
        (Z-up, feet ~6, spine ~73). NIF skin-space vertices (stored in Vertex Data)
        use a different origin (feet ~-115, spine ~-48). The offset between these
        two coordinate systems is constant for a given skeleton bind pose.

        We derive it from the BSSkin inverse-bind transforms: for each bone,
        inv(inv_bind) maps bone-local origin to skin-space, while bone_world_transforms
        gives the world-space position. The Z difference is the offset.
        """
        try:
            from creation_lib.nif.nif_file import NifFile as _NifFile

            nif = _NifFile.load(nif_path)

            # Extract inv_bind transforms from BSSkin::BoneData for all skinned shapes
            bone_inv_binds: dict[str, np.ndarray] = {}
            for block in nif.blocks:
                if not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
                    continue
                skin_ref = block.get_field("Skin")
                if skin_ref is None or skin_ref < 0:
                    continue
                skin_inst = nif.blocks[skin_ref]
                if skin_inst.type_name != "BSSkin::Instance":
                    continue
                data_ref = skin_inst.get_field("Data")
                if data_ref is None or data_ref < 0:
                    continue
                bone_data = nif.blocks[data_ref]
                bone_list = skin_inst.get_field("Bones") or []
                bone_infos = bone_data.get_field("Bone List") or []
                for bi, bone_ref in enumerate(bone_list):
                    if not isinstance(bone_ref, int) or bone_ref < 0:
                        continue
                    bone_name = nif.blocks[bone_ref].get_field("Name")
                    if not bone_name or bone_name in bone_inv_binds:
                        continue
                    if bi >= len(bone_infos):
                        continue
                    info = bone_infos[bi]
                    rot = info.get("Rotation", {})
                    trans = info.get("Translation", {})
                    # Build 4x4 inv_bind matrix
                    # NIF Matrix33 uses mCR naming (col C, row R) — transpose
                    inv_bind = np.eye(4, dtype=np.float64)
                    inv_bind[0, 0] = float(rot.get("m11", 1))
                    inv_bind[0, 1] = float(rot.get("m21", 0))
                    inv_bind[0, 2] = float(rot.get("m31", 0))
                    inv_bind[1, 0] = float(rot.get("m12", 0))
                    inv_bind[1, 1] = float(rot.get("m22", 1))
                    inv_bind[1, 2] = float(rot.get("m32", 0))
                    inv_bind[2, 0] = float(rot.get("m13", 0))
                    inv_bind[2, 1] = float(rot.get("m23", 0))
                    inv_bind[2, 2] = float(rot.get("m33", 1))
                    inv_bind[0, 3] = float(trans.get("x", 0))
                    inv_bind[1, 3] = float(trans.get("y", 0))
                    inv_bind[2, 3] = float(trans.get("z", 0))
                    bone_inv_binds[bone_name] = inv_bind

            if not bone_inv_binds or not self.bone_world_transforms:
                return 0.0

            # For each bone present in both sets, compute world_z - skin_z
            offsets: list[float] = []
            for bone_name, inv_bind in bone_inv_binds.items():
                if bone_name not in self.bone_world_transforms:
                    continue
                world_trans, _ = self.bone_world_transforms[bone_name]
                world_z = float(world_trans[2])
                # Bone origin in skin-space: inv(inv_bind) @ [0,0,0,1]
                bind_mat = np.linalg.inv(inv_bind)
                skin_z = float(bind_mat[2, 3])
                offsets.append(world_z - skin_z)

            if not offsets:
                return 0.0

            offset = float(np.median(offsets))
            _log.debug("Z offset from %d bone correspondences: %.2f",
                       len(offsets), offset)
            return offset
        except Exception as e:
            _log.debug("Could not compute Z offset: %s", e)
        return 0.0

    def build_particle_to_vertex_mapping(self, mesh_vertices: np.ndarray) -> None:
        """Build vertex→particle mapping by nearest-particle matching.

        For each mesh vertex, finds the closest cloth particle. Vertices
        beyond a distance threshold are left unmapped (-1). This ensures
        every cloth-region vertex is driven by a particle, not just one
        vertex per particle.

        The result is stored as TWO arrays:
        - particle_to_vertex: (P,) int — particle_idx → nearest mesh vertex
          (for overlay/generation compatibility)
        - vertex_to_particle: (V,) int — mesh_vertex_idx → nearest particle
          (-1 if no particle is close enough)

        Args:
            mesh_vertices: (V, 3) float32 array of mesh vertex positions.
        """
        if self.particle_data is None or mesh_vertices is None:
            self.particle_to_vertex = None
            self.vertex_to_particle = None
            return



        from creation_lib.scientific.native_runtime import CKDTree as cKDTree

        particle_pos = self.particle_data.positions

        # Particle→vertex (legacy, for region panel compatibility)
        mesh_tree = cKDTree(mesh_vertices)
        _, p2v_indices = mesh_tree.query(particle_pos, k=1)
        self.particle_to_vertex = p2v_indices.astype(np.intp)

        # Vertex→particle (for mesh deformation)
        particle_tree = cKDTree(particle_pos)
        dists, v2p_indices = particle_tree.query(mesh_vertices, k=1)
        v2p = v2p_indices.astype(np.intp)
        # Threshold: vertices too far from any particle are not cloth
        if len(particle_pos) > 1:
            # Use mean nearest-neighbor distance between particles as threshold
            p_dists, _ = particle_tree.query(particle_pos, k=2)
            mean_particle_spacing = float(p_dists[:, 1].mean())
            threshold = mean_particle_spacing * 2.5
        else:
            threshold = float(dists.max()) + 1.0
        v2p[dists > threshold] = -1
        self.vertex_to_particle = v2p

        mapped_count = int((v2p >= 0).sum())
        _log.info("Built vertex→particle mapping: %d/%d mesh verts mapped to %d particles",
                  mapped_count, len(mesh_vertices), len(particle_pos))

    def _extract_overlay_data(self) -> None:
        """Extract particle/constraint/capsule data into overlay-ready arrays."""
        scd = self.active_sim_cloth
        if scd is None:
            return

        self._extract_particles(scd)
        self._extract_constraints(scd)
        self._extract_collidables(scd)
        self.data_version += 1

    def _extract_particles(self, scd: dict) -> None:
        """Extract particle positions, masses, and radii from JSON dict.

        Positions come from the native cloth_inspect_full_json output
        (already sourced from hclSimClothPose.positions).
        """
        particles = scd.get("particles", [])
        n = len(particles)
        if n == 0:
            self.particle_data = None
            return

        positions = np.zeros((n, 3), dtype=np.float32)
        masses = np.zeros(n, dtype=np.float32)
        inv_masses = np.full(n, -1.0, dtype=np.float32)
        radii = np.zeros(n, dtype=np.float32)
        is_fixed = np.zeros(n, dtype=bool)

        fixed_set = set(scd.get("fixed_particle_indices", []))

        for i, p in enumerate(particles):
            pos = p.get("position", [0.0, 0.0, 0.0])
            if len(pos) >= 3:
                positions[i] = np.array(pos[:3], dtype=np.float32)
            masses[i] = float(p.get("mass", 0.0))
            inv_masses[i] = float(p.get("inv_mass", -1.0))
            radii[i] = float(p.get("radius", 0.0))
            is_fixed[i] = i in fixed_set

        # Convert from Havok space to NIF space
        _havok_to_nif(positions, self._z_offset)

        self.particle_data = ParticleData(
            positions=positions, masses=masses,
            inv_masses=inv_masses, radii=radii,
            is_fixed=is_fixed,
        )

    def _extract_constraints(self, scd: dict) -> None:
        """Extract constraint links for overlay rendering and bend quads."""
        self.constraint_links.clear()
        self.bend_quads.clear()

        for cs in scd.get("constraint_sets", []):
            class_name = cs.get("class_name", "")
            ctype = _classify_constraint(class_name)
            links = _extract_links_from_constraint_json(cs, ctype)
            self.constraint_links.extend(links)
            if ctype == "bend":
                self.bend_quads.extend(_extract_bend_quads_json(cs))

    def _extract_collidables(self, scd: dict) -> None:
        """Extract capsule and sphere shapes for overlay rendering."""
        self.capsules.clear()
        self.spheres.clear()

        for col in scd.get("collidables", []):
            _extract_collidable_from_json(col, self.capsules, self.spheres,
                                         self.bone_world_transforms, self._z_offset)

    def world_segment_to_bone_local(
        self,
        collidable_bone_name: str,
        start_world: np.ndarray,
        end_world: np.ndarray,
    ) -> tuple[np.ndarray, np.ndarray]:
        """Inverse of the capsule branch in ``_extract_collidable_from_json``: world-space segment → bone-local.

        Undoes the Havok→NIF ``_z_offset`` shift and the collidable bone's
        world transform so the values can be written back to an
        ``hclCapsuleShape``. Falls back to an identity transform if the
        collidable bone cannot be resolved (same policy as extraction).
        """
        start_w = np.asarray(start_world, dtype=np.float64).copy()
        end_w = np.asarray(end_world, dtype=np.float64).copy()
        start_w[2] += self._z_offset
        end_w[2] += self._z_offset
        nif_bone = _resolve_collidable_bone(
            collidable_bone_name, set(self.bone_world_transforms.keys()),
        )
        if nif_bone is None:
            return start_w, end_w
        bone_trans, bone_rot = self.bone_world_transforms[nif_bone]
        inv_rot = np.asarray(bone_rot, dtype=np.float64).T
        local_s = inv_rot @ (start_w - np.asarray(bone_trans, dtype=np.float64))
        local_e = inv_rot @ (end_w - np.asarray(bone_trans, dtype=np.float64))
        return local_s, local_e

    def refresh_from_blob(self, new_blob: bytes) -> None:
        """Update scene state after any native mutation that returns new blob bytes.

        """
        from creation_lib._native import havok_native  # noqa: PLC0415
        self.blob = new_blob
        self.cloth_json = json.loads(havok_native.cloth_inspect_full_json(new_blob))
        self._extract_overlay_data()


# --- Helper functions ---

def _classify_constraint(class_name: str) -> str:
    """Map HCL constraint class name to overlay type label."""
    mapping = {
        "hclStandardLinkConstraintSet": "standard",
        "hclStretchLinkConstraintSet": "stretch",
        "hclBendStiffnessConstraintSet": "bend",
        "hclLocalRangeConstraintSet": "localrange",
    }
    return mapping.get(class_name, "unknown")


def _extract_bend_quads_json(cs: dict) -> list[tuple[int, int, int, int, float]]:
    """Extract bend quad tuples (a,b,c,d,stiffness) from a constraint set JSON dict."""
    out: list[tuple[int, int, int, int, float]] = []
    for entry in cs.get("links", []):
        a = int(entry.get("particleA", -1))
        b = int(entry.get("particleB", -1))
        c = int(entry.get("particleC", -1))
        d = int(entry.get("particleD", -1))
        stiff = abs(float(entry.get("bendStiffness", 1.0)))
        if a >= 0 and b >= 0 and c >= 0 and d >= 0:
            out.append((a, b, c, d, stiff))
    return out


def _extract_links_from_constraint_json(cs: dict, ctype: str) -> list[ConstraintLink]:
    """Extract particle-pair links from a constraint set JSON dict."""
    links = []
    for entry in cs.get("links", []):
        if ctype == "bend":
            a = int(entry.get("particleA", -1))
            b = int(entry.get("particleB", -1))
            c = int(entry.get("particleC", -1))
            d = int(entry.get("particleD", -1))
            stiff = abs(float(entry.get("bendStiffness", 1.0)))
            if a >= 0 and b >= 0:
                links.append(ConstraintLink(a, b, ctype, stiff))
            if c >= 0 and d >= 0:
                links.append(ConstraintLink(c, d, ctype, stiff))
        else:
            a = int(entry.get("particleA", -1))
            b = int(entry.get("particleB", -1))
            stiff = float(entry.get("stiffness", 1.0))
            if a >= 0 and b >= 0:
                links.append(ConstraintLink(a, b, ctype, stiff))
    return links


def _extract_collidable_from_json(
    col: dict,
    capsules: list,
    spheres: list,
    bone_transforms: dict[str, tuple[np.ndarray, np.ndarray]],
    z_offset: float = 0.0,
) -> None:
    """Extract capsule/sphere geometry from a collidable JSON dict."""
    shape_class = col.get("shape_class", "")
    bone_name = col.get("name", "")

    if shape_class == "hclCapsuleShape":
        try:
            start = np.array(col.get("start", [0.0, 0.0, 0.0])[:3], dtype=np.float32)
            end = np.array(col.get("end", [0.0, 0.0, 0.0])[:3], dtype=np.float32)
            radius = float(col.get("radius", 1.0))

            nif_bone = _resolve_collidable_bone(bone_name, set(bone_transforms.keys()))
            if nif_bone is not None:
                bone_trans, bone_rot = bone_transforms[nif_bone]
                start = (bone_rot @ start.astype(np.float64) + bone_trans).astype(np.float32)
                end = (bone_rot @ end.astype(np.float64) + bone_trans).astype(np.float32)
            else:
                _log.debug("No NIF bone found for collidable %r", bone_name)

            start[2] -= z_offset
            end[2] -= z_offset
            capsules.append(CapsuleData(start=start, end=end, radius=radius, bone_name=bone_name))
        except Exception as e:
            _log.debug("Failed to extract capsule from JSON: %s", e)

    elif shape_class == "hclSphereShape":
        try:
            center = np.array(col.get("center", [0.0, 0.0, 0.0])[:3], dtype=np.float32)
            radius = float(col.get("radius", 1.0))

            nif_bone = _resolve_collidable_bone(bone_name, set(bone_transforms.keys()))
            if nif_bone is not None:
                bone_trans, bone_rot = bone_transforms[nif_bone]
                center = (bone_rot @ center.astype(np.float64) + bone_trans).astype(np.float32)
            else:
                _log.debug("No NIF bone found for sphere collidable %r", bone_name)

            center[2] -= z_offset
            spheres.append(SphereData(center=center, radius=radius, bone_name=bone_name))
        except Exception as e:
            _log.debug("Failed to extract sphere from JSON: %s", e)
