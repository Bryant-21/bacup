"""ClothMakerApp — cloth viewer/editor workspace.

Cloth viewer/editor workspace: overlays, region/vertex painting brushes,
capsule authoring, undo/redo, preview simulation, and NIF export.
"""
from __future__ import annotations

import logging
from dataclasses import dataclass
from pathlib import Path

import numpy as np

from ui.mesh_workspace.base_app import MeshWorkspaceBase
from ui.mesh_workspace.undo import UndoStack
from .cloth_scene import ClothScene

_log = logging.getLogger("cloth_maker.app")

_CLOTH_BLOCK_TYPE = "BSClothExtraData"


def _inject_cloth_blob_into_nif(nif, new_blob: bytes, dest_nif: str | Path) -> None:
    """Inject `new_blob` into an in-memory NifFile and save to `dest_nif`.

    Sibling of `cli.cloth_commands.pack` — that command runs on raw NIF BYTES
    via `nif_core_native.cloth_pack_blob`, but the cloth_maker workspace already
    holds a Python-mutated `NifFile` object (cloth_skin_bind has applied shape
    edits and the export must preserve those edits in a single save pass).
    Mirror any BSClothExtraData-block structural change in both places.
    """
    cloth_block = next(
        (b for b in nif.blocks if b.type_name == _CLOTH_BLOCK_TYPE),
        None,
    )
    if cloth_block is None:
        root_id = int(nif._footer_roots[0]) if nif._footer_roots else 0
        root = nif.get_block(root_id)
        if root is None:
            raise ValueError(f"Root block {root_id} missing — cannot add BSClothExtraData")
        cloth_block = nif.add_block(_CLOTH_BLOCK_TYPE, {"Name": "CES"})
        existing = list(root.get_field("Extra Data List") or [])
        existing.append(cloth_block.block_id)
        root.set_field("Extra Data List", existing)
        root.set_field("Num Extra Data List", len(existing))
    cloth_block.set_field(
        "Binary Data",
        {"Data Size": len(new_blob), "Data": list(new_blob)},
    )
    nif.save(str(dest_nif))

# ---- Recent files (cloth maker instance) ------------------------------------
_RECENT_FILE_PATH = Path.home() / ".cloth_maker_recent.json"
_MAX_RECENT = 10


def _load_recent() -> list[str]:
    """Load recent files from disk."""
    import json
    try:
        if _RECENT_FILE_PATH.exists():
            data = json.loads(_RECENT_FILE_PATH.read_text(encoding="utf-8"))
            if isinstance(data, list):
                return [str(p) for p in data if isinstance(p, str)]
    except Exception:
        pass
    return []


def _save_recent(entries: list[str]) -> None:
    import json
    try:
        _RECENT_FILE_PATH.write_text(
            json.dumps(entries, indent=2), encoding="utf-8"
        )
    except Exception:
        pass


def add_recent(filepath: str) -> None:
    """Add a file to the cloth maker recent list."""
    filepath = str(Path(filepath).resolve())
    entries = _load_recent()
    entries = [p for p in entries if p != filepath]
    entries.insert(0, filepath)
    entries = entries[:_MAX_RECENT]
    _save_recent(entries)


def get_recent_list() -> list[str]:
    return _load_recent()


def remove_recent(filepath: str) -> None:
    filepath = str(Path(filepath).resolve())
    entries = _load_recent()
    entries = [p for p in entries if p != filepath]
    _save_recent(entries)


def clear_recent() -> None:
    _save_recent([])


# ---- Undo snapshot ----------------------------------------------------------

@dataclass
class ClothUndoSnapshot:
    """Lightweight snapshot of mutable cloth state for undo/redo."""
    particle_positions: np.ndarray | None
    particle_masses: np.ndarray | None
    particle_radii: np.ndarray | None
    capsule_data: list  # shallow copy of capsule list
    sphere_data: list   # shallow copy of sphere list
    region_triangles: set | None
    pin_vertices: set | None


@dataclass
class TrishapeInfo:
    """Metadata for a single BSTriShape in the loaded NIF."""
    name: str
    block_index: int
    num_vertices: int
    num_triangles: int
    # Range in the merged skin_data triangle array
    tri_start: int  # inclusive
    tri_end: int    # exclusive


