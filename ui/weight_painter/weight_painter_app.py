"""Weight Painter — core orchestrator.

Weight painting and auto-skinning tool with GPU mesh preview.
Reuses rendering infrastructure from ui.editor and creation_lib.nif.rendering.
Integrated into the toolkit as WeightPainterWorkspace.
"""
from __future__ import annotations

import copy
import logging
from pathlib import Path
from typing import TYPE_CHECKING

import moderngl
import numpy as np

from ui.mesh_workspace.base_app import MeshWorkspaceBase

_log = logging.getLogger("weight_painter.app")


class WeightPainterApp(MeshWorkspaceBase):
    """Weight painting and auto-skinning editor with 3D preview."""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings=toolkit_settings)
        self.status_text = "Import a mesh to begin"

        # State
        self.skin_data = None  # SkinData — current mesh being edited
        self.reference_skin = None  # SkinData — reference body
        self.selected_bone_idx: int = -1
        self.selected_bone_name: str = ""
        self.file_path: str = ""
        self.nif_file = None  # Loaded NIF for re-export
        self.modified = False

        # Skinned rendering
        self.skinned_renderer = None  # SkinnedRenderer instance
        self.skinned_mesh = None  # SkinnedMesh GPU object
        self.reference_skinned_mesh = None  # SkinnedMesh for reference body overlay
        self.weight_overlay = None  # WeightOverlay state
        self.mesh_picker = None  # MeshPicker for ray-cast brush

        # Display mode: "shaded" (default), "weights" (heatmap), "segments", "all_weights"
        self.display_mode: str = "weights"

        # Vertex mask: 0.0=editable, 1.0=locked. None until mesh loaded.
        self.mask: np.ndarray | None = None
        self.show_mask: bool = True  # Visual feedback for masked vertices
        self._mask_vbo: object | None = None  # moderngl.Buffer

        # Segment selection (for highlight)
        self.selected_segment_id: int = -1

        # Adjacency (built once per mesh for smooth/flood brushes)
        self._adjacency: list[set[int]] | None = None

        # Undo — each entry stores weights, bone indices, segment IDs, segments, and mask.
        self.undo_stack: list[
            tuple[
                str,
                np.ndarray,
                np.ndarray,
                np.ndarray | None,
                list,
                np.ndarray | None,
            ]
        ] = []
        self.redo_stack: list[
            tuple[
                str,
                np.ndarray,
                np.ndarray,
                np.ndarray | None,
                list,
                np.ndarray | None,
            ]
        ] = []
        self._max_undo = 50

        # Wireframe overlay
        self.show_wireframe: bool = False
        self._wireframe_vao: object | None = None  # moderngl.VertexArray
        self._wireframe_line_count: int = 0

        # Segment boundary edges (overlay on top of any display mode)
        self.show_segment_edges: bool = True
        self._segment_edge_vao: object | None = None  # moderngl.VertexArray
        self._segment_edge_count: int = 0

        # Brush cursor (3D ring on mesh surface)
        self.brush_cursor = None  # BrushCursor instance
        self.brush_cursor_pos: np.ndarray | None = None  # World-space hit point
        self.brush_cursor_normal: np.ndarray | None = None  # Surface normal at hit

        # Brush state
        self.brush_type: str = "paint"  # paint, smooth, blur, gradient, mirror, flood
        self.brush_radius: float = 5.0
        self.brush_strength: float = 0.5
        self.brush_falloff: float = 0.5
        self.paint_mode: str = "add"  # add, subtract, set
        self.auto_normalize: bool = True
        self.mirror_x: bool = False
        # Gradient brush state (two-click workflow)
        self.gradient_start: np.ndarray | None = None  # First click point
        self.gradient_pending: bool = False  # True after first click, waiting for second
        # Bone weight clipboard (copy/paste between bones)
        self._copied_bone_idx: int = -1
        self._copied_bone_name: str = ""
        self._copied_weights: np.ndarray | None = None  # (N,) per-vertex weights

        # Diffuse texture (loaded on import, bound during render)
        self._diffuse_texture: object | None = None  # moderngl.Texture

        # Import dialog state
        self._show_import_dialog: bool = False
        self._import_path: str = ""

        # Export dialog state
        self._show_export_dialog: bool = False
        self._export_path: str = ""

        # Reference mesh dialog state
        self._show_reference_dialog: bool = False

        # Transfer weights dialog state
        self._show_transfer_dialog: bool = False
        self._transfer_method: int = 2  # 0=Barycentric, 1=Proximity, 2=Hybrid
        self._transfer_bone_filter: str = ""
        self._transfer_segments: bool = True
        self._transfer_source: str = "reference"  # "reference" or "file"
        self._transfer_file_path: str = ""
        self._transfer_stats: str = ""

        # Panels (created in _init_panels)
        self.viewport_panel = None
        self.bone_tree_panel = None
        self.brush_panel = None
        self.segment_panel = None

    def setup(self):
        """Called on first frame when GL context is available."""
        super().setup()

        from creation_lib.nif.rendering import SkinnedRenderer, WeightOverlay
        self.skinned_renderer = SkinnedRenderer(self.ctx)
        self.weight_overlay = WeightOverlay()

        _log.info("Weight painter setup complete")

    def _init_panels(self):
        """Create panel instances."""
        if self._panels_initialized:
            return
        from .panels.viewport_panel import ViewportPanel
        from .panels.bone_tree_panel import BoneTreePanel
        from .panels.brush_panel import BrushPanel
        from .panels.segment_panel import SegmentPanel

        self.viewport_panel = ViewportPanel(self)
        self.bone_tree_panel = BoneTreePanel(self)
        self.brush_panel = BrushPanel(self)
        self.segment_panel = SegmentPanel(self)
        self._panels_initialized = True

    def draw_workspace(self):
        """Not used — panels are drawn via toolkit dockable windows."""
        pass

    # ------------------------------------------------------------------
    # Import / Export
    # ------------------------------------------------------------------

    def promote_unskinned_shapes(self, nif) -> list[str]:
        """Give any unskinned BSTriShapes a default root bone + rigid weights.

        Returns the names of shapes that were promoted.
        """
        from creation_lib.nif.operations.skinning import (
            add_bone_node,
            make_shape_skinned,
            set_rigid_weights,
        )

        promoted: list[str] = []
        candidates: list[tuple[int, str]] = []
        for block in nif.blocks:
            if not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
                continue
            skin = block.get_field("Skin")
            if skin is not None and int(skin) >= 0:
                continue
            name = block.get_field("Name") or f"Shape{block.block_id}"
            candidates.append((block.block_id, str(name)))

        for shape_id, name in candidates:
            bone_id = add_bone_node(
                nif, f"{name}_Root", translation=(0.0, 0.0, 0.0), parent_id=0,
            )
            make_shape_skinned(nif, shape_id, bone_ids=[bone_id])
            set_rigid_weights(nif, shape_id, bone_local_index=0)
            promoted.append(name)

        return promoted

    def import_mesh(self, path: str):
        """Import a mesh file (.nif, .obj) and set up for editing."""
        p = Path(path)
        if not p.exists():
            self.status_text = f"File not found: {path}"
            _log.warning("Import failed: %s", self.status_text)
            return

        ext = p.suffix.lower()
        try:
            if ext == ".nif":
                from creation_lib.nif import NifFile
                from creation_lib.skinning.reference_body import extract_skin_data_from_nif

                self.nif_file = NifFile.load(path)
                promoted = self.promote_unskinned_shapes(self.nif_file)
                if promoted:
                    self.status_text = (
                        f"Promoted {len(promoted)} unskinned "
                        f"shape{'s' if len(promoted) != 1 else ''}: "
                        f"{', '.join(promoted)}"
                    )
                    _log.info(self.status_text)
                # Extract directly from the in-memory NIF so the painter's
                # nif_file and skin_data stay in lockstep — no silent
                # round-trip through the source path on disk.
                self.skin_data = extract_skin_data_from_nif(self.nif_file)
            elif ext == ".obj":
                from creation_lib.skinning.importers import import_obj
                self.skin_data = import_obj(path)
                self.nif_file = None
            else:
                self.status_text = f"Unsupported format: {ext}"
                return
        except Exception as e:
            self.status_text = f"Import error: {e}"
            _log.error("Import failed: %s", e, exc_info=True)
            return

        self.file_path = str(p)
        self.selected_bone_idx = -1
        self.selected_bone_name = ""
        self.undo_stack.clear()
        self.redo_stack.clear()
        self.modified = False

        # Initialize vertex mask (all editable)
        self.mask = np.zeros(self.skin_data.num_vertices, dtype=np.float32)

        # Build adjacency for smooth/flood brushes
        from creation_lib.skinning.brushes import build_adjacency
        self._adjacency = build_adjacency(
            self.skin_data.triangles, self.skin_data.num_vertices
        )

        # Update mesh picker
        if self.mesh_picker:
            self.mesh_picker.set_mesh(self.skin_data.vertices, self.skin_data.triangles)

        # Build GPU mesh
        self._build_gpu_mesh()

        # Load diffuse texture
        self._load_diffuse_texture()

        # Frame camera on mesh bounds
        self._frame_camera_on_mesh(self.skin_data)

        # Auto-select first bone so weight heatmap is immediately visible
        if self.skin_data.bone_names:
            self.select_bone(0)

        self.status_text = (
            f"Loaded: {p.name} — "
            f"{self.skin_data.num_vertices} verts, "
            f"{self.skin_data.num_triangles} tris, "
            f"{len(self.skin_data.bone_names)} bones"
        )
        _log.info(self.status_text)

    def _load_diffuse_texture(self) -> None:
        """Try to load a diffuse texture for the first BSTriShape with one."""
        if not self.nif_file or not self.ctx:
            return
        # Release previous texture
        if self._diffuse_texture is not None:
            try:
                self._diffuse_texture.release()
            except Exception:
                pass
            self._diffuse_texture = None

        texture_dirs = self.get_texture_dirs()
        if not texture_dirs:
            _log.debug("No texture dirs configured — skipping diffuse load")
            return

        try:
            from ui.mesh_workspace.texture_loader import resolve_diffuse_for_shape
            for block in self.nif_file.blocks:
                if block.type_name in ("BSTriShape", "BSSubIndexTriShape",
                                        "BSMeshLODTriShape"):
                    tex = resolve_diffuse_for_shape(
                        self.ctx, self.nif_file, block, texture_dirs,
                    )
                    if tex is not None:
                        self._diffuse_texture = tex
                        if self.skinned_mesh is not None:
                            self.skinned_mesh.diffuse_texture_id = 1
                        return
        except Exception as e:
            _log.debug("Diffuse texture load failed: %s", e)

    def load_reference(self, path: str):
        """Load a reference body NIF whose weights will be transferred to the target mesh."""
        p = Path(path)
        if not p.exists():
            self.status_text = f"Reference not found: {path}"
            return
        try:
            from creation_lib.skinning.reference_body import extract_skin_data_from_nif
            self.reference_skin = extract_skin_data_from_nif(path)
            self._build_reference_gpu_mesh()
            # Frame camera on reference if no target mesh loaded yet
            if self.skin_data is None:
                self._frame_camera_on_mesh(self.reference_skin)
            self.status_text = (
                f"Reference: {p.name} — "
                f"{self.reference_skin.num_vertices} verts, "
                f"{len(self.reference_skin.bone_names)} bones"
            )
            _log.info("Loaded reference: %s", self.status_text)
        except Exception as e:
            self.status_text = f"Reference load error: {e}"
            _log.error("Reference load failed: %s", e, exc_info=True)

    def auto_skin(self, method: str = "hybrid"):
        """Run auto-skinning against reference body.

        Args:
            method: "barycentric", "proximity", or "hybrid".
        """
        if self.skin_data is None:
            self.status_text = "No mesh loaded"
            return

        if self.reference_skin is None:
            self.status_text = "No reference body loaded — assign bone names first"
            _log.warning("Auto-skin skipped: no reference body")
            return

        self.push_undo("Auto-skin")

        from creation_lib.skinning.weight_transfer import transfer_weights
        from creation_lib.skinning.normalization import normalize_weights
        from creation_lib.skinning.partitions import assign_partitions_from_bones

        try:
            weights, bone_indices, stats = transfer_weights(
                source=self.reference_skin,
                target_vertices=self.skin_data.vertices,
                target_triangles=self.skin_data.triangles,
                method=method,
            )

            # Copy bone names from reference
            self.skin_data.bone_names = list(self.reference_skin.bone_names)

            # Normalize weights
            weights, bone_indices, mod_count = normalize_weights(
                weights, bone_indices, max_bones=4,
            )

            self.skin_data.weights = weights
            self.skin_data.bone_indices = bone_indices

            body_part_ids = assign_partitions_from_bones(self.skin_data)
            self._set_segments_from_body_parts(body_part_ids)

            self.modified = True
            self._rebuild_weight_buffer()
            self._rebuild_segment_colors()

            self.status_text = (
                f"Auto-skin ({method}): {stats['transferred']} verts, "
                f"{stats.get('fallback_count', 0)} fallbacks"
            )
            _log.info(self.status_text)
        except Exception as e:
            self.status_text = f"Auto-skin error: {e}"
            _log.error("Auto-skin failed: %s", e, exc_info=True)

    def transfer_weights_from_mesh(
        self,
        source_path: str | None = None,
        method: str = "hybrid",
        bone_filter: str = "",
        transfer_segments: bool = True,
    ):
        """Transfer weights from a source mesh to the current target mesh.

        Args:
            source_path: Path to source NIF. If None, uses the loaded reference body.
            method: "barycentric", "proximity", or "hybrid".
            bone_filter: If non-empty, only transfer bones whose names contain this
                substring (case-insensitive).
            transfer_segments: If True, also transfer segment assignments.
        """
        if self.skin_data is None:
            self.status_text = "No target mesh loaded"
            return

        # Resolve source skin data
        if source_path:
            try:
                from creation_lib.skinning.reference_body import extract_skin_data_from_nif
                source_skin = extract_skin_data_from_nif(source_path)
            except Exception as e:
                self.status_text = f"Source load error: {e}"
                _log.error("Transfer source load failed: %s", e, exc_info=True)
                return
        elif self.reference_skin is not None:
            source_skin = self.reference_skin
        else:
            self.status_text = "No source mesh — load a reference body or browse for a NIF"
            return

        if source_skin.num_vertices == 0:
            self.status_text = "Source mesh is empty"
            return

        self.push_undo("Transfer Weights")

        from creation_lib.skinning.weight_transfer import transfer_weights
        from creation_lib.skinning.normalization import normalize_weights

        try:
            weights, bone_indices, stats = transfer_weights(
                source=source_skin,
                target_vertices=self.skin_data.vertices,
                target_triangles=self.skin_data.triangles,
                method=method,
            )

            # Build bone name mapping: source index -> target index
            # Add any new bones from source that don't exist in target
            target_bone_names = list(self.skin_data.bone_names)
            src_to_tgt: dict[int, int] = {}
            filter_lower = bone_filter.strip().lower()

            for si, src_name in enumerate(source_skin.bone_names):
                # Apply bone filter
                if filter_lower and filter_lower not in src_name.lower():
                    continue
                if src_name in target_bone_names:
                    src_to_tgt[si] = target_bone_names.index(src_name)
                else:
                    src_to_tgt[si] = len(target_bone_names)
                    target_bone_names.append(src_name)

            if not src_to_tgt:
                self.status_text = "No matching bones between source and target"
                return

            # Remap bone indices from source space to target space
            remapped_bi = np.zeros_like(bone_indices)
            remapped_w = np.zeros_like(weights)
            for vi in range(len(weights)):
                for j in range(weights.shape[1]):
                    src_bi = int(bone_indices[vi, j])
                    w = float(weights[vi, j])
                    if w > 0 and src_bi in src_to_tgt:
                        remapped_bi[vi, j] = src_to_tgt[src_bi]
                        remapped_w[vi, j] = w
                    elif w > 0:
                        # Bone was filtered out — keep existing weight if any
                        pass

            # If bone filter is active, merge with existing weights (don't clobber unfiltered bones)
            if filter_lower:
                # Only overwrite slots for filtered bones
                filtered_tgt_indices = set(src_to_tgt.values())
                for vi in range(self.skin_data.num_vertices):
                    # Collect existing weights for non-filtered bones
                    existing: dict[int, float] = {}
                    for j in range(self.skin_data.weights.shape[1]):
                        bi = int(self.skin_data.bone_indices[vi, j])
                        w = float(self.skin_data.weights[vi, j])
                        if w > 0 and bi not in filtered_tgt_indices:
                            existing[bi] = existing.get(bi, 0.0) + w
                    # Add transferred weights for filtered bones
                    for j in range(remapped_w.shape[1]):
                        bi = int(remapped_bi[vi, j])
                        w = float(remapped_w[vi, j])
                        if w > 0:
                            existing[bi] = existing.get(bi, 0.0) + w
                    # Pack back into arrays
                    sorted_bw = sorted(existing.items(), key=lambda x: -x[1])[:4]
                    remapped_w[vi] = 0.0
                    remapped_bi[vi] = 0
                    for j, (bi, w) in enumerate(sorted_bw):
                        remapped_bi[vi, j] = bi
                        remapped_w[vi, j] = w

            # Update bone names
            self.skin_data.bone_names = target_bone_names

            # Normalize
            remapped_w, remapped_bi, _ = normalize_weights(
                remapped_w, remapped_bi, max_bones=4,
            )

            self.skin_data.weights = remapped_w
            self.skin_data.bone_indices = remapped_bi

            if transfer_segments and source_skin.segment_ids is not None:
                self._set_segments_from_reference(source_skin)

            self.modified = True
            self._rebuild_weight_buffer()
            self._rebuild_segment_colors()

            mapped_count = len(src_to_tgt)
            self._transfer_stats = (
                f"{stats['transferred']} verts, {mapped_count} bones mapped, "
                f"{stats.get('fallback_count', 0)} fallbacks"
            )
            self.status_text = f"Transfer ({method}): {self._transfer_stats}"
            _log.info(self.status_text)
        except Exception as e:
            self.status_text = f"Transfer error: {e}"
            _log.error("Weight transfer failed: %s", e, exc_info=True)

    def export_nif(self, path: str):
        """Export the current skin data as a game-ready NIF.

        If the mesh was originally loaded from a NIF, the original NIF is
        used as a template. Otherwise, this is a stub that reports
        the limitation.
        """
        if self.skin_data is None:
            self.status_text = "No mesh to export"
            return

        if self.nif_file is None:
            self.status_text = "Export requires an original NIF — re-import from NIF first"
            _log.warning("Export skipped: no NIF file template")
            return

        try:
            from creation_lib.skinning.nif_export import write_skin_data_to_nif
            from creation_lib.skinning.reference_body import extract_skin_data_from_nif

            # Safety net: never write an unskinned NIF. If any BSTriShape in
            # the in-memory template still has Skin == -1 (import edge case,
            # stale template), promote it with a default root bone + rigid
            # weights and re-derive skin_data so write_skin_data_to_nif has
            # matching bones to map against.
            rescued = self.promote_unskinned_shapes(self.nif_file)
            if rescued:
                _log.warning(
                    "Export rescue: promoted %d unskinned shape(s): %s",
                    len(rescued), rescued,
                )
                self.skin_data = extract_skin_data_from_nif(self.nif_file)

            write_skin_data_to_nif(self.nif_file, self.skin_data, path)
            self.modified = False
            self.status_text = f"Exported: {Path(path).name}"
            _log.info("NIF exported to %s", path)
        except Exception as e:
            self.status_text = f"Export error: {e}"
            _log.error("NIF export failed: %s", e, exc_info=True)

    # ------------------------------------------------------------------
    # Bone selection
    # ------------------------------------------------------------------

    def select_bone(self, bone_idx: int):
        """Select a bone for painting/viewing."""
        if self.skin_data is None:
            return
        if bone_idx < 0 or bone_idx >= len(self.skin_data.bone_names):
            self.selected_bone_idx = -1
            self.selected_bone_name = ""
            if self.weight_overlay:
                self.weight_overlay.clear()
        else:
            self.selected_bone_idx = bone_idx
            self.selected_bone_name = self.skin_data.bone_names[bone_idx]
            if self.weight_overlay:
                self.weight_overlay.select_bone(bone_idx, self.selected_bone_name)

        self.status_text = (
            f"Bone: {self.selected_bone_name}"
            if self.selected_bone_name else "No bone selected"
        )

    def close_mesh(self):
        """Clear current mesh and reset editor state."""
        if self._diffuse_texture is not None:
            try:
                self._diffuse_texture.release()
            except Exception:
                pass
            self._diffuse_texture = None
        if self.skinned_mesh is not None:
            try:
                self.skinned_mesh.vao.release()
            except Exception:
                pass
            self.skinned_mesh = None
        if self._wireframe_vao is not None:
            try:
                self._wireframe_vao.release()
            except Exception:
                pass
            self._wireframe_vao = None
            self._wireframe_line_count = 0
        self.skin_data = None
        self.nif_file = None
        self.file_path = ""
        self.selected_bone_idx = -1
        self.selected_bone_name = ""
        self.selected_segment_id = -1
        self.display_mode = "weights"
        self._adjacency = None
        self.undo_stack.clear()
        self.redo_stack.clear()
        self.modified = False
        self.gradient_pending = False
        self.gradient_start = None
        self._copied_weights = None
        self._copied_bone_idx = -1
        self._copied_bone_name = ""
        self.mask = None
        self._mask_vbo = None
        self.status_text = "Import a mesh to begin"
        _log.info("Mesh closed, editor state reset")

    def select_segment(self, part_id: int):
        """Select a segment for highlighting. -1 to deselect."""
        old = self.selected_segment_id
        self.selected_segment_id = part_id
        if old != part_id:
            self._rebuild_segment_colors()
        if part_id >= 0:
            self.display_mode = "segments"

    def set_display_mode(self, mode: str):
        """Switch display mode and update GPU colors if needed."""
        old = self.display_mode
        self.display_mode = mode
        if mode == "all_weights":
            self._upload_all_bone_colors()
        elif mode == "segments" and old != "segments":
            self._rebuild_segment_colors()

    def _compute_all_bone_colors(self) -> np.ndarray:
        """Compute per-vertex blended colors showing all bone influences.

        Each bone gets a unique hue. Per-vertex color is the weighted blend
        of its influencing bones' colors. Returns (N, 3) float32 array.
        """
        sd = self.skin_data
        if sd is None:
            return np.zeros((0, 3), dtype=np.float32)

        n_bones = len(sd.bone_names)
        n_verts = sd.num_vertices

        # Generate unique hue per bone using golden ratio for good distribution
        bone_colors = np.zeros((max(n_bones, 1), 3), dtype=np.float32)
        for i in range(n_bones):
            hue = (i * 0.618033988749895) % 1.0  # Golden ratio
            # HSV to RGB (saturation=0.85, value=0.95)
            bone_colors[i] = self._hsv_to_rgb(hue, 0.85, 0.95)

        # Blend per-vertex: sum(weight_j * bone_color[bone_index_j])
        colors = np.zeros((n_verts, 3), dtype=np.float32)
        for j in range(sd.weights.shape[1]):
            w = sd.weights[:, j]  # (N,)
            bi = sd.bone_indices[:, j]  # (N,)
            mask = w > 0
            if not mask.any():
                continue
            # Gather bone colors for this slot
            valid_bi = np.clip(bi[mask], 0, n_bones - 1)
            colors[mask] += w[mask, np.newaxis] * bone_colors[valid_bi]

        # Clamp
        np.clip(colors, 0.0, 1.0, out=colors)
        return colors

    def _upload_all_bone_colors(self):
        """Upload all-bone blended colors to the segment color VBO."""
        if not hasattr(self, '_segment_color_vbo') or self._segment_color_vbo is None:
            return
        if self.skin_data is None:
            return
        colors = self._compute_all_bone_colors()
        try:
            self._segment_color_vbo.write(colors.tobytes())
        except Exception:
            self._build_gpu_mesh()

    @staticmethod
    def _hsv_to_rgb(h: float, s: float, v: float) -> np.ndarray:
        """Convert HSV to RGB. Returns (3,) float32 array."""
        import colorsys
        r, g, b = colorsys.hsv_to_rgb(h, s, v)
        return np.array([r, g, b], dtype=np.float32)

    # ------------------------------------------------------------------
    # Undo / Redo
    # ------------------------------------------------------------------

    def push_undo(self, label: str):
        """Save current editable state to undo stack."""
        if self.skin_data is None:
            return
        part_copy = (
            self.skin_data.segment_ids.copy()
            if self.skin_data.segment_ids is not None
            else None
        )
        mask_copy = self.mask.copy() if self.mask is not None else None
        self.undo_stack.append((
            label,
            self.skin_data.weights.copy(),
            self.skin_data.bone_indices.copy(),
            part_copy,
            copy.deepcopy(self.skin_data.segments),
            mask_copy,
        ))
        if len(self.undo_stack) > self._max_undo:
            self.undo_stack.pop(0)
        self.redo_stack.clear()

    def undo(self):
        """Restore previous weight and segment state."""
        if not self.undo_stack or self.skin_data is None:
            return
        # Save current state to redo
        part_copy = (
            self.skin_data.segment_ids.copy()
            if self.skin_data.segment_ids is not None
            else None
        )
        mask_copy = self.mask.copy() if self.mask is not None else None
        self.redo_stack.append((
            "redo",
            self.skin_data.weights.copy(),
            self.skin_data.bone_indices.copy(),
            part_copy,
            copy.deepcopy(self.skin_data.segments),
            mask_copy,
        ))
        label, weights, bone_indices, seg_ids, segments, mask = self.undo_stack.pop()
        self.skin_data.weights = weights
        self.skin_data.bone_indices = bone_indices
        if seg_ids is not None:
            self.skin_data.segment_ids = seg_ids
        self.skin_data.segments = segments
        if mask is not None:
            self.mask = mask
        self._rebuild_weight_buffer()
        self._rebuild_segment_submeshes()
        self._rebuild_mask_buffer()
        self.status_text = f"Undo: {label}"

    def redo(self):
        """Re-apply undone weight/segment change."""
        if not self.redo_stack or self.skin_data is None:
            return
        # Save current state to undo
        part_copy = (
            self.skin_data.segment_ids.copy()
            if self.skin_data.segment_ids is not None
            else None
        )
        mask_copy = self.mask.copy() if self.mask is not None else None
        self.undo_stack.append((
            "redo",
            self.skin_data.weights.copy(),
            self.skin_data.bone_indices.copy(),
            part_copy,
            copy.deepcopy(self.skin_data.segments),
            mask_copy,
        ))
        label, weights, bone_indices, seg_ids, segments, mask = self.redo_stack.pop()
        self.skin_data.weights = weights
        self.skin_data.bone_indices = bone_indices
        if seg_ids is not None:
            self.skin_data.segment_ids = seg_ids
        self.skin_data.segments = segments
        if mask is not None:
            self.mask = mask
        self._rebuild_weight_buffer()
        self._rebuild_segment_submeshes()
        self._rebuild_mask_buffer()
        self.status_text = f"Redo: {label}"

    # ------------------------------------------------------------------
    # Brush application
    # ------------------------------------------------------------------

    def _connected_vertex_selection(self, hit_tri_idx: int | None) -> np.ndarray | None:
        if self.skin_data is None or self._adjacency is None or hit_tri_idx is None:
            return None
        sd = self.skin_data
        if hit_tri_idx < 0 or hit_tri_idx >= sd.num_triangles:
            return None

        selected = np.zeros(sd.num_vertices, dtype=bool)
        stack = [int(v) for v in sd.triangles[hit_tri_idx]]
        for vi in stack:
            selected[vi] = True
        while stack:
            vi = stack.pop()
            for ni in self._adjacency[vi]:
                if not selected[ni]:
                    selected[ni] = True
                    stack.append(ni)
        return selected

    def _connected_triangle_selection(self, hit_tri_idx: int | None) -> np.ndarray | None:
        vertex_selection = self._connected_vertex_selection(hit_tri_idx)
        if self.skin_data is None or vertex_selection is None:
            return None
        return np.any(vertex_selection[self.skin_data.triangles], axis=1)

    def _nearest_triangle_index(self, point: np.ndarray) -> int | None:
        if self.skin_data is None or self.skin_data.num_triangles == 0:
            return None
        sd = self.skin_data
        v0 = sd.vertices[sd.triangles[:, 0]]
        v1 = sd.vertices[sd.triangles[:, 1]]
        v2 = sd.vertices[sd.triangles[:, 2]]
        centroids = (v0 + v1 + v2) / 3.0
        return int(np.argmin(np.linalg.norm(centroids - point, axis=1)))

    def _mirrored_bone_index(self, bone_idx: int) -> int:
        if self.skin_data is None or not (0 <= bone_idx < len(self.skin_data.bone_names)):
            return bone_idx
        bone_names = self.skin_data.bone_names
        name = bone_names[bone_idx]
        mirror_pairs = [
            ("LArm", "RArm"), ("LLeg", "RLeg"),
            ("Left", "Right"), ("L_", "R_"), ("l_", "r_"), ("L", "R"),
        ]
        for left_pat, right_pat in mirror_pairs:
            if left_pat in name:
                mirror_name = name.replace(left_pat, right_pat, 1)
                if mirror_name in bone_names:
                    return bone_names.index(mirror_name)
            if right_pat in name:
                mirror_name = name.replace(right_pat, left_pat, 1)
                if mirror_name in bone_names:
                    return bone_names.index(mirror_name)
        return bone_idx

    def _apply_weight_brush_at(
        self,
        brush_center: np.ndarray,
        bone_idx: int,
        selection_mask: np.ndarray | None,
    ) -> None:
        from creation_lib.skinning import brushes

        sd = self.skin_data
        if sd is None:
            return
        bt = self.brush_type

        if bt == "paint":
            sd.weights, sd.bone_indices = brushes.paint_weight(
                sd.weights, sd.bone_indices,
                bone_idx=bone_idx,
                vertex_positions=sd.vertices,
                brush_center=brush_center,
                brush_radius=self.brush_radius,
                brush_strength=self.brush_strength,
                mode=self.paint_mode,
                falloff=self.brush_falloff,
                auto_normalize=self.auto_normalize,
                vertex_mask=self.mask,
                selection_mask=selection_mask,
            )
        elif bt == "smooth":
            if self._adjacency is None:
                return
            sd.weights, sd.bone_indices = brushes.smooth_weights(
                sd.weights, sd.bone_indices,
                adjacency=self._adjacency,
                vertex_positions=sd.vertices,
                brush_center=brush_center,
                brush_radius=self.brush_radius,
                brush_strength=self.brush_strength,
                vertex_mask=self.mask,
                selection_mask=selection_mask,
            )
        elif bt == "blur":
            sd.weights, sd.bone_indices = brushes.blur_weights(
                sd.weights, sd.bone_indices,
                vertex_positions=sd.vertices,
                brush_center=brush_center,
                brush_radius=self.brush_radius,
                brush_strength=self.brush_strength,
                vertex_mask=self.mask,
                selection_mask=selection_mask,
            )
        elif bt == "flood":
            if self._adjacency is None:
                return
            dists = np.linalg.norm(sd.vertices - brush_center, axis=1)
            if selection_mask is not None:
                dists = dists.copy()
                dists[~selection_mask] = np.inf
            start_vi = int(np.argmin(dists))
            if not np.isfinite(dists[start_vi]):
                return
            sd.weights, sd.bone_indices = brushes.flood_fill_weight(
                sd.weights, sd.bone_indices,
                bone_idx=bone_idx,
                weight_value=self.brush_strength,
                start_vertex=start_vi,
                adjacency=self._adjacency,
                auto_normalize=self.auto_normalize,
                vertex_mask=self.mask,
                selection_mask=selection_mask,
            )

    def apply_brush(self, brush_center: np.ndarray, hit_tri_idx: int | None = None):
        """Apply the current brush at the given world-space position."""
        if self.skin_data is None or self.selected_bone_idx < 0:
            return

        from creation_lib.skinning import brushes

        sd = self.skin_data
        bt = self.brush_type

        if bt in ("paint", "smooth", "blur", "flood"):
            selection_mask = self._connected_vertex_selection(hit_tri_idx)
            self._apply_weight_brush_at(
                brush_center, self.selected_bone_idx, selection_mask,
            )
            if self.mirror_x:
                mirrored_center = brush_center.copy()
                mirrored_center[0] = -mirrored_center[0]
                mirror_tri_idx = self._nearest_triangle_index(mirrored_center)
                self._apply_weight_brush_at(
                    mirrored_center,
                    self._mirrored_bone_index(self.selected_bone_idx),
                    self._connected_vertex_selection(mirror_tri_idx),
                )
        elif bt == "gradient":
            # Gradient is handled by apply_gradient() via two-click workflow
            return
        elif bt == "mirror":
            sd.weights, sd.bone_indices = brushes.mirror_weights(
                sd.weights, sd.bone_indices,
                bone_names=sd.bone_names,
                vertex_positions=sd.vertices,
                axis=0,  # X-axis mirror
            )

        self.modified = True
        self._rebuild_weight_buffer()

    def apply_gradient(self, start_point: np.ndarray, end_point: np.ndarray):
        """Apply gradient weights between two points on the mesh."""
        if self.skin_data is None or self.selected_bone_idx < 0:
            return

        from creation_lib.skinning import brushes

        sd = self.skin_data
        sd.weights, sd.bone_indices = brushes.gradient_weights(
            sd.weights, sd.bone_indices,
            bone_idx=self.selected_bone_idx,
            vertex_positions=sd.vertices,
            start_point=start_point,
            end_point=end_point,
            auto_normalize=self.auto_normalize,
        )
        self.modified = True
        self._rebuild_weight_buffer()

    def copy_bone_weights(self):
        """Copy per-vertex weights for the selected bone to clipboard."""
        if self.skin_data is None or self.selected_bone_idx < 0:
            return
        sd = self.skin_data
        bi = self.selected_bone_idx
        per_vert = np.zeros(sd.num_vertices, dtype=np.float32)
        for j in range(sd.weights.shape[1]):
            mask = (sd.bone_indices[:, j] == bi) & (sd.weights[:, j] > 0)
            per_vert[mask] = sd.weights[mask, j]
        self._copied_bone_idx = bi
        self._copied_bone_name = self.selected_bone_name
        self._copied_weights = per_vert
        self.status_text = f"Copied weights from {self.selected_bone_name}"

    def paste_bone_weights(self):
        """Paste copied weights onto the currently selected bone."""
        if (self.skin_data is None or self.selected_bone_idx < 0
                or self._copied_weights is None):
            return
        sd = self.skin_data
        if len(self._copied_weights) != sd.num_vertices:
            self.status_text = "Paste failed: vertex count mismatch"
            return

        from creation_lib.skinning.brushes import _ensure_bone_slot, _normalize_vertex

        self.push_undo("Paste weights")
        target_bi = self.selected_bone_idx
        for vi in range(sd.num_vertices):
            w = float(self._copied_weights[vi])
            if w > 0:
                slot = _ensure_bone_slot(sd.weights, sd.bone_indices, vi, target_bi)
                sd.weights[vi, slot] = w
            if self.auto_normalize:
                _normalize_vertex(sd.weights, vi)
        self.modified = True
        self._rebuild_weight_buffer()
        self.status_text = (
            f"Pasted weights from {self._copied_bone_name} → "
            f"{self.selected_bone_name}"
        )

    def swap_bone_weights(self):
        """Swap weights between the copied bone and the selected bone."""
        if (self.skin_data is None or self.selected_bone_idx < 0
                or self._copied_weights is None or self._copied_bone_idx < 0):
            return
        sd = self.skin_data
        src_bi = self._copied_bone_idx
        dst_bi = self.selected_bone_idx
        if src_bi == dst_bi:
            return

        from creation_lib.skinning.brushes import _ensure_bone_slot, _normalize_vertex

        self.push_undo("Swap weights")
        for vi in range(sd.num_vertices):
            # Get current weight for both bones
            src_w, dst_w = 0.0, 0.0
            src_slot, dst_slot = -1, -1
            for j in range(sd.weights.shape[1]):
                if int(sd.bone_indices[vi, j]) == src_bi and sd.weights[vi, j] > 0:
                    src_w = float(sd.weights[vi, j])
                    src_slot = j
                if int(sd.bone_indices[vi, j]) == dst_bi and sd.weights[vi, j] > 0:
                    dst_w = float(sd.weights[vi, j])
                    dst_slot = j

            if src_w > 0 or dst_w > 0:
                s = _ensure_bone_slot(sd.weights, sd.bone_indices, vi, src_bi)
                d = _ensure_bone_slot(sd.weights, sd.bone_indices, vi, dst_bi)
                sd.weights[vi, s] = dst_w
                sd.weights[vi, d] = src_w
                if self.auto_normalize:
                    _normalize_vertex(sd.weights, vi)

        self.modified = True
        self._rebuild_weight_buffer()
        self.status_text = (
            f"Swapped weights: {self._copied_bone_name} ↔ "
            f"{self.selected_bone_name}"
        )

    # ------------------------------------------------------------------
    # Mask operations
    # ------------------------------------------------------------------

    def apply_mask_brush(
        self,
        brush_center: np.ndarray,
        unmask: bool = False,
        hit_tri_idx: int | None = None,
    ):
        """Paint mask values onto vertices within brush radius.

        Args:
            brush_center: World-space center of the brush.
            unmask: If True, erase mask (set to 0.0); otherwise set to 1.0.
        """
        if self.skin_data is None or self.mask is None:
            return

        from creation_lib.skinning.brushes import _falloff_factor

        verts = self.skin_data.vertices
        dists = np.linalg.norm(verts - brush_center, axis=1)
        in_radius = dists < self.brush_radius
        connected = self._connected_vertex_selection(hit_tri_idx)
        if connected is not None:
            in_radius &= connected
        affected = np.where(in_radius)[0]

        for vi in affected:
            ff = _falloff_factor(float(dists[vi]), self.brush_radius, self.brush_falloff)
            delta = self.brush_strength * ff
            if unmask:
                self.mask[vi] = max(0.0, self.mask[vi] - delta)
            else:
                self.mask[vi] = min(1.0, self.mask[vi] + delta)

        self._rebuild_mask_buffer()

    def clear_mask(self):
        """Set all mask values to 0.0 (all editable)."""
        if self.mask is not None:
            self.push_undo("Clear Mask")
            self.mask[:] = 0.0
            self._rebuild_mask_buffer()
            self.status_text = "Mask cleared"

    def invert_mask(self):
        """Invert all mask values (0→1, 1→0)."""
        if self.mask is not None:
            self.push_undo("Invert Mask")
            self.mask[:] = 1.0 - self.mask
            self._rebuild_mask_buffer()
            self.status_text = "Mask inverted"

    def _rebuild_mask_buffer(self):
        """Update the mask VBO on GPU."""
        if self.mask is None or self._mask_vbo is None:
            return
        try:
            self._mask_vbo.write(self.mask.astype(np.float32).tobytes())
        except Exception:
            pass

    def apply_segment_brush(self, brush_center: np.ndarray, hit_tri_idx: int):
        """Paint segment IDs onto triangles within brush radius.

        Assigns the currently selected segment ID to all triangles whose
        centroid falls within the brush radius of the hit point.
        """
        if self.skin_data is None or self.selected_segment_id < 0:
            return

        sd = self.skin_data
        if sd.segment_ids is None:
            return

        target_id = self.selected_segment_id

        # Compute centroids of all triangles
        v0 = sd.vertices[sd.triangles[:, 0]]
        v1 = sd.vertices[sd.triangles[:, 1]]
        v2 = sd.vertices[sd.triangles[:, 2]]
        centroids = (v0 + v1 + v2) / 3.0

        # Find triangles within brush radius
        dists = np.linalg.norm(centroids - brush_center, axis=1)
        mask = dists < self.brush_radius
        connected = self._connected_triangle_selection(hit_tri_idx)
        if connected is not None:
            mask &= connected

        if not np.any(mask):
            # At minimum, paint the directly hit triangle
            sd.segment_ids[hit_tri_idx] = target_id
        else:
            sd.segment_ids[mask] = target_id

        if sd.segments:
            from creation_lib.skinning.partitions import sync_fo4_segments_from_ids
            sd.segments = sync_fo4_segments_from_ids(sd)

        self.modified = True
        self._rebuild_segment_submeshes()

    def _rebuild_segment_submeshes(self):
        """Rebuild segment submeshes and color VBO after segment changes.

        This does a full GPU mesh rebuild since the index buffer order changes
        when segments are re-sorted.
        """
        if self.skin_data is None or self.skinned_mesh is None:
            return
        self._build_gpu_mesh()

    # ------------------------------------------------------------------
    # Segment assignment
    # ------------------------------------------------------------------

    def _set_segments_from_body_parts(self, body_part_ids: np.ndarray) -> None:
        """Set FO4 segments from body part IDs and store segment indexes."""
        if self.skin_data is None:
            return
        from creation_lib.skinning.partitions import rebuild_fo4_segments_from_body_parts

        segments, segment_ids = rebuild_fo4_segments_from_body_parts(
            self.skin_data, body_part_ids,
        )
        self.skin_data.segments = segments
        self.skin_data.segment_ids = segment_ids

    def _set_segments_from_reference(self, source_skin) -> None:
        """Copy nearest reference segment indexes plus FO4 segment metadata."""
        if self.skin_data is None:
            return
        from creation_lib.skinning.partitions import (
            assign_partitions_from_reference,
            sync_fo4_segments_from_ids,
        )

        self.skin_data.segment_ids = assign_partitions_from_reference(
            self.skin_data, source_skin,
        )
        if source_skin.segments:
            self.skin_data.segments = copy.deepcopy(source_skin.segments)
            self.skin_data.segments = sync_fo4_segments_from_ids(self.skin_data)
        else:
            self._set_segments_from_body_parts(self.skin_data.segment_ids)

    def auto_assign_segments(self, method: str = "bones"):
        """Auto-assign segments using the given method."""
        if self.skin_data is None:
            return

        from creation_lib.skinning.partitions import assign_partitions_from_bones

        if method == "bones":
            body_part_ids = assign_partitions_from_bones(self.skin_data)
            self._set_segments_from_body_parts(body_part_ids)
            self.status_text = "Segments assigned from bone weights"
        elif method == "reference" and self.reference_skin is not None:
            self._set_segments_from_reference(self.reference_skin)
            self.status_text = "Segments assigned from reference body"
        else:
            self.status_text = "No reference body for segment assignment"
            return

        self.modified = True
        self._rebuild_segment_colors()
        _log.info(self.status_text)

    # ------------------------------------------------------------------
    # GPU mesh building
    # ------------------------------------------------------------------

    def _build_gpu_mesh(self):
        """Build a GPU mesh from current SkinData for viewport rendering.

        Sorts triangles by segment ID to create contiguous submesh ranges
        for hard-boundary segment rendering via separate draw calls.
        """
        if not self.skin_data or not self.skinned_renderer:
            return
        if self.skinned_renderer.program is None:
            return

        sd = self.skin_data
        ctx = self.ctx

        # Release previous mesh
        if self.skinned_mesh is not None:
            try:
                self.skinned_mesh.vao.release()
            except Exception:
                pass
            self.skinned_mesh = None

        # Interleave position + normal + uv
        vbo_data = np.column_stack([sd.vertices, sd.normals, sd.uvs]).astype(np.float32)
        vbo = ctx.buffer(vbo_data.tobytes())

        # Bone weights and indices
        weight_vbo = ctx.buffer(sd.weights[:, :4].astype(np.float32).tobytes())
        index_vbo = ctx.buffer(sd.bone_indices[:, :4].astype(np.int32).tobytes())

        # Segment color VBO (per-vertex RGB — used as fallback for all_weights mode)
        segment_colors = self._compute_segment_colors()
        segment_color_vbo = ctx.buffer(segment_colors.tobytes())

        # Vertex color VBO (per-vertex RGBA from NIF)
        if sd.vertex_colors is not None:
            vc_data = sd.vertex_colors.astype(np.float32)
        else:
            vc_data = np.ones((sd.num_vertices, 4), dtype=np.float32)
        vertex_color_vbo = ctx.buffer(vc_data.tobytes())

        # Mask VBO (per-vertex float, 0.0=editable, 1.0=locked)
        mask_data = (
            self.mask.astype(np.float32)
            if self.mask is not None
            else np.zeros(sd.num_vertices, dtype=np.float32)
        )
        mask_vbo = ctx.buffer(mask_data.tobytes())

        # Build sorted index buffer and segment submeshes
        sorted_indices, submeshes = self._build_segment_submeshes(sd)
        ibo = ctx.buffer(sorted_indices.tobytes())

        program = self.skinned_renderer.program

        vao = ctx.vertex_array(
            program,
            [
                (vbo, "3f 3f 2f", "in_position", "in_normal", "in_uv"),
                (weight_vbo, "4f", "in_bone_weights"),
                (index_vbo, "4i", "in_bone_indices"),
                (segment_color_vbo, "3f", "in_segment_color"),
                (vertex_color_vbo, "4f", "in_vertex_color"),
                (mask_vbo, "f", "in_mask"),
            ],
            index_buffer=ibo,
            index_element_size=4,
        )

        from creation_lib.nif.rendering.skinned_renderer import SkinnedMesh
        self.skinned_mesh = SkinnedMesh(
            vao=vao,
            index_count=len(sd.triangles) * 3,
            bone_names=list(sd.bone_names),
            segment_submeshes=submeshes,
        )

        # Preserve diffuse texture flag if texture was already loaded
        if self._diffuse_texture is not None:
            self.skinned_mesh.diffuse_texture_id = 1

        # Store VBO references for weight/segment/mask buffer updates
        self._weight_vbo = weight_vbo
        self._index_vbo = index_vbo
        self._segment_color_vbo = segment_color_vbo
        self._mask_vbo = mask_vbo

        _log.info("Built GPU mesh: %d verts, %d tris, %d submeshes",
                  sd.num_vertices, sd.num_triangles, len(submeshes))

        # Build wireframe edge buffer alongside the main mesh
        self._build_wireframe(sd)
        # Build segment boundary edges
        self._build_segment_edges()

    def _build_wireframe(self, sd):
        """Build a wireframe edge VAO from the mesh triangles.

        Creates a GL_LINES index buffer with unique edges, reusing the
        brush cursor shader program for rendering.
        """
        if self.brush_cursor is None or self.brush_cursor.program is None:
            return
        if self.ctx is None:
            return

        # Release previous wireframe
        if self._wireframe_vao is not None:
            try:
                self._wireframe_vao.release()
            except Exception:
                pass
            self._wireframe_vao = None
            self._wireframe_line_count = 0

        tris = sd.triangles
        # Build unique edge set (each edge stored as sorted pair)
        edge_set: set[tuple[int, int]] = set()
        for tri in tris:
            v0, v1, v2 = int(tri[0]), int(tri[1]), int(tri[2])
            edge_set.add((min(v0, v1), max(v0, v1)))
            edge_set.add((min(v1, v2), max(v1, v2)))
            edge_set.add((min(v0, v2), max(v0, v2)))

        if not edge_set:
            return

        edges = np.array(list(edge_set), dtype=np.uint32)  # (E, 2)
        edge_ibo = self.ctx.buffer(edges.tobytes())

        # Reuse the vertex position data (first 3 floats of each vertex)
        vbo = self.ctx.buffer(sd.vertices.astype(np.float32).tobytes())

        self._wireframe_vao = self.ctx.vertex_array(
            self.brush_cursor.program,
            [(vbo, "3f", "in_position")],
            index_buffer=edge_ibo,
            index_element_size=4,
        )
        self._wireframe_line_count = len(edge_set)

    def _build_segment_edges(self):
        """Build a VAO of edges that lie on segment boundaries.

        A boundary edge is shared by two triangles with different segment IDs.
        These edges are rendered as colored lines on top of any display mode,
        so you can see segment boundaries while viewing weight heatmaps.
        """
        if self.brush_cursor is None or self.brush_cursor.program is None:
            return
        if self.ctx is None or self.skin_data is None:
            return

        sd = self.skin_data

        # Release previous
        if self._segment_edge_vao is not None:
            try:
                self._segment_edge_vao.release()
            except Exception:
                pass
            self._segment_edge_vao = None
            self._segment_edge_count = 0

        if sd.segment_ids is None or len(sd.segment_ids) == 0:
            return

        # Build edge → triangle adjacency map
        # edge (sorted pair) → list of triangle indices
        edge_tris: dict[tuple[int, int], list[int]] = {}
        for ti in range(sd.num_triangles):
            tri = sd.triangles[ti]
            v0, v1, v2 = int(tri[0]), int(tri[1]), int(tri[2])
            for a, b in [(v0, v1), (v1, v2), (v0, v2)]:
                key = (min(a, b), max(a, b))
                edge_tris.setdefault(key, []).append(ti)

        # Find boundary edges: adjacent triangles with different segment IDs
        boundary_edges: list[tuple[int, int]] = []
        for edge, tri_list in edge_tris.items():
            if len(tri_list) >= 2:
                pid0 = int(sd.segment_ids[tri_list[0]])
                pid1 = int(sd.segment_ids[tri_list[1]])
                if pid0 != pid1:
                    boundary_edges.append(edge)
            elif len(tri_list) == 1:
                # Mesh boundary edge — also show these
                boundary_edges.append(edge)

        if not boundary_edges:
            return

        edges = np.array(boundary_edges, dtype=np.uint32)  # (E, 2)
        edge_ibo = self.ctx.buffer(edges.tobytes())
        vbo = self.ctx.buffer(sd.vertices.astype(np.float32).tobytes())

        self._segment_edge_vao = self.ctx.vertex_array(
            self.brush_cursor.program,
            [(vbo, "3f", "in_position")],
            index_buffer=edge_ibo,
            index_element_size=4,
        )
        self._segment_edge_count = len(boundary_edges)

    def _build_reference_gpu_mesh(self):
        """Build a GPU mesh for the reference body (transparent overlay)."""
        if not self.reference_skin or not self.skinned_renderer:
            return

        # Release previous reference mesh
        if self.reference_skinned_mesh is not None:
            try:
                self.reference_skinned_mesh.vao.release()
            except Exception:
                pass
            self.reference_skinned_mesh = None

        self.reference_skinned_mesh = (
            self.skinned_renderer.build_skinned_mesh_from_skin_data(
                self.reference_skin
            )
        )
        if self.reference_skinned_mesh:
            _log.info("Built reference GPU mesh: %d verts",
                      self.reference_skin.num_vertices)

    def _compute_segment_colors(self) -> np.ndarray:
        """Compute per-vertex segment colors from triangle segment IDs.

        Each vertex gets the color of the segment of the first triangle
        that references it. When a segment is selected, non-selected
        segments are dimmed. Returns (N, 3) float32 array.

        Used as fallback for "all_weights" mode (per-vertex blending).
        For "segments" mode, submesh rendering is preferred.
        """
        from ui.weight_painter.panels.segment_panel import get_segment_color

        sd = self.skin_data
        colors = np.zeros((sd.num_vertices, 3), dtype=np.float32)

        if sd.segment_ids is None or len(sd.segment_ids) == 0:
            return colors

        sel = self.selected_segment_id
        dim = 0.25  # Dimming factor for non-selected segments

        # For each triangle, assign its segment color to its vertices
        # (first-write wins — fast single pass)
        assigned = np.zeros(sd.num_vertices, dtype=bool)

        for tri_idx in range(sd.num_triangles):
            part_id = int(sd.segment_ids[tri_idx])
            color = get_segment_color(part_id)
            # Dim non-selected segments when one is selected
            if sel >= 0 and part_id != sel:
                color = tuple(c * dim for c in color)
            for vi in sd.triangles[tri_idx]:
                vi = int(vi)
                if not assigned[vi]:
                    colors[vi] = color
                    assigned[vi] = True

        return colors

    @staticmethod
    def _build_segment_submeshes(sd):
        """Sort triangles by segment ID and build contiguous submesh ranges.

        Returns:
            (sorted_indices, submeshes): sorted uint32 index array and list of
            SegmentSubmesh entries with start_index / tri_count / color.
        """
        from creation_lib.nif.rendering.skinned_renderer import SegmentSubmesh
        from ui.weight_painter.panels.segment_panel import get_segment_color

        n_tris = sd.num_triangles
        if n_tris == 0:
            return sd.triangles.flatten().astype(np.uint32), []

        # Sort triangle indices by segment ID (stable sort preserves order within groups)
        part_ids = sd.segment_ids
        sort_order = np.argsort(part_ids, kind="stable")

        # Build sorted index array
        sorted_tris = sd.triangles[sort_order]  # (M, 3) reordered
        sorted_indices = sorted_tris.flatten().astype(np.uint32)

        # Build submesh ranges by walking the sorted segment IDs
        sorted_parts = part_ids[sort_order]
        submeshes: list[SegmentSubmesh] = []

        i = 0
        while i < n_tris:
            pid = int(sorted_parts[i])
            start = i
            while i < n_tris and int(sorted_parts[i]) == pid:
                i += 1
            count = i - start
            color = get_segment_color(pid)
            submeshes.append(SegmentSubmesh(
                segment_id=pid,
                start_index=start * 3,  # Index into the index buffer (vertex indices)
                tri_count=count,
                color=color,
            ))

        return sorted_indices, submeshes

    def _rebuild_segment_colors(self):
        """Update only the segment color VBO after segment reassignment."""
        if self.skin_data is None:
            return
        if hasattr(self, '_segment_color_vbo') and self._segment_color_vbo is not None:
            try:
                new_colors = self._compute_segment_colors()
                self._segment_color_vbo.write(new_colors.tobytes())
            except Exception:
                # Size mismatch — full rebuild
                self._build_gpu_mesh()
        # Rebuild boundary edges whenever segments change
        self._build_segment_edges()

    def _frame_camera_on_mesh(self, skin_data):
        """Frame the orbit camera on a SkinData bounding box."""
        if self.camera is None or skin_data is None or skin_data.num_vertices == 0:
            return
        import glm
        verts = skin_data.vertices
        mins = verts.min(axis=0)
        maxs = verts.max(axis=0)
        center = glm.vec3(
            float((mins[0] + maxs[0]) * 0.5),
            float((mins[1] + maxs[1]) * 0.5),
            float((mins[2] + maxs[2]) * 0.5),
        )
        spread = float(max(maxs - mins))
        self.camera.frame_on_bounds(center, spread * 0.5)
        _log.info("Camera framed: center=(%.1f, %.1f, %.1f), spread=%.1f",
                  center.x, center.y, center.z, spread)

    def _rebuild_weight_buffer(self):
        """Update only the weight and bone index VBOs (avoids full mesh rebuild)."""
        if self.skin_data is None:
            return

        sd = self.skin_data
        if hasattr(self, '_weight_vbo') and self._weight_vbo is not None:
            try:
                self._weight_vbo.write(sd.weights[:, :4].astype(np.float32).tobytes())
                self._index_vbo.write(sd.bone_indices[:, :4].astype(np.int32).tobytes())
            except Exception:
                # VBO size mismatch — full rebuild
                self._build_gpu_mesh()

    # ------------------------------------------------------------------
    # Import / Export dialogs
    # ------------------------------------------------------------------

    def _open_import_dialog(self):
        """Open the import file dialog."""
        self._show_import_dialog = True

    def _open_export_dialog(self):
        """Open the export file dialog."""
        self._show_export_dialog = True
        if self.file_path:
            p = Path(self.file_path)
            self._export_path = str(p.with_stem(p.stem + "_skinned"))

    def gui(self):
        """Called every frame by the workspace host.

        In toolkit mode, panels are drawn via DockableWindow gui_functions
        bound by WeightPainterWorkspace._bind_dockable_windows(). This method
        is only called for non-docked updates (floating panels, modals, etc.).
        """
        if self._first_frame:
            self.setup()
            self._init_panels()
            self._first_frame = False
