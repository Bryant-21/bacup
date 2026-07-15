"""Scope Aligner — core orchestrator.

Lightweight app that reuses rendering infrastructure from ui.editor
but strips out selection, gizmo, undo, MCP server, and animation.
"""
from __future__ import annotations

import logging
import sys
from pathlib import Path

import glm
import moderngl
from imgui_bundle import imgui, hello_imgui, immapp
from creation_lib.ui.theme.window_chrome import set_ini_folder, set_native_dark_title_bar
from app.paths import get_ini_dir
from creation_lib.ui.widgets.user_guide import (
    UserGuide,
    draw_generic_user_guide_window,
    draw_help_menu,
    draw_toolbar_help_button,
)

from creation_lib.renderer.scene_renderer import SceneRenderer
from ui.editor.nif_session import NifSession, NifRegistry, AttachmentNode
from creation_lib.renderer.nif_loader import load_nif_to_scene, _update_world_transforms
from creation_lib.renderer.overlays.connect_point import ConnectPointDisplay, compute_cp_world_transform
from creation_lib.renderer.lighting import LightingSetup
from .scope_camera import ScopeCamera
from .skeleton_loader import load_skeleton_bones
from .skeleton_display import SkeletonDisplay

_log = logging.getLogger("aligner.app")


def _setup_logging():
    """Configure logging for standalone mode."""
    if logging.getLogger().handlers:
        return
    fmt = logging.Formatter("%(levelname)s [%(name)s] %(message)s")
    sh = logging.StreamHandler(sys.stdout)
    sh.setLevel(logging.DEBUG)
    sh.setFormatter(fmt)
    root = logging.getLogger()
    root.setLevel(logging.DEBUG)
    root.addHandler(sh)