class ClothMakerApp(MeshWorkspaceBase):
    """Cloth viewer/editor workspace — inherits shared mesh infrastructure."""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings=toolkit_settings)
        self.status_text = "Import a NIF to begin (with or without existing cloth data)"

        # Cloth-specific state
        self.scene = ClothScene()
        self.nif_file = None  # Loaded NIF for mesh rendering

        # Skin data — for brush raycasting, particle-to-vertex mapping, overlays
        self.skinned_mesh = None  # legacy — deformation methods check this
        self.skin_data = None  # SkinData — for brush raycasting
        self._bind_pose_matrices: list[np.ndarray] | None = None  # world-space bone mats
        # SceneRenderer deformation state: list of (vbo, cpu_array, n_verts) per mesh node
        # cpu_array shape: (n_verts, stride_floats) — first 8 cols are pos+normal+uv
        self._scene_deform_vbos: list | None = None
        self._scene_deform_original: list | None = None  # saved original cpu_arrays for reset

        # Overlays (created in setup)
        self.particle_overlay = None
        self.constraint_overlay = None
        self.capsule_overlay = None
        self.velocity_overlay = None
        self.region_overlay = None
        self.controls_overlay = None

        # Viewport geometry (updated each frame for overlay positioning)
        self._viewport_pos = None
        self._viewport_size = None

        # Brush cursor tracking (for viewport painting)
        self._cursor_hit_point: np.ndarray | None = None
        self._cursor_normal: np.ndarray | None = None
        self._painting: bool = False

        # Panels
        self.viewer_panel = None
        self.cloth_tree_panel = None
        self.param_panel = None
        self.preview_panel = None
        self.template_panel = None
        self.region_panel = None
        self.authoring_panel = None
        self.cloth_area_panel = None

        # Viewport capsule gizmo (created alongside panels — needs
        # authoring_panel for selection)
        self.capsule_gizmo = None

        # Trishape metadata (populated on import)
        self.trishape_infos: list[TrishapeInfo] = []

        # Block id of the BSTriShape the user wants to become cloth on export.
        # Set to the first shape in `trishape_infos` on import; changeable via
        # the region panel's Cloth Target Shape combo.
        self.cloth_target_shape_id: int | None = None

        # Mesh deformation state (for cloth preview)
        self._original_vertices: np.ndarray | None = None  # backup of skin_data.vertices
        self._deform_mesh_enabled: bool = True  # UI toggle

        # Import dialog
        self._show_import_dialog = False

        # Undo/redo
        self.undo_stack: UndoStack[ClothUndoSnapshot] = UndoStack(max_entries=30)

        # View options
        self.wireframe: bool = False
        self.backface_culling: bool = True

        # Diffuse texture is loaded by material_pipeline via nif_loader.

        # Shared scene settings (bg color, lighting, grid, nav)
        from ui.mesh_workspace.scene_settings import SceneSettings
        self.scene_settings = SceneSettings(grid_visible=False)

        # Render mode (Textured / Wireframe / Unlit)
        from creation_lib.renderer.render_modes import RenderModeManager
        self.render_mode_mgr: RenderModeManager | None = None
        self._scene_settings_dirty = False

    def setup(self):
        """Initialize GL context + cloth-specific renderers."""
        super().setup()

        from .overlays.particle_overlay import ParticleOverlay
        self.particle_overlay = ParticleOverlay(self.ctx)

        from .overlays.constraint_overlay import ConstraintOverlay
        self.constraint_overlay = ConstraintOverlay(self.ctx)

        from .overlays.capsule_overlay import CapsuleOverlay
        self.capsule_overlay = CapsuleOverlay(self.ctx)

        from .overlays.velocity_overlay import VelocityOverlay
        self.velocity_overlay = VelocityOverlay(self.ctx)

        from .overlays.region_overlay import RegionOverlay
        self.region_overlay = RegionOverlay(self.ctx)

        from .overlays.controls_overlay import ControlsOverlay
        self.controls_overlay = ControlsOverlay(self)

        # Render mode manager
        from creation_lib.renderer.render_modes import RenderModeManager
        self.render_mode_mgr = RenderModeManager(self.renderer)
        self.renderer.render_mode_mgr = self.render_mode_mgr

        # Apply scene settings now that renderer + camera exist
        self.scene_settings.apply_to(self)

        _log.info("Cloth maker setup complete")

    def _init_panels(self):
        if self._panels_initialized:
            return
        from .panels.viewer_panel import ViewerPanel
        from .panels.cloth_tree_panel import ClothTreePanel
        from .panels.param_panel import ParamPanel
        from .panels.preview_panel import PreviewPanel
        from .panels.template_panel import TemplatePanel
        from .panels.region_panel import RegionPanel
        from .panels.authoring_panel import AuthoringPanel
        from .panels.cloth_area_panel import ClothAreaPanel
        from .capsule_gizmo import CapsuleGizmo
        self.viewer_panel = ViewerPanel(self)
        self.cloth_tree_panel = ClothTreePanel(self)
        self.param_panel = ParamPanel(self)
        self.preview_panel = PreviewPanel(self)
        self.template_panel = TemplatePanel(self)
        self.region_panel = RegionPanel(self)
        self.authoring_panel = AuthoringPanel(self)
        self.cloth_area_panel = ClothAreaPanel(self)
        self.capsule_gizmo = CapsuleGizmo(self)
        self._panels_initialized = True

    def draw_workspace(self):
        """Draw the cloth maker UI — called each frame by the toolkit."""
        if self._first_frame:
            self.setup()
            self._first_frame = False
        if not self._panels_initialized:
            self._init_panels()

        from imgui_bundle import imgui

        # Left pane: cloth tree
        if self.cloth_tree_panel:
            self.cloth_tree_panel.draw()

        # Center: viewport with overlays
        self._draw_viewport()

        # Right pane: viewer panel
        if self.viewer_panel:
            self.viewer_panel.draw()

    def _draw_viewport(self):
        """Draw the 3D viewport with mesh + cloth overlays."""
        from imgui_bundle import imgui
        import glm
        import moderngl

        flags = (imgui.WindowFlags_.no_scrollbar.value
                 | imgui.WindowFlags_.no_scroll_with_mouse.value)
        visible, _ = imgui.begin("Viewport##cloth_maker", flags=flags)
        if not visible:
            imgui.end()
            return

        # Render mode combo
        from creation_lib.renderer.render_modes import RenderMode, LABELS as RM_LABELS
        _CM_MODES = [RenderMode.TEXTURED, RenderMode.WIREFRAME, RenderMode.UNLIT]
        _CM_LABELS = [RM_LABELS[m] for m in _CM_MODES]
        if self.render_mode_mgr is not None:
            cur_idx = (_CM_MODES.index(self.render_mode_mgr.mode)
                       if self.render_mode_mgr.mode in _CM_MODES else 0)
            imgui.set_next_item_width(110)
            c, new_idx = imgui.combo("##cm_render_mode", cur_idx, _CM_LABELS)
            if c:
                self.render_mode_mgr.set_mode(_CM_MODES[new_idx])

        viewport_pos = imgui.get_cursor_screen_pos()
        size = imgui.get_content_region_avail()

        # Store viewport geometry for controls overlay positioning
        self._viewport_pos = viewport_pos
        self._viewport_size = size

        renderer = self.renderer
        camera = self.camera

        if size.x <= 0 or size.y <= 0 or not renderer or not camera:
            imgui.end()
            return

        renderer.ensure_fbo(int(size.x), int(size.y))
        renderer.render(camera, self.lighting)

        # Render region highlight overlay (painted triangles)
        if (self.region_overlay and self.skin_data is not None
                and self.region_panel is not None and renderer.fbo):
            rp = self.region_panel
            has_region = len(rp._region_triangles) > 0
            has_pins = len(rp._pin_vertices) > 0
            if has_region or has_pins:
                aspect = size.x / max(size.y, 1)
                view = camera.get_view_matrix()
                proj = camera.get_projection_matrix(aspect)
                vp = proj * view
                vp_tuple_region = tuple(c for col in vp for c in col)

                renderer.fbo.use()
                sd = self.skin_data

                # Region triangles — blue overlay
                if has_region:
                    self.region_overlay.render(
                        vp_tuple_region, sd.vertices, sd.triangles,
                        rp._region_triangles,
                        color=(0.2, 0.5, 0.8, 0.35),
                    )

                # Pin vertices — show as orange triangles touching pinned verts
                if has_pins:
                    pin_arr = np.array(sorted(rp._pin_vertices), dtype=np.intp)
                    # Boolean mask: which triangles have at least one pinned vert
                    mask = (
                        np.isin(sd.triangles[:, 0], pin_arr)
                        | np.isin(sd.triangles[:, 1], pin_arr)
                        | np.isin(sd.triangles[:, 2], pin_arr)
                    )
                    pin_tris = set(np.where(mask)[0].tolist())
                    if pin_tris:
                        self.region_overlay.render(
                            vp_tuple_region, sd.vertices, sd.triangles,
                            pin_tris,
                            color=(0.9, 0.5, 0.1, 0.45),
                        )

                self.ctx.screen.use()

        # Render cloth overlays
        if self.scene.loaded and renderer.fbo:
            aspect = size.x / max(size.y, 1)
            view = camera.get_view_matrix()
            proj = camera.get_projection_matrix(aspect)
            vp = proj * view
            vp_tuple = tuple(c for col in vp for c in col)

            renderer.fbo.use()

            if self.scene.show_constraints and self.constraint_overlay:
                self.constraint_overlay.render(
                    vp_tuple, self.scene.constraint_links,
                    self.scene.particle_data,
                    self.scene.data_version,
                )

            if self.scene.show_capsules and self.capsule_overlay:
                self.capsule_overlay.render(
                    vp_tuple, self.scene.capsules, self.scene.spheres,
                    self.scene.data_version,
                )

            # Velocity arrows (only during sim preview)
            if (self.preview_panel and self.preview_panel._show_velocities
                    and self.preview_panel.solver is not None
                    and self.velocity_overlay):
                self.velocity_overlay.render(
                    vp_tuple,
                    self.preview_panel.solver.positions,
                    self.preview_panel.solver.velocities,
                    arrow_scale=self.preview_panel._velocity_scale,
                )

            if self.particle_overlay and self.scene.particle_data is not None:
                if self.scene.show_particles:
                    self.particle_overlay.render(
                        vp_tuple, self.scene.particle_data,
                        show_pins=self.scene.show_pins,
                    )
                elif self.scene.show_pins:
                    # Particles hidden but pins requested — do a pins-only
                    # pass so the user can still see where they anchored.
                    self.particle_overlay.render(
                        vp_tuple, self.scene.particle_data,
                        show_pins=True, pins_only=True,
                    )

            self.ctx.screen.use()

        # Render brush cursor ring on mesh surface
        if self._cursor_hit_point is not None and renderer.fbo:
            self._render_brush_cursor(renderer, camera, size)

        # Display FBO
        tex_id = renderer.get_fbo_texture_id()
        if tex_id:
            imgui.image(
                imgui.ImTextureRef(tex_id), size,
                uv0=imgui.ImVec2(0, 1), uv1=imgui.ImVec2(1, 0),
            )

        # Capsule drag/rotate gizmo — drawn on top of the FBO image so the
        # handles overlay the rendered viewport.
        gizmo_using = False
        if self.capsule_gizmo is not None:
            self.capsule_gizmo.draw(camera, viewport_pos, size)
            gizmo_using = self.capsule_gizmo.is_using()

        # Input handling (camera + brush)
        if imgui.is_item_hovered():
            io = imgui.get_io()
            if self.capsule_gizmo is not None:
                if self.capsule_gizmo.handle_hotkeys():
                    gizmo_using = True

            # H key = toggle controls overlay
            if (imgui.is_key_pressed(imgui.Key.h)
                    and not io.want_text_input
                    and self.controls_overlay is not None):
                self.controls_overlay.visible = not self.controls_overlay.visible

            # Render mode shortcuts: 1=Textured, 2=Wireframe, 5=Unlit
            if not io.want_text_input and self.render_mode_mgr is not None:
                if imgui.is_key_pressed(imgui.Key._1):
                    self.render_mode_mgr.set_mode(RenderMode.TEXTURED)
                elif imgui.is_key_pressed(imgui.Key._2):
                    self.render_mode_mgr.set_mode(RenderMode.WIREFRAME)
                elif imgui.is_key_pressed(imgui.Key._5):
                    self.render_mode_mgr.set_mode(RenderMode.UNLIT)

            # F key = frame camera on mesh
            if (imgui.is_key_pressed(imgui.Key.f)
                    and not io.want_text_input):
                if self.skin_data is not None:
                    self.frame_camera(self.skin_data.vertices)
                elif (self.scene.particle_data is not None
                      and self.scene.particle_data.positions is not None):
                    self.frame_camera(self.scene.particle_data.positions)

            # Alt or middle/right mouse = camera orbit/pan
            if io.key_alt or io.mouse_down[2] or io.mouse_down[1]:
                camera.handle_input(io)
                # Hide brush cursor during camera manipulation
                self._cursor_hit_point = None
                self._cursor_normal = None
            elif io.mouse_down[0] and self._is_brush_active():
                # Left click = brush paint
                self._handle_brush(io, viewport_pos, size)
            else:
                # Track hover for brush cursor
                self._update_hover(io, viewport_pos, size)
                if not io.mouse_down[0] and self._painting:
                    self._painting = False
                # Capsule click-pick — only on the rising edge of left click,
                # when the gizmo isn't already consuming the input and no
                # brush is active. ImGuizmo is_using takes priority so drag
                # operations aren't hijacked by picking.
                if (not gizmo_using
                        and not self._is_brush_active()
                        and self.capsule_gizmo is not None
                        and imgui.is_mouse_clicked(imgui.MouseButton_.left)):
                    self.capsule_gizmo.pick(
                        camera, viewport_pos, size, imgui.get_mouse_pos(),
                    )

            if io.mouse_wheel != 0:
                camera.handle_input(io)

        # Status bar
        imgui.set_cursor_pos_y(
            imgui.get_window_height()
            - imgui.get_text_line_height_with_spacing() - 4
        )
        imgui.text_colored(
            imgui.ImVec4(0.8, 0.8, 0.8, 1.0),
            self.status_text,
        )

        imgui.end()

    def import_nif(self, path: str) -> None:
        """Import a NIF file — works with or without existing cloth data."""
        p = Path(path)
        if not p.exists():
            remove_recent(path)
            self.status_text = f"File not found: {path}"
            return

        try:
            add_recent(path)

            # Clear undo stack and reset mesh deformation on new import
            self.undo_stack.clear()
            self.reset_mesh_deformation()
            self._original_vertices = None
            self._scene_deform_vbos = None

            self.scene.load_from_nif(path)

            # Load the NIF mesh for rendering
            from creation_lib.nif import NifFile
            self.nif_file = NifFile.load(path)

            # Load full scene for textured rendering via SceneRenderer / Fo4Backend
            if self.renderer:
                from creation_lib.renderer.nif_loader import load_nif_to_scene, _update_world_transforms
                import glm
                program = self.renderer.programs.get("default")
                scene_root, _ = load_nif_to_scene(
                    path, self.renderer.ctx, program,
                    texture_dirs=self.get_texture_dirs(game_id="fo4"),
                )
                self.renderer.scene_root = scene_root
                if scene_root:
                    _update_world_transforms(scene_root, glm.mat4(1.0))

            # Extract trishape metadata for the mesh selection panel
            self.trishape_infos = self._extract_trishape_infos(self.nif_file)

            # Default the cloth-export target to the first BSTriShape. User can
            # override via the region panel's Cloth Target Shape combo. Reset
            # to None if the NIF has no shapes at all.
            if self.trishape_infos:
                self.cloth_target_shape_id = self.trishape_infos[0].block_index
            else:
                self.cloth_target_shape_id = None

            # Try to build skinned mesh for the underlying outfit
            try:
                from creation_lib.skinning.reference_body import extract_skin_data_from_nif
                skin_data = extract_skin_data_from_nif(path)
                if skin_data:
                    self.skin_data = skin_data

                    # Transform skin-space vertices/normals to world space on CPU.
                    # This puts the mesh in the same coordinate system as Havok
                    # cloth particles and bone capsules (feet on grid, no z_offset).
                    self._bind_pose_matrices = self._compute_bind_pose_matrices_from_skin_data(
                        skin_data, self.scene.bone_world_transforms,
                    )
                    if self._bind_pose_matrices is not None:
                        world_verts = self._skin_vertices_to_world()
                        if world_verts is not None:
                            skin_data.vertices = world_verts
                            world_normals = self._skin_normals_to_world()
                            if world_normals is not None:
                                skin_data.normals = world_normals
                            # Overlay data should use world space (no z_offset).
                            # Bake world-space skin_data into the SceneRenderer VBOs
                            # so the textured mesh renders at world-space positions
                            # AND can be deformed per-frame without a root transform.
                            if self.scene.loaded:
                                self.scene._z_offset = 0.0
                                self.scene._extract_overlay_data()
                                if self.renderer and self.renderer.scene_root:
                                    from creation_lib.renderer.nif_loader import _update_world_transforms
                                    import glm
                                    # Reset root transform to identity — world-space
                                    # positions are now baked directly into the VBOs.
                                    _update_world_transforms(
                                        self.renderer.scene_root, glm.mat4(1.0)
                                    )
                                    self._scene_deform_vbos = (
                                        self._build_scene_deform_vbos(skin_data)
                                    )

                    self.frame_camera(skin_data.vertices)
                    if self.mesh_picker is not None:
                        self.mesh_picker.set_mesh(
                            skin_data.vertices, skin_data.triangles,
                        )
            except Exception as e:
                _log.debug("No skinned mesh available: %s", e)
                # Frame on particles instead
                if self.scene.particle_data is not None:
                    self.frame_camera(self.scene.particle_data.positions)

            # Build particle→vertex mapping for loaded cloth (nearest-vertex match)
            if self.scene.loaded and self.skin_data is not None:
                self.scene.build_particle_to_vertex_mapping(self.skin_data.vertices)

            # Auto-select all trishapes as cloth area on bare NIF import
            if not self.scene.loaded and self.region_panel and self.skin_data is not None:
                self.region_panel._region_triangles = set(range(len(self.skin_data.triangles)))

            if self.scene.loaded:
                cj = self.scene.cloth_json or {}
                scd = self.scene.active_sim_cloth
                particle_count = len(scd.get("particles", [])) if scd else 0
                constraint_count = len(self.scene.constraint_links)
                capsule_count = len(self.scene.capsules)
                self.status_text = (
                    f"{cj.get('name', '')}: {particle_count} particles, "
                    f"{constraint_count} links, {capsule_count} capsules"
                )
            else:
                self.status_text = (
                    f"Loaded {Path(path).name} (no cloth data — "
                    f"use Region or Template to add cloth)"
                )

        except Exception as e:
            self.status_text = f"Import error: {e}"
            _log.error("Failed to import %s: %s", path, e, exc_info=True)

    def export_nif(self, dest_path: str) -> None:
        """Export the authored cloth scene to a single self-contained NIF.

        Flow (no intermediate buttons, no source-NIF overwrite):
          1. Bind the user's target BSTriShape (``self.cloth_target_shape_id``)
             to cluster bones derived from the current sim particles. This
             mutates the in-memory ``self.nif_file`` — adds NiNode bones,
             writes BSSkin::Instance + BSSkin::BoneData, rewrites vertex
             bone weights/indices, and promotes the shape to
             BSSubIndexTriShape. Skipped if the target shape is already
             skinned.
          2. Serialize the in-memory cloth graph to HCL packfile bytes.
          3. Inject the blob into ``self.nif_file`` (creates BSClothExtraData
             on bare NIFs) and save to ``dest_path`` in a single pass.

        The source NIF on disk is never touched. The viewport keeps its
        pre-export scene graph — load ``dest_path`` in-game to see the
        bound result, or re-import it in cloth_maker.
        """
        if self.nif_file is None:
            self.status_text = "No NIF loaded"
            return
        if not self.scene.loaded:
            self.status_text = "No cloth scene — author a region first"
            return
        if self.cloth_target_shape_id is None:
            self.status_text = "No cloth target shape selected"
            return

        try:
            from creation_lib.nif.operations.cloth_skin_bind import cloth_skin_bind
            from creation_lib.nif.operations.skinning import (
                add_bone_node,
                convert_to_sub_index_tri_shape,
                make_shape_skinned,
                set_rigid_weights,
            )

            nif = self.nif_file
            shape_id = self.cloth_target_shape_id

            shape = nif.get_block(shape_id)
            if shape is None:
                self.status_text = f"Target shape {shape_id} missing from NIF"
                return

            # The cloth runtime walks every BSTriShape/BSSubIndexTriShape in
            # the NIF. Leaving a plain unskinned BSTriShape next to a cloth
            # BSSIT crashes rendering on load — D3D11 trips an access
            # violation in the shader input assembler when the shapes'
            # skinning states diverge. Rigid-promote every non-target
            # unskinned shape to a dummy root bone so the whole NIF renders
            # through the skinned pipeline.
            rigid_promoted: list[str] = []
            for block in list(nif.blocks):
                if not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
                    continue
                if block.block_id == shape_id:
                    continue
                skin = block.get_field("Skin")
                if skin is not None and int(skin) >= 0:
                    continue
                name = str(block.get_field("Name") or f"Shape{block.block_id}")
                bone_id = add_bone_node(
                    nif, f"{name}_Root",
                    translation=(0.0, 0.0, 0.0), parent_id=0,
                )
                make_shape_skinned(nif, block.block_id, bone_ids=[bone_id])
                set_rigid_weights(nif, block.block_id, bone_local_index=0)
                if block.type_name == "BSTriShape":
                    convert_to_sub_index_tri_shape(nif, block.block_id)
                rigid_promoted.append(name)

            # Only bind if the target shape isn't already skinned. Re-binding
            # a BSSIT would overwrite user skin data; skipping keeps existing
            # rigs intact when the user imports a pre-skinned cloth NIF.
            skin_ref = shape.get_field("Skin")
            already_skinned = skin_ref is not None and int(skin_ref) >= 0

            bind_info = "already skinned"
            if not already_skinned:
                sim_positions = self._scene_sim_positions()
                if not sim_positions:
                    self.status_text = "Cloth scene has no sim particles"
                    return
                bone_names = cloth_skin_bind(
                    nif, shape_id, sim_positions=sim_positions,
                )
                bind_info = f"bound to {len(bone_names)} cluster bones"

            _inject_cloth_blob_into_nif(nif, self.scene.blob, dest_path)

            shape_name = str(shape.get_field("Name") or f"Shape{shape_id}")
            rigid_note = (
                f" + rigid-promoted {len(rigid_promoted)} other shape(s)"
                if rigid_promoted else ""
            )
            self.status_text = (
                f"Exported {Path(dest_path).name}: "
                f"{shape_name} {bind_info}{rigid_note}"
            )
            _log.info("Exported cloth NIF to %s (%s %s%s)",
                      dest_path, shape_name, bind_info, rigid_note)
        except Exception as e:
            self.status_text = f"Export error: {e}"
            _log.error("Failed to export %s: %s", dest_path, e, exc_info=True)

    def _scene_sim_positions(self) -> list[tuple[float, float, float]]:
        """Pull raw sim-particle positions from the in-memory cloth scene JSON.

        Reads particle positions from cloth_json["sim_cloths"][idx]["particles"][i]["position"],
        which are already sourced from hclSimClothPose by cloth_inspect_full_json.
        """
        scd = self.scene.active_sim_cloth
        if scd is None:
            raise ValueError("No active sim cloth in scene")
        particles = scd.get("particles", [])
        if not particles:
            raise ValueError("Active sim cloth has no particles")
        return [
            (float(p["position"][0]), float(p["position"][1]), float(p["position"][2]))
            for p in particles
            if p.get("position") and len(p["position"]) >= 3
        ]

    # ------------------------------------------------------------------
    # Undo / Redo
    # ------------------------------------------------------------------

    def _take_snapshot(self) -> ClothUndoSnapshot:
        """Capture current mutable state for undo."""
        pd = self.scene.particle_data
        rp = self.region_panel
        return ClothUndoSnapshot(
            particle_positions=pd.positions.copy() if pd and pd.positions is not None else None,
            particle_masses=pd.masses.copy() if pd and pd.masses is not None else None,
            particle_radii=pd.radii.copy() if pd and pd.radii is not None else None,
            capsule_data=list(self.scene.capsules),
            sphere_data=list(self.scene.spheres),
            region_triangles=set(rp._region_triangles) if rp else None,
            pin_vertices=set(rp._pin_vertices) if rp else None,
        )

    def _restore_snapshot(self, snap: ClothUndoSnapshot) -> None:
        """Restore mutable state from a snapshot."""
        pd = self.scene.particle_data
        if pd is not None:
            if snap.particle_positions is not None:
                pd.positions[:] = snap.particle_positions
            if snap.particle_masses is not None:
                pd.masses[:] = snap.particle_masses
            if snap.particle_radii is not None:
                pd.radii[:] = snap.particle_radii
        self.scene.capsules = list(snap.capsule_data)
        self.scene.spheres = list(snap.sphere_data)
        self.scene.data_version += 1
        rp = self.region_panel
        if rp is not None:
            if snap.region_triangles is not None:
                rp._region_triangles = set(snap.region_triangles)
            if snap.pin_vertices is not None:
                rp._pin_vertices = set(snap.pin_vertices)

    def push_undo(self, label: str) -> None:
        """Save current state to undo stack before a mutation."""
        self.undo_stack.push(label, self._take_snapshot())

    def undo(self) -> None:
        # UndoStack.undo() moves the popped entry to _redo; overwrite it with
        # the current state below so redo restores what we're about to change.
        result = self.undo_stack.undo()
        if result is None:
            return
        label, snap = result
        # The redo stack now has the old snapshot. We need to replace it
        # with the *current* state so redo restores what we're about to overwrite.
        current = self._take_snapshot()
        # Replace the entry that was just moved to redo
        if self.undo_stack._redo:
            self.undo_stack._redo[-1] = (label, current)
        self._restore_snapshot(snap)
        self.status_text = f"Undo: {label}"

    def redo(self) -> None:
        result = self.undo_stack.redo()
        if result is None:
            return
        label, snap = result
        # Save current state so the undo entry has the right snapshot
        current = self._take_snapshot()
        if self.undo_stack._undo:
            self.undo_stack._undo[-1] = (label, current)
        self._restore_snapshot(snap)
        self.status_text = f"Redo: {label}"

    # ------------------------------------------------------------------
    # Mesh deformation (cloth preview)
    # ------------------------------------------------------------------

    def _build_scene_deform_vbos(self, skin_data) -> list | None:
        """Build per-VBO deformation state from the current scene_root.

        Walks scene_root depth-first, reads each mesh VBO into a CPU numpy
        array (stride-aware), writes world-space skin_data pos/normal/uv into
        the first 8 float columns (leaving tangents/bitangents intact), and
        writes the result back to the GPU.

        Returns list of (vbo, cpu_array, n_verts) tuples, or None on failure.
        cpu_array shape: (n_verts, stride_floats) — first 8 cols are pos+normal+uv.
        """
        if self.renderer is None or self.renderer.scene_root is None:
            return None
        if skin_data is None or len(skin_data.vertices) == 0:
            return None

        def _collect(node):
            out = []
            if node.mesh is not None:
                out.append(node)
            for c in node.children:
                out.extend(_collect(c))
            return out

        mesh_nodes = _collect(self.renderer.scene_root)
        if not mesh_nodes:
            return None

        refs = []
        vert_offset = 0
        for node in mesh_nodes:
            mesh = node.mesh
            if not mesh.vbo_format:
                continue
            stride_floats = sum(int(p[:-1]) for p in mesh.vbo_format.split())
            try:
                raw = mesh.vbo.read()
            except Exception:
                continue
            n_verts = len(raw) // (stride_floats * 4)
            if n_verts == 0:
                continue
            skin_end = vert_offset + n_verts
            if skin_end > len(skin_data.vertices):
                _log.debug("Deform VBO setup: vertex overflow at %s, skipping", node.name)
                continue

            cpu = np.frombuffer(raw, dtype=np.float32).reshape(n_verts, stride_floats).copy()
            # Bake world-space skin_data into first 8 float columns (pos+normal+uv)
            cpu[:, 0:3] = skin_data.vertices[vert_offset:skin_end]
            cpu[:, 3:6] = skin_data.normals[vert_offset:skin_end]
            cpu[:, 6:8] = skin_data.uvs[vert_offset:skin_end]
            mesh.vbo.write(cpu.tobytes())

            refs.append((mesh.vbo, cpu, n_verts))
            vert_offset += n_verts

        if not refs:
            return None
        if vert_offset != len(skin_data.vertices):
            _log.debug("Deform VBO: VBO total %d vs skin_data %d",
                       vert_offset, len(skin_data.vertices))
        return refs

    def _write_deform_vbo_data(self, vbo_data: "np.ndarray") -> None:
        """Write pos+normal+uv vertex data to SceneRenderer VBOs or SkinnedMesh VBO.

        vbo_data: (N, 8) float32 — columns 0:3=pos, 3:6=normal, 6:8=uv.
        SceneRenderer path: writes into the first 8 floats of each stride;
        tangents/bitangents columns are left unchanged.
        """
        if self._scene_deform_vbos is not None:
            vert_offset = 0
            for vbo, cpu, n_verts in self._scene_deform_vbos:
                slc = vbo_data[vert_offset:vert_offset + n_verts]
                cpu[:, 0:3] = slc[:, 0:3]
                cpu[:, 3:6] = slc[:, 3:6]
                cpu[:, 6:8] = slc[:, 6:8]
                vbo.write(cpu.tobytes())
                vert_offset += n_verts
        elif self.skinned_mesh is not None and self.skinned_mesh.vertex_vbo is not None:
            self.skinned_mesh.vertex_vbo.write(vbo_data.tobytes())

    def deform_mesh(self, solver_positions: np.ndarray) -> None:
        """Deform the skinned mesh vertices to match solver particle positions.

        Uses the vertex→particle mapping: for each mapped mesh vertex,
        applies the displacement of its nearest particle from rest position.
        This ensures ALL cloth-region vertices deform, not just one per particle.

        Args:
            solver_positions: (P, 3) float32 particle positions from the solver.
        """
        if not self._deform_mesh_enabled:
            return
        has_scene_vbos = self._scene_deform_vbos is not None
        has_skinned = (self.skinned_mesh is not None
                       and self.skinned_mesh.vertex_vbo is not None)
        if not has_scene_vbos and not has_skinned:
            return
        if self.skin_data is None:
            return

        v2p = self.scene.vertex_to_particle
        if v2p is None:
            # Fall back to particle_to_vertex for generated cloth
            mapping = self.scene.particle_to_vertex
            if mapping is None or len(mapping) == 0:
                return
            self._deform_mesh_p2v(solver_positions, mapping)
            return

        # Backup original vertices on first deformation
        if self._original_vertices is None:
            self._original_vertices = self.skin_data.vertices.copy()

        # Find mapped vertices (v2p >= 0) that map to dynamic (non-fixed) particles
        mapped_mask = v2p >= 0
        particle_indices = v2p[mapped_mask]
        valid_particles = particle_indices < len(solver_positions)

        # Filter out fixed particles — those are rigid (capsule-driven bones),
        # their vertices should not deform
        pd = self.scene.particle_data
        if pd is not None and pd.is_fixed is not None:
            is_dynamic = ~pd.is_fixed[particle_indices[valid_particles]]
        else:
            is_dynamic = np.ones(valid_particles.sum(), dtype=bool)

        mapped_verts = np.where(mapped_mask)[0][valid_particles][is_dynamic]
        source_particles = particle_indices[valid_particles][is_dynamic]
        self.skin_data.vertices[mapped_verts] = solver_positions[source_particles]

        # Rebuild VBO data: position(3f) + normal(3f) + uv(2f)
        vbo_data = np.column_stack([
            self.skin_data.vertices,
            self.skin_data.normals,
            self.skin_data.uvs,
        ]).astype(np.float32)
        self._write_deform_vbo_data(vbo_data)

    def _deform_mesh_p2v(self, solver_positions: np.ndarray,
                         mapping: np.ndarray) -> None:
        """Fallback deformation using particle→vertex mapping (generated cloth)."""
        if self._original_vertices is None:
            self._original_vertices = self.skin_data.vertices.copy()

        n_particles = min(len(mapping), len(solver_positions))
        vertex_indices = mapping[:n_particles]
        valid = vertex_indices < len(self.skin_data.vertices)
        self.skin_data.vertices[vertex_indices[valid]] = solver_positions[:n_particles][valid]

        vbo_data = np.column_stack([
            self.skin_data.vertices,
            self.skin_data.normals,
            self.skin_data.uvs,
        ]).astype(np.float32)
        self._write_deform_vbo_data(vbo_data)

    def reset_mesh_deformation(self) -> None:
        """Restore original mesh vertex positions after simulation."""
        if self._original_vertices is None:
            return
        has_scene_vbos = self._scene_deform_vbos is not None
        has_skinned = (self.skinned_mesh is not None
                       and self.skinned_mesh.vertex_vbo is not None)
        if self.skin_data is None or (not has_scene_vbos and not has_skinned):
            return

        self.skin_data.vertices[:] = self._original_vertices
        self._original_vertices = None

        # Rebuild VBO
        vbo_data = np.column_stack([
            self.skin_data.vertices,
            self.skin_data.normals,
            self.skin_data.uvs,
        ]).astype(np.float32)
        self._write_deform_vbo_data(vbo_data)

    # ------------------------------------------------------------------
    # Trishape extraction
    # ------------------------------------------------------------------

    @staticmethod
    def _extract_trishape_infos(nif_file) -> list[TrishapeInfo]:
        """Extract per-shape metadata from a loaded NifFile.

        Walks BSTriShape blocks in the same order as
        extract_skin_data_from_nif so that tri_start/tri_end ranges
        map directly into the merged skin_data.triangles array.
        """
        infos: list[TrishapeInfo] = []
        tri_offset = 0

        for block in nif_file.blocks:
            if not nif_file.schema.is_subtype_of(block.type_name, "BSTriShape"):
                continue

            vertex_data = block.get_field("Vertex Data") or []
            triangles = block.get_field("Triangles") or []

            if not vertex_data:
                continue

            n_verts = len(vertex_data)
            n_tris = len(triangles)
            name = block.get_field("Name") or block.type_name

            infos.append(TrishapeInfo(
                name=str(name),
                block_index=block.block_id,
                num_vertices=n_verts,
                num_triangles=n_tris,
                tri_start=tri_offset,
                tri_end=tri_offset + n_tris,
            ))
            tri_offset += n_tris

        return infos

    # ------------------------------------------------------------------
    # Bind-pose bone matrices (skin-space → world-space)
    # ------------------------------------------------------------------

    @staticmethod
    def _compute_bind_pose_matrices_from_skin_data(
        skin_data, bone_world_transforms: dict,
    ) -> list[np.ndarray] | None:
        """Compute bind-pose bone matrices from SkinData + NIF bone transforms.

        Each matrix transforms vertices from skin-space to NIF world-space
        (where the skeleton bind pose lives). This puts the mesh in the same
        coordinate system as Havok cloth particles and bone capsules.

        Formula: bone_matrix[i] = world_4x4(bone_i) @ inv_bind[i]
        """
        if (not skin_data.bone_names
                or not skin_data.inv_bind_transforms
                or not bone_world_transforms):
            return None

        matrices: list[np.ndarray] = []
        for i, bone_name in enumerate(skin_data.bone_names):
            if i >= len(skin_data.inv_bind_transforms):
                matrices.append(np.eye(4, dtype=np.float32))
                continue

            bt = bone_world_transforms.get(bone_name)
            if bt is None:
                matrices.append(np.eye(4, dtype=np.float32))
                continue

            trans, rot = bt  # (3,) float32, (3,3) float32
            world_mat = np.eye(4, dtype=np.float64)
            world_mat[:3, :3] = rot.astype(np.float64)
            world_mat[:3, 3] = trans.astype(np.float64)

            inv_bind = skin_data.inv_bind_transforms[i].astype(np.float64)
            mat = (world_mat @ inv_bind).astype(np.float32)
            matrices.append(mat)

        return matrices

    def _skin_vertices_to_world(self) -> np.ndarray | None:
        """Transform skin-space vertices to world-space on CPU.

        Uses bone weights + bind-pose matrices to compute world-space positions.
        """
        if self.skin_data is None or self._bind_pose_matrices is None:
            return None
        return self._apply_skinning(
            self.skin_data.vertices, self._bind_pose_matrices,
            self.skin_data.weights, self.skin_data.bone_indices,
            homogeneous=True,
        )

    def _skin_normals_to_world(self) -> np.ndarray | None:
        """Transform skin-space normals to world-space on CPU.

        Normals use the rotation part only (w=0 in homogeneous coords).
        """
        if self.skin_data is None or self._bind_pose_matrices is None:
            return None
        result = self._apply_skinning(
            self.skin_data.normals, self._bind_pose_matrices,
            self.skin_data.weights, self.skin_data.bone_indices,
            homogeneous=False,
        )
        if result is not None:
            # Renormalize after blending
            norms = np.linalg.norm(result, axis=1, keepdims=True)
            norms = np.maximum(norms, 1e-8)
            result /= norms
        return result

    @staticmethod
    def _apply_skinning(
        data: np.ndarray, matrices: list[np.ndarray],
        weights: np.ndarray, bone_indices: np.ndarray,
        homogeneous: bool = True,
    ) -> np.ndarray:
        """Apply weighted bone transforms to per-vertex data.

        Args:
            data: (V, 3) positions or normals.
            matrices: Per-bone 4x4 transform matrices.
            weights: (V, max_bones) blend weights.
            bone_indices: (V, max_bones) bone indices.
            homogeneous: True for positions (w=1), False for normals (w=0).
        """
        n_verts = len(data)
        w_val = 1.0 if homogeneous else 0.0
        pad = np.full((n_verts, 1), w_val, dtype=np.float32)
        data4 = np.column_stack([data, pad])  # (V, 4)

        mat_stack = np.array(matrices, dtype=np.float32)  # (B, 4, 4)

        result = np.zeros((n_verts, 3), dtype=np.float32)
        for j in range(weights.shape[1]):
            w = weights[:, j]
            bi = np.clip(bone_indices[:, j], 0, len(mat_stack) - 1)
            mats = mat_stack[bi]  # (V, 4, 4)
            transformed = np.einsum("vij,vj->vi", mats, data4)
            result += w[:, None] * transformed[:, :3]

        return result

    # ------------------------------------------------------------------
    # Brush cursor & input helpers
    # ------------------------------------------------------------------

    def _is_brush_active(self) -> bool:
        """Return True if any panel has a brush mode active."""
        # Authoring panel per-vertex brush
        if (self.authoring_panel is not None
                and self.authoring_panel._brush_active
                and self.scene.loaded):
            return True
        # Region panel brush (only when explicitly toggled on)
        if (self.region_panel is not None
                and self.region_panel._brush_active
                and self.skin_data is not None):
            return True
        return False

    def _render_brush_cursor(self, renderer, camera, size):
        """Render the 3D brush ring at the current cursor position."""
        brush_cursor = self.brush_cursor
        if brush_cursor is None:
            return
        if self._cursor_hit_point is None or self._cursor_normal is None:
            return

        renderer.fbo.use()

        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)
        vp = proj * view
        vp_tuple = tuple(c for col in vp for c in col)

        # Choose color based on active brush mode
        if self._painting:
            color = (1.0, 0.4, 0.2, 0.95)  # Orange while painting
        elif (self.region_panel is not None
              and self.region_panel._brush_mode == "pin"):
            color = (0.7, 0.5, 0.2, 0.8)  # Amber for pin brush
        elif (self.authoring_panel is not None
              and self.authoring_panel._brush_active):
            color = (0.4, 0.8, 1.0, 0.8)  # Cyan for authoring brush
        else:
            color = (1.0, 1.0, 1.0, 0.8)  # White default

        radius = self._get_active_brush_radius()

        brush_cursor.render(vp_tuple, self._cursor_hit_point,
                            self._cursor_normal, radius, color=color)

        self.ctx.screen.use()

    def _get_active_brush_radius(self) -> float:
        """Return the brush radius from whichever panel is active."""
        if (self.authoring_panel is not None
                and self.authoring_panel._brush_active):
            return self.authoring_panel._brush_radius
        if self.region_panel is not None:
            return self.region_panel._brush_radius
        return self.brush_state.radius

    def _update_hover(self, io, viewport_pos, size):
        """Track surface hit point + normal for brush cursor display."""
        if self.mesh_picker is None or self.skin_data is None:
            self._cursor_hit_point = None
            self._cursor_normal = None
            return

        mouse_x = io.mouse_pos.x - viewport_pos.x
        mouse_y = io.mouse_pos.y - viewport_pos.y

        if mouse_x < 0 or mouse_y < 0 or mouse_x > size.x or mouse_y > size.y:
            self._cursor_hit_point = None
            self._cursor_normal = None
            return

        camera = self.camera
        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)

        ray_origin, ray_dir = self.mesh_picker.unproject_ray(
            mouse_x, mouse_y, size.x, size.y, view, proj,
        )

        result = self.mesh_picker.pick_surface_point(ray_origin, ray_dir)
        if result is not None:
            hit_point, tri_idx = result
            self._cursor_hit_point = hit_point.astype(np.float32)
            # Compute face normal
            sd = self.skin_data
            tri = sd.triangles[tri_idx]
            v0 = sd.vertices[tri[0]]
            v1 = sd.vertices[tri[1]]
            v2 = sd.vertices[tri[2]]
            normal = np.cross(v1 - v0, v2 - v0)
            n_len = np.linalg.norm(normal)
            if n_len > 1e-6:
                self._cursor_normal = (normal / n_len).astype(np.float32)
            else:
                self._cursor_normal = np.array([0, 0, 1], dtype=np.float32)
        else:
            self._cursor_hit_point = None
            self._cursor_normal = None

    def _handle_brush(self, io, viewport_pos, size):
        """Handle brush painting on the mesh — forwards to active panel."""
        if self.mesh_picker is None or self.skin_data is None:
            return

        mouse_x = io.mouse_pos.x - viewport_pos.x
        mouse_y = io.mouse_pos.y - viewport_pos.y

        if mouse_x < 0 or mouse_y < 0 or mouse_x > size.x or mouse_y > size.y:
            return

        camera = self.camera
        aspect = size.x / max(size.y, 1)
        view = camera.get_view_matrix()
        proj = camera.get_projection_matrix(aspect)

        ray_origin, ray_dir = self.mesh_picker.unproject_ray(
            mouse_x, mouse_y, size.x, size.y, view, proj,
        )

        result = self.mesh_picker.pick_surface_point(ray_origin, ray_dir)
        if result is None:
            return

        hit_point, tri_idx = result

        # Update brush cursor position during painting
        self._cursor_hit_point = hit_point.astype(np.float32)
        sd = self.skin_data
        tri = sd.triangles[tri_idx]
        v0, v1, v2 = sd.vertices[tri[0]], sd.vertices[tri[1]], sd.vertices[tri[2]]
        normal = np.cross(v1 - v0, v2 - v0)
        n_len = np.linalg.norm(normal)
        self._cursor_normal = (
            (normal / n_len).astype(np.float32) if n_len > 1e-6
            else np.array([0, 0, 1], dtype=np.float32)
        )

        # Push undo on first frame of a new brush stroke
        if not self._painting:
            self.push_undo("Brush stroke")
        self._painting = True
        erasing = io.key_shift

        # Forward to authoring panel per-vertex brush if active
        if (self.authoring_panel is not None
                and self.authoring_panel._brush_active
                and self.scene.loaded):
            self.authoring_panel.handle_brush_stroke(
                hit_point.astype(np.float32), erasing=erasing,
            )
            return

        # Forward to region panel brush
        if self.region_panel is not None and self.skin_data is not None:
            self.region_panel.handle_brush_input(
                tri_idx, hit_point.astype(np.float32),
                sd.vertices, sd.triangles, erasing=erasing,
            )