class ScopeAlignerApp:
    """Lightweight scope alignment tool."""

    def __init__(self, toolkit_settings=None):
        self.ctx: moderngl.Context | None = None
        self.renderer: SceneRenderer | None = None
        self.camera = ScopeCamera()
        self.lighting = LightingSetup()
        self.registry = NifRegistry()
        self.ba2_manager = None
        self.render_mode_mgr = None  # renderer checks this; None → TEXTURED
        self.status_text = "Load a weapon NIF to begin"
        self.active = True
        self._toolkit_settings = toolkit_settings
        self._first_frame = True
        self._panels_initialized = False

        # Skeleton data
        self._skeleton_data: dict | None = None
        self._skeleton_xml_data: dict | None = None  # full skeleton hierarchy
        self._world_positions: dict | None = None  # bone name → (x,y,z)
        self._world_rotations: dict | None = None  # bone name → 3x3 numpy rotation matrix

        # Skeleton overlay display
        self.skeleton_display = SkeletonDisplay()

        # Skinned body rendering (shared with bone_editor)
        self.skinned_renderer = None  # SkinnedRenderer instance
        self.skinned_meshes: list = []  # SkinnedMesh instances
        self.skeleton = None  # SkeletonManager for skinning math
        self._skeleton_nif_path: str | None = None

        # Connect point display (not used for visualization, but renderer checks for it)
        self.connect_points = ConnectPointDisplay(app=None)

        # Panels (created in _init_panels)
        self.setup_panel = None
        self.offset_panel = None
        self.output_panel = None
        self.viewport_panel = None
        self._show_user_guide = False

    def get_user_guide(self) -> UserGuide:
        from .panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide(
            "Scope Aligner User Guide",
            USER_GUIDE_MARKDOWN,
            "aligner_user_guide",
        )

    def toggle_user_guide(self) -> None:
        self._show_user_guide = not self._show_user_guide

    def draw_user_guide_window(self) -> None:
        self._show_user_guide = draw_generic_user_guide_window(
            self._show_user_guide,
            self.get_user_guide(),
        )

    def draw_menu(self) -> None:
        draw_help_menu(self)

    def draw_toolbar(self, icon_font=None) -> None:
        draw_toolbar_help_button(self, icon_font)

    def setup(self):
        """Called on first frame when GL context is available."""
        self.ctx = moderngl.get_context()
        self.renderer = SceneRenderer(self.ctx)
        self.renderer.init_shaders()
        self.renderer.init_grid()

        # Initialize skinned renderer for body mesh overlay
        from creation_lib.nif.rendering.skinned_renderer import SkinnedRenderer
        self.skinned_renderer = SkinnedRenderer(self.ctx)

        _log.info("Scope aligner GL context initialized: %s",
                  self.ctx.info["GL_RENDERER"])

        # Load skeleton
        try:
            self._skeleton_data = load_skeleton_bones()
            self.camera.set_from_skeleton(self._skeleton_data)
            _log.info("Skeleton loaded: camera=(%.1f,%.1f,%.1f)",
                      *self._skeleton_data["camera_pos"])
        except Exception:
            _log.exception("Failed to load skeleton — using defaults")

    def _init_panels(self):
        """Create panel instances."""
        if self._panels_initialized:
            return
        from .panels.setup_panel import SetupPanel
        from .panels.offset_panel import OffsetPanel
        from .panels.output_panel import OutputPanel
        from .panels.viewport import ViewportPanel

        self.setup_panel = SetupPanel(self)
        self.offset_panel = OffsetPanel(self)
        self.output_panel = OutputPanel(self)
        self.viewport_panel = ViewportPanel(self)

        # Update skeleton status in setup panel
        if self._skeleton_data:
            self.setup_panel._skeleton_loaded = True

        self._panels_initialized = True

    def _build_weapon_transform(self) -> glm.mat4:
        """Build a full weapon bone transform (rotation + translation) from skeleton data."""
        if not self._skeleton_data:
            return glm.mat4(1.0)

        wx, wy, wz = [float(v) for v in self._skeleton_data["weapon_pos"]]
        mat = glm.mat4(1.0)

        # Apply weapon bone world rotation (includes full parent chain)
        weapon_rot = self._skeleton_data.get("weapon_rot")
        if weapon_rot is not None:
            for i in range(3):
                for j in range(3):
                    mat[i][j] = float(weapon_rot[j][i])

        mat[3][0] = wx
        mat[3][1] = wy
        mat[3][2] = wz

        return mat

    def load_sighted_animation(self, hkx_path: str):
        """Load a sighted animation HKX and update camera with sighted bone positions."""
        from .skeleton_loader import load_sighted_pose_bones
        from .animation_loader import load_sighted_pose, _parse_skeleton_xml, _SKELETON_XML

        self._skeleton_data = load_sighted_pose_bones(hkx_path)
        self.camera.set_from_skeleton(self._skeleton_data)

        self._world_positions, self._world_rotations = load_sighted_pose(hkx_path)
        self._skeleton_xml_data = _parse_skeleton_xml(_SKELETON_XML)

        if self.ctx and self.renderer:
            cp_prog = self.renderer.programs.get("connect_point")
            if cp_prog and self._world_positions and self._skeleton_xml_data:
                self.skeleton_display.rebuild(
                    self._world_positions, self._skeleton_xml_data,
                    self.ctx, cp_prog,
                )

        # Reposition already-loaded weapon mesh with full bone transform (position + rotation)
        if self.registry.sessions and "Weapon" in self._world_positions:
            try:
                session = self.registry.get_session("main")
                session.scene_root.transform = self._build_weapon_transform()
                _update_world_transforms(session.scene_root, glm.mat4(1.0))

                center, radius = self._aggregate_bounds(session.scene_root)
                if radius > 0:
                    self.camera.frame_on_bounds(center, radius)
            except KeyError:
                pass

        _log.info("Sighted animation loaded: camera=(%.1f,%.1f,%.1f), weapon=(%.1f,%.1f,%.1f)",
                  *self._skeleton_data["camera_pos"], *self._skeleton_data["weapon_pos"])
        self.status_text = f"Animation: {Path(hkx_path).name}"

    def load_composite_body(self, skeleton_hkx: str, skeleton_nif: str | None,
                            body_nif_paths: list[str], game: str):
        """Load skeleton + body parts as skinned mesh overlay.

        Loads the HKX skeleton (for skinning math), extracts and merges
        body NIF meshes into a single SkinnedMesh.
        """
        from creation_lib.bone_edit.skeleton import SkeletonManager
        from creation_lib.skinning.reference_body import extract_skin_data_from_nif, _merge_skin_data
        from creation_lib.nif.rendering.skinned_renderer import attach_nif_bind_worlds

        self.skeleton = SkeletonManager.from_hkx(Path(skeleton_hkx))
        _log.info("Loaded skeleton: %d bones from %s",
                  self.skeleton.bone_count, skeleton_hkx)

        # Augment with NIF skeleton bones (_skin markers)
        if skeleton_nif and Path(skeleton_nif).exists():
            self._skeleton_nif_path = skeleton_nif
            added = self.skeleton.augment_from_nif(Path(skeleton_nif))
            _log.info("Augmented with NIF skeleton: +%d bones", added)

        parts = []
        for nif_path in body_nif_paths:
            try:
                skin = extract_skin_data_from_nif(nif_path)
                parts.append(skin)
            except Exception as e:
                _log.warning("Failed to extract skin from %s: %s",
                             Path(nif_path).name, e)

        if not parts:
            self.status_text = "No body parts could be loaded"
            return

        merged = _merge_skin_data(parts) if len(parts) > 1 else parts[0]

        mesh = self.skinned_renderer.build_skinned_mesh_from_skin_data(merged)
        if mesh is None:
            self.status_text = "Failed to build skinned mesh"
            return

        self.skinned_meshes = [mesh]

        # Attach NIF bind worlds for accurate skinning
        if self._skeleton_nif_path:
            attach_nif_bind_worlds(self._skeleton_nif_path, self.skinned_meshes)

        self.status_text = (
            f"Body: {len(parts)} parts, {merged.num_vertices}v"
        )
        _log.info("Composite body loaded: %d parts, %d verts, %d bones",
                  len(parts), merged.num_vertices, len(merged.bone_names))

    def _build_texture_dirs(self, nif_path=None) -> tuple[list[Path], list[Path], list[Path]]:
        """Build ordered texture search directories. Delegates to shared creation_lib."""
        from app.paths import get_app_root
        from creation_lib.textures.texture_dirs import build_texture_dirs
        return build_texture_dirs(
            self._toolkit_settings,
            nif_path=nif_path,
            mods_root=get_app_root() / "mods",
        )

    def _init_ba2_manager(self, user_archive_dirs: list[Path],
                          base_archive_dirs: list[Path]):
        """Initialize BA2 archive manager (lazy — archives opened on first miss)."""
        from creation_lib.textures.texture_dirs import create_ba2_manager
        self.ba2_manager = create_ba2_manager(
            user_archive_dirs, base_archive_dirs, existing=self.ba2_manager,
        )

    def load_weapon(self, filepath: str):
        """Load weapon NIF as main session."""
        try:
            texture_dirs, user_archive_dirs, base_archive_dirs = self._build_texture_dirs(filepath)
            self._init_ba2_manager(user_archive_dirs, base_archive_dirs)

            program = self.renderer.programs.get("fo4") if self.renderer else None
            if not program:
                _log.warning("FO4 shader not ready")
                return

            scene_root, nif = load_nif_to_scene(
                filepath, self.ctx, program, texture_dirs,
                self.ba2_manager, nif_id="main",
            )

            if self._skeleton_data and "weapon_pos" in self._skeleton_data:
                scene_root.transform = self._build_weapon_transform()
                _update_world_transforms(scene_root, glm.mat4(1.0))

            self.registry.clear()
            session = NifSession(
                nif_id="main", nif=nif, file_path=filepath,
                scene_root=scene_root, anim_manager=None,
            )
            self.registry.add_session(session)
            self.registry.active_id = "main"

            self.renderer.scene_root = scene_root
            self.renderer.clear_alt_vao_cache()

            center, radius = self._aggregate_bounds(scene_root)
            if radius > 0:
                self.camera.frame_on_bounds(center, radius)

            self.status_text = f"Loaded: {Path(filepath).name}"
            _log.info("Loaded weapon: %s", filepath)
        except Exception as e:
            _log.error("Failed to load weapon: %s", e)
            self.status_text = f"Error: {e}"

    def load_scope(self, filepath: str, connect_point: str):
        """Attach scope NIF at the specified connect point on the weapon."""
        try:
            parent_session = self.registry.get_session("main")
        except KeyError:
            _log.error("No weapon loaded")
            return

        nif_id = self.registry.next_child_id()

        try:
            texture_dirs, *_ = self._build_texture_dirs(filepath)
            program = self.renderer.programs.get("fo4")
            scene_root, nif = load_nif_to_scene(
                filepath, self.ctx, program, texture_dirs,
                ba2_mgr=self.ba2_manager, nif_id=nif_id,
            )
        except Exception:
            _log.exception("Failed to load scope NIF: %s", filepath)
            return

        child_cp_name = connect_point.replace("P-", "C-").replace("p-", "c-")
        child_cp_offset = self._find_child_connect_point(nif, child_cp_name)

        cp_world = self._get_cp_world_transform(parent_session, connect_point)

        attach_node = AttachmentNode(
            name=f"attach_{connect_point}",
            block_id=-1,
            parent_nif_id="main",
            child_nif_id=nif_id,
            connect_point_name=connect_point,
        )
        attach_node.transform = cp_world
        if child_cp_offset is not None:
            attach_node.transform = attach_node.transform * child_cp_offset
        attach_node.children.append(scene_root)

        parent_session.scene_root.children.append(attach_node)

        session = NifSession(
            nif_id=nif_id, nif=nif, file_path=filepath,
            scene_root=scene_root, anim_manager=None,
            parent_nif_id="main",
            attachment_point=connect_point,
            attachment_node=attach_node,
        )
        self.registry.add_session(session)

        _update_world_transforms(self.renderer.scene_root, glm.mat4(1.0))

        self.status_text = f"Attached: {Path(filepath).name} at {connect_point}"
        _log.info("Attached scope %s as %s at %s", filepath, nif_id, connect_point)

    def get_connect_point_names(self) -> list[str]:
        """Get list of parent connect point names from the weapon NIF."""
        if not self.registry.sessions:
            return []
        nif = self.registry.get_session("main").nif
        names = []
        for block in nif.blocks:
            if block.type_name == "BSConnectPoint::Parents":
                cps = block.get_field("Connect Points") or []
                for cp in cps:
                    if isinstance(cp, dict):
                        name = cp.get("Name", "")
                        if isinstance(name, list):
                            name = "".join(str(c) for c in name)
                        if name:
                            names.append(name)
        return names

    def _find_child_connect_point(self, nif, cp_name: str):
        """Find a child connect point by name. Returns offset transform or None."""
        target = cp_name.replace("C-", "").replace("c-", "")
        for block in nif.blocks:
            if block.type_name == "BSConnectPoint::Children":
                point_names = block.get_field("Point Name") or []
                if isinstance(point_names, str):
                    point_names = [point_names]
                for name in point_names:
                    if not isinstance(name, str):
                        name = str(name)
                    if name == cp_name or name == target or name == f"C-{target}" or name == f"c-{target}":
                        return glm.mat4(1.0)
        return None

    def _get_cp_world_transform(self, session, cp_name: str) -> glm.mat4:
        """Get the world transform of a named parent connect point."""
        nif = session.nif

        # Use standalone helper from connect_point_display
        world_pos, world_rot = compute_cp_world_transform(nif, cp_name)
        if world_pos is not None:
            import numpy as np
            mat = glm.mat4(1.0)
            for i in range(3):
                for j in range(3):
                    mat[i][j] = float(world_rot[j][i])
            mat[3][0] = float(world_pos[0])
            mat[3][1] = float(world_pos[1])
            mat[3][2] = float(world_pos[2])
            return mat

        _log.warning("CP '%s' not found, using identity", cp_name)
        return glm.mat4(1.0)

    def _aggregate_bounds(self, node) -> tuple:
        """Compute aggregate bounding sphere."""
        centers, radii = [], []
        def _collect(n):
            if n.bound_radius > 0:
                centers.append(n.bound_center)
                radii.append(n.bound_radius)
            for c in n.children:
                _collect(c)
        _collect(node)
        if not centers:
            return glm.vec3(0), 0
        avg = glm.vec3(0)
        for c in centers:
            avg += c
        avg /= len(centers)
        max_r = max(glm.length(c - avg) + r for c, r in zip(centers, radii))
        return avg, max_r

    def gui(self):
        """Called every frame by hello_imgui."""
        if self._first_frame:
            self.setup()
            self._init_panels()
            self._first_frame = False

        if not self.active:
            return

        self.viewport_panel.draw()
        self.setup_panel.draw()
        self.offset_panel.draw()
        self.output_panel.draw()
        self.draw_user_guide_window()


def main():
    """Standalone entry point."""
    _setup_logging()

    app = ScopeAlignerApp()

    runner_params = hello_imgui.RunnerParams()
    runner_params.app_window_params.window_title = "Scope Aligner"
    set_ini_folder(runner_params, "aligner", get_ini_dir())
    runner_params.app_window_params.window_geometry.size = (1600, 900)
    runner_params.callbacks.show_gui = app.gui
    runner_params.callbacks.show_menus = app.draw_menu
    runner_params.callbacks.post_init = set_native_dark_title_bar
    runner_params.imgui_window_params.default_imgui_window_type = (
        hello_imgui.DefaultImGuiWindowType.provide_full_screen_dock_space
    )
    runner_params.imgui_window_params.show_menu_bar = True

    # Simple docking layout
    params = hello_imgui.DockingParams()
    params.layout_condition = hello_imgui.DockingLayoutCondition.application_start

    params.docking_splits = [
        hello_imgui.DockingSplit(
            initial_dock_="MainDockSpace",
            new_dock_="LeftDock",
            direction_=imgui.Dir.left,
            ratio_=0.22,
        ),
        hello_imgui.DockingSplit(
            initial_dock_="LeftDock",
            new_dock_="LeftDockBottom",
            direction_=imgui.Dir.down,
            ratio_=0.55,
        ),
    ]

    _noop = lambda: None  # noqa: E731
    def _win(label: str, dock: str) -> hello_imgui.DockableWindow:
        w = hello_imgui.DockableWindow(label_=label, dock_space_name_=dock)
        w.call_begin_end = False
        w.gui_function = _noop
        return w

    params.dockable_windows = [
        _win("Viewport##aligner", "MainDockSpace"),
        _win("Setup##aligner", "LeftDock"),
        _win("Offsets##aligner", "LeftDockBottom"),
        _win("Output##aligner", "LeftDockBottom"),
    ]
    runner_params.docking_params = params

    toolbar_opts = hello_imgui.EdgeToolbarOptions()
    toolbar_opts.size_em = 2.5
    runner_params.callbacks.add_edge_toolbar(
        hello_imgui.EdgeToolbarType.top,
        app.draw_toolbar,
        toolbar_opts,
    )

    runner_params.imgui_window_params.show_status_bar = True
    runner_params.imgui_window_params.show_status_fps = True
    runner_params.imgui_window_params.tweaked_theme = (
        hello_imgui.ImGuiTweakedTheme(hello_imgui.ImGuiTheme_.darcula)
    )

    addons = immapp.AddOnsParams()
    addons.with_markdown = True
    immapp.run(runner_params, addons)
