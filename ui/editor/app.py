"""NIF Editor — main application.

Entry point: ``uv run python -m ui.editor [nif_path]``
"""

import json
import logging
import math
import os
import sys
import time
from pathlib import Path

from imgui_bundle import imgui, hello_imgui, immapp, imguizmo

from creation_lib.renderer.camera import OrbitCamera
from creation_lib.renderer.scene_renderer import SceneRenderer
from .docking_layout import get_nif_dockable_windows  # noqa: F401 — used by toolkit
from .selection import SelectionManager
from creation_lib.renderer.gizmo import GizmoManager, TRANSLATE, ROTATE, SCALE
from creation_lib.renderer.render_modes import RenderModeManager, RenderMode
from creation_lib.renderer.lighting import LightingSetup
from .animation import AnimationManager
from .animation_coordinator import AnimationCoordinator
from .nif_session import NifSession, NifRegistry
from .nif_watcher import NifFileWatcher
from .nif_file_types import NIF_LIKE_FILETYPES, is_nif_like_path
from .particles.model import build_particle_models
from .particles.runtime import ParticleRuntime
from .texture_watcher import TextureWatcher
from creation_lib.renderer.overlays.connect_point import ConnectPointDisplay
from .light_display import LightDisplay
from .undo import UndoManager, SnapshotAction
from creation_lib.nif.nif_file import NifFile

import glm


# Logging setup — only runs when imported as standalone app
_logging_initialized = False


def _setup_logging():
    global _logging_initialized
    if _logging_initialized:
        return
    _logging_initialized = True
    from ui.core.logging_utils import setup_logging

    setup_logging("toolkit")


_log = logging.getLogger("nif_editor.app")

import moderngl

SETTINGS_PATH = Path(__file__).parent / "editor_settings.json"


class NifEditorApp:
    def __init__(self, nif_path: str | None = None, toolkit_settings=None):
        self._init_nif_path = nif_path
        self.ctx: moderngl.Context | None = None
        self.renderer: SceneRenderer | None = None
        self.camera = OrbitCamera()
        self.lighting = LightingSetup()
        self.selection_mgr = SelectionManager()
        self.selection_mgr.on_selection_changed(self._on_selection_changed)
        self.gizmo = GizmoManager()
        self.nif_watcher = NifFileWatcher()
        self.texture_watcher = TextureWatcher()
        self.material_watcher = TextureWatcher(label="Material")
        self._material_watch_nif_ids: dict[str, set[str]] = {}
        self._nif_reload_pending: str | None = None
        self.connect_points = ConnectPointDisplay(self)
        self.connect_point_display = self.connect_points  # alias for scene_tree/toolbar
        self.light_display = LightDisplay(self)
        self.render_mode_mgr: RenderModeManager | None = None
        self.status_text = "Drop a NIF file or press O to open"
        self._first_frame = True
        self.show_command_palette = False
        # Gizmo undo tracking
        self._gizmo_was_active = False
        self._gizmo_snapshot = None
        self._gizmo_snapshot_nif_id = None
        self.active = True  # toolkit sets this to False when workspace is inactive
        self._toolkit_settings = toolkit_settings
        self.small_font = None  # Set by toolkit _load_fonts for overlay HUD

        # Multi-NIF registry — replaces self.nif/nif_file/nif_path/nif_root
        self.registry = NifRegistry()

        # BA2 archive manager for texture loading from archives
        self.ba2_manager = None  # initialized lazily in load_nif

        # Background NIF loading state
        self._loading: bool = False
        self._loading_future = None  # concurrent.futures.Future | None
        self._loading_nif_id: str = "main"
        self._loading_filename: str = ""
        self._loading_ba2_mgr = None  # BA2Manager snapshot held by background thread
        from concurrent.futures import ThreadPoolExecutor

        self._load_executor = ThreadPoolExecutor(
            max_workers=1, thread_name_prefix="nif_load"
        )

        # Background NIF attach state (mirrors _loading_* pattern)
        self._attaching: bool = False
        self._attach_future = None  # Future[PreparedAttachData] | None
        self._attach_filename: str = ""
        self._branch_paste_busy: bool = False
        self._branch_paste_pending = None
        self._branch_paste_queued_at: float = 0.0
        self._branch_paste_label: str = ""

        # Debug toggles (used by toolbar debug menu)
        self._toggle_lighting = True
        self._toggle_vertexColor = True
        self._toggle_diffuse = True
        self._toggle_normal = True
        self._toggle_spec = True
        self._toggle_envMap = True

        # TBR debug tuning sliders — default to "Fallout 4" preset
        self._dbg_envBoost = 2.0  # env map color multiplier
        self._dbg_metalF0 = 0.9  # max Fresnel F0 for metals
        self._dbg_diffuseBleed = 0.0  # min diffuse weight at full metalness
        self._dbg_exposure = 4.2  # tonemap exposure
        self._dbg_specBoost = 1.0  # specular highlight multiplier
        self._dbg_ambientBoost = 1.0  # ambient light multiplier
        self._lighting_tuning_idx = 1  # index into LIGHTING_PRESET_NAMES ("Fallout 4")

        # Undo — NIF-aware global stack
        self.undo_manager = UndoManager(
            registry=self.registry, max_history=self._load_undo_limit()
        )

        # Animation coordinator — cross-NIF animation sync
        self.anim_coordinator = AnimationCoordinator(self.registry)

        # Import options
        from creation_lib.renderer.nif_importer import ImportOptions

        self.import_options = ImportOptions()

        # Log handler — forwards to hello_imgui log widget
        self._log_handler = _BufferLogHandler()
        logging.getLogger().addHandler(self._log_handler)

        # Viewport geometry (updated each frame for overlay positioning)
        self._viewport_pos = None
        self._viewport_size = None
        self._viewport_label = "Viewport"

        self._viewport_ctx_node = None  # node targeted by last RMB click

        # Multi-NIF dialog state
        self._show_save_dialog = False
        self._show_detach_dialog = False
        self._show_about = False
        self._pending_detach: str | None = None
        self._save_dialog_checks: dict[str, bool] = {}

        # Panel instances (created after import to avoid circular deps)
        self._panels_initialized = False

        # Settings
        self._saved_panel_visibility = {}
        self._show_collision_saved = False  # default; overwritten by _load_settings
        self._show_lights_saved = True  # default; overwritten by _load_settings
        self._pending_render_toggles = {"shadows": True, "ssao": False, "show_vertices": False}
        self._load_settings()
        self.light_display._visible = self._show_lights_saved

    # -- Bridge properties --
    # These delegate to the active session in the registry so panels that
    # reference self.app.nif_file etc. keep working.

    @property
    def nif(self):
        """Active session's NifFile (bridge property)."""
        if self.registry.sessions:
            return self.registry.active_session.nif
        return None

    @property
    def nif_file(self):
        """Alias for nif (bridge property)."""
        return self.nif

    @property
    def nif_path(self):
        """Active session's file path (bridge property)."""
        if self.registry.sessions:
            return self.registry.active_session.file_path
        return self._init_nif_path

    @property
    def current_path(self):
        """Alias for nif_path (bridge property)."""
        return self.nif_path

    @property
    def nif_root(self):
        """Active session's scene root (bridge property)."""
        if self.registry.sessions:
            return self.registry.active_session.scene_root
        return None

    @property
    def hidden_block_ids(self):
        """Active session's hidden block IDs (bridge property)."""
        if self.registry.sessions:
            return self.registry.active_session.hidden_block_ids
        return set()

    @property
    def animation_mgr(self):
        """Active session's animation manager (bridge property)."""
        if self.registry.sessions:
            return self.registry.active_session.anim_manager
        return None

    def _create_animation_manager(self) -> AnimationManager:
        manager = AnimationManager()
        manager.set_sound_callback(self._play_animation_sound)
        return manager

    def _default_new_nif_game_id(self) -> str:
        if self.registry.sessions:
            profile = self.registry.active_session.game_profile
            if profile is not None:
                return profile.id
        settings = getattr(self, "_toolkit_settings", None)
        if settings is not None:
            get_active_game = getattr(settings, "get_active_game", None)
            if callable(get_active_game):
                return get_active_game() or "fo4"
        return "fo4"

    @staticmethod
    def _force_root_ninode(nif) -> None:
        root = nif.get_block(0) if nif.blocks else None
        if root is None:
            return
        root.type_name = "NiNode"
        if root.get_field("Name") is None:
            root.set_field("Name", "")
        if root.get_field("Children") is None:
            root.set_field("Children", [])
        if root.get_field("Num Children") is None:
            root.set_field("Num Children", 0)
        nif.header.block_type_names = ["NiNode"]
        nif.header.block_type_index = [0]
        nif.header.num_blocks = len(nif.blocks)
        nif.header.block_sizes = [0] * len(nif.blocks)

    def new_blank_nif(self, game_id: str | None = None):
        """Create an unsaved blank NIF editor session with a root NiNode."""
        if not self.renderer or not self.ctx:
            self.status_text = "Cannot create NIF: renderer is not ready"
            return None

        from creation_lib.core.game_profiles import get_profile
        from creation_lib.nif.nif_file import NifFile
        from creation_lib.renderer.nif_loader import rebuild_scene_from_nif

        game_id = game_id or self._default_new_nif_game_id()
        try:
            game_profile = get_profile(game_id)
        except KeyError:
            self.status_text = f"Unknown game: {game_id}"
            return None

        nif = NifFile.new(game_id)
        self._force_root_ninode(nif)

        self.renderer.ensure_game_backend(game_id)
        program = self.renderer.programs.get(game_id) or self.renderer.programs.get(
            "default"
        )
        texture_dirs, user_archive_dirs, base_archive_dirs = self._build_texture_dirs(
            game_profile=game_profile
        )
        self.ba2_manager = self._create_ba2_manager(user_archive_dirs, base_archive_dirs)
        scene_root = rebuild_scene_from_nif(
            nif,
            self.ctx,
            program,
            texture_dirs,
            self.ba2_manager,
            game_profile=game_profile,
        )
        anim_mgr = self._create_animation_manager()
        anim_mgr.scan(nif)
        particle_models, particle_runtime = self._create_particle_runtime(
            nif,
            "main",
            texture_dirs=texture_dirs,
            ba2_mgr=self.ba2_manager,
        )
        session = NifSession(
            nif_id="main",
            nif=nif,
            file_path="untitled.nif",
            scene_root=scene_root,
            anim_manager=anim_mgr,
            game_profile=game_profile,
            particle_models=particle_models,
            particle_runtime=particle_runtime,
            dirty=True,
        )

        self.nif_watcher.stop_watching()
        self._nif_reload_pending = None
        self.registry.clear()
        self.registry.add_session(session)
        self.registry.active_id = "main"
        self.renderer.scene_root = scene_root
        self.renderer.clear_alt_vao_cache()
        self.selection_mgr.clear()
        self.selection_mgr.register_bounds(scene_root)
        self.undo_manager.clear()
        self.light_display._needs_rebuild = True
        self._refresh_asset_watchers()
        self._nif_dirty = True
        self.status_text = "New NIF: untitled.nif"
        return session

    def queue_paste_branch_into_new(
        self,
        branch_blocks: list,
        source_block_id: int,
        game_id: str,
    ) -> None:
        self._branch_paste_pending = (branch_blocks, source_block_id, game_id)
        self._branch_paste_queued_at = time.perf_counter()
        self._branch_paste_busy = True
        self._branch_paste_label = f"Pasting branch {source_block_id} into new NIF..."
        self.status_text = self._branch_paste_label
        _log.info(
            "Queued Paste Branch Into New: source_block_id=%s blocks=%d game=%s",
            source_block_id,
            len(branch_blocks),
            game_id,
        )

    def _poll_branch_paste_into_new(self) -> None:
        pending = getattr(self, "_branch_paste_pending", None)
        if pending is None:
            return
        if time.perf_counter() - getattr(self, "_branch_paste_queued_at", 0.0) < 0.10:
            return

        branch_blocks, source_block_id, game_id = pending
        self._branch_paste_pending = None
        try:
            self.block_ops.execute_paste_branch_into_new(
                branch_blocks,
                source_block_id,
                game_id,
            )
        except Exception as exc:
            _log.exception("Paste Branch Into New failed")
            self.status_text = f"Paste Branch Into New failed: {exc}"
        finally:
            self._branch_paste_busy = False
            self._branch_paste_label = ""

    def _create_particle_runtime(self, nif, nif_id: str, texture_dirs=None, ba2_mgr=None):
        try:
            models = build_particle_models(nif, nif_id=nif_id)
            textures, greyscale_textures = self._load_particle_textures(
                models,
                texture_dirs,
                ba2_mgr,
            )
            runtime = (
                ParticleRuntime(
                    models,
                    texture_by_system=textures,
                    greyscale_texture_by_system=greyscale_textures,
                )
                if models
                else None
            )
            return models, runtime
        except Exception:
            _log.exception("Failed to create particle runtime for NIF %s", nif_id)
            return [], None

    def _load_particle_textures(self, models, texture_dirs=None, ba2_mgr=None):
        if not models or not texture_dirs or getattr(self, "ctx", None) is None:
            return {}, {}

        from creation_lib.renderer.material_pipeline import load_texture_path

        textures = {}
        greyscale_textures = {}
        for model in models:
            texture = self._load_particle_texture_path(
                load_texture_path,
                getattr(model, "source_texture", None),
                texture_dirs,
                ba2_mgr,
                model.system_block_id,
            )
            if texture is not None:
                textures[model.system_block_id] = texture

            greyscale_texture = self._load_particle_texture_path(
                load_texture_path,
                getattr(model, "greyscale_texture", None),
                texture_dirs,
                ba2_mgr,
                model.system_block_id,
            )
            if greyscale_texture is not None:
                greyscale_textures[model.system_block_id] = greyscale_texture
        return textures, greyscale_textures

    def _load_particle_texture_path(
        self,
        load_texture_path,
        texture_path,
        texture_dirs,
        ba2_mgr,
        system_block_id,
    ):
        if not texture_path:
            return None
        try:
            return load_texture_path(
                self.ctx,
                texture_path,
                texture_dirs,
                ba2_mgr,
            )
        except Exception:
            _log.exception(
                "Failed to load particle texture %s for block %s",
                texture_path,
                system_block_id,
            )
            return None

    def _play_animation_sound(self, event) -> None:
        from ui.editor.sound_events import play_sound_cue

        result = play_sound_cue(event.cue, self)
        if result.error:
            self.status_text = f"Sound unavailable: {event.cue} ({result.error})"
            _log.warning("Animation sound unavailable: %s", result)
        else:
            self.status_text = f"Playing sound: {event.cue}"

    def _load_undo_limit(self) -> int:
        """Read undo_limit from editor_settings.json, default 50."""
        try:
            with open(Path(__file__).parent / "editor_settings.json") as f:
                return json.load(f).get("undo_limit", 50)
        except Exception:
            return 50

    def _init_panels(self):
        """Late-initialize panel instances to avoid circular imports."""
        if self._panels_initialized:
            return
        from .panels.toolbar import ToolbarPanel
        from .panels.scene_tree import SceneTreePanel
        from .panels.properties import PropertiesPanel
        from .panels.texture_editor import TextureEditorPanel
        from .panels.validation import ValidationPanel
        from .panels.settings import SettingsPanel
        from .panels.skeleton_ops import SkeletonOpsPanel
        from .panels.batch_operations import BatchOperationsPanel
        from .panels.collision_dialog import CollisionDialog
        from .panels.particle_builder import ParticleBuilderPopup
        from .panels.controls_overlay import ControlsOverlay
        from .panels.command_palette import CommandPalette
        from .panels.animation_editor import AnimationEditorPanel
        from .panels.particle_systems import ParticleSystemsPanel
        from .panels.nif_browser import NifBrowserDialog
        from .panels.nif_file_browser import NifFileBrowserPanel

        self.toolbar = ToolbarPanel(self)
        self._nif_browser = NifBrowserDialog()
        self.scene_tree = SceneTreePanel(self)
        self.properties = PropertiesPanel(self)
        self.texture_editor = TextureEditorPanel(self)
        self.validation = ValidationPanel(self)
        self.settings_panel = SettingsPanel(self)
        self.skeleton_ops = SkeletonOpsPanel(self)
        self.ai_terminal = None  # not used in toolkit mode; AI Chat is the shared panel
        self.batch_operations = BatchOperationsPanel(self)
        self.collision_dialog = CollisionDialog(self)
        self.particle_builder_popup = ParticleBuilderPopup(self)
        self.animation_editor = AnimationEditorPanel(self)
        self.particle_systems = ParticleSystemsPanel(self)
        self.selection_mgr.on_selection_changed(
            self.particle_systems.on_selection_changed
        )
        self.nif_file_browser = NifFileBrowserPanel(self)
        self.controls_overlay = ControlsOverlay(self)
        self.command_palette = CommandPalette(self)

        from .block_ops import BlockOperations

        self.block_ops = BlockOperations(self)

        # Apply saved panel visibility (falls back to new defaults)
        _pv = self._saved_panel_visibility
        self.validation._visible = _pv.get("validation", False)
        self.skeleton_ops._visible = _pv.get("skeleton_ops", False)
        self.animation_editor._visible = _pv.get("animation_editor", True)

        self._panels_initialized = True
        if hasattr(self, "renderer") and self.renderer:
            self.renderer.settings_panel = self.settings_panel

    def setup(self):
        """Called on first frame when GL context exists."""
        self.ctx = moderngl.get_context()
        self.renderer = SceneRenderer(self.ctx)
        self.renderer.init_shaders()
        self.renderer.init_grid()
        # renderer.grid_visible will be synced from SettingsPanel once _init_panels() runs
        self.renderer._show_collision = self._show_collision_saved
        # Apply toggles that were loaded from settings before the renderer existed
        _pending = self._pending_render_toggles
        self.renderer.toggles.shadows = _pending["shadows"]
        self.renderer.toggles.ssao = _pending["ssao"]
        self.renderer.toggles.show_vertices = _pending["show_vertices"]
        self.render_mode_mgr = RenderModeManager(self.renderer)
        # Wire UI managers into the decoupled renderer
        self.renderer.render_mode_mgr = self.render_mode_mgr
        self.renderer.selection_mgr = self.selection_mgr
        self.renderer.connect_points = self.connect_points
        self.renderer.light_display = self.light_display
        _log.info("ModernGL context initialized: %s", self.ctx.info["GL_RENDERER"])
        if self._init_nif_path:
            self.load_nif(self._init_nif_path)

    def gui(self):
        """Legacy method — not used in toolkit mode.

        The toolkit draws panels via DockableWindow gui_functions
        (bound in nif_workspace._bind_dockable_windows) and the
        workspace draw() method for floating panels.
        """
        pass

        # Multi-NIF dialogs
        self._draw_save_dialog()
        self._draw_detach_dialog()
        self._draw_about_window()

        # Rebuild connect point display if needed
        self._rebuild_connect_points()

        # Rebuild light display if needed
        if (
            self.light_display._needs_rebuild
            and self.nif
            and self.renderer
            and self.ctx
        ):
            cp_prog = self.renderer.programs.get("connect_point")
            if cp_prog:
                self.light_display.rebuild(
                    self.nif, self.ctx, cp_prog, nif_id=self.registry.active_id
                )
                self.light_display._needs_rebuild = False
                # Push extracted light data to lighting system
                self.lighting.point_lights = self.light_display.point_lights
                # Register light nodes for gizmo picking
                self.selection_mgr.register_extra_nodes(self.light_display.light_nodes)

        # Check NIF file watcher
        reload_path = self.nif_watcher.check_reload()
        if reload_path:
            self.load_nif(reload_path)
        self._handle_material_reloads()
        self._handle_texture_reloads()

    def _draw_viewport(self):
        """Draw 3D viewport panel with FBO + ImGuizmo overlays."""
        flags = (
            imgui.WindowFlags_.no_scrollbar.value
            | imgui.WindowFlags_.no_scroll_with_mouse.value
        )
        imgui.begin(self._viewport_label, flags=flags)
        viewport_pos = imgui.get_cursor_screen_pos()
        size = imgui.get_content_region_avail()

        # Store viewport geometry for overlay positioning
        self._viewport_pos = viewport_pos
        self._viewport_size = size

        if size.x > 0 and size.y > 0 and self.renderer:
            self.renderer.ensure_fbo(size.x, size.y)

            try:
                self.renderer.active_nif_session = self.registry.active_session
            except KeyError:
                self.renderer.active_nif_session = None
            self.renderer.active_nif_registry = self.registry
            self.renderer.render(self.camera, self.lighting)

            tex_id = self.renderer.get_fbo_texture_id()
            if tex_id:
                # UV flip: OpenGL FBOs render bottom-up, imgui expects top-down
                imgui.image(
                    imgui.ImTextureRef(tex_id),
                    size,
                    uv0=imgui.ImVec2(0, 1),
                    uv1=imgui.ImVec2(1, 0),
                )

            # Capture hover/click state BEFORE gizmo.draw() which may consume clicks
            is_hovered = imgui.is_window_hovered()
            is_lmb_clicked = imgui.is_mouse_clicked(0)
            is_rmb_clicked = imgui.is_mouse_clicked(1)

            # ImGuizmo gizmo overlay
            new_transform = self.gizmo.draw(
                self.camera,
                viewport_pos,
                size,
                self.selection_mgr.selected,
            )
            # Track gizmo drag start/end for undo
            gizmo_active = self.gizmo.is_using()
            if gizmo_active and not self._gizmo_was_active:
                # Drag started — snapshot "before" state
                node = self.selection_mgr.selected
                if node and node.block_id >= 0:
                    nif_id = node.nif_id or self.registry.active_id
                    if nif_id in self.registry.sessions:
                        nif_obj = self.registry.get_session(nif_id).nif
                        block = nif_obj.get_block(node.block_id)
                        if block:
                            self._gizmo_snapshot = SnapshotAction(
                                _description=f"Move {node.name}",
                            )
                            self._gizmo_snapshot.capture_before(nif_obj)
                            self._gizmo_snapshot_nif_id = nif_id
            elif not gizmo_active and self._gizmo_was_active:
                # Drag ended — push undo with before/after
                if self._gizmo_snapshot is not None:
                    nif_id = self._gizmo_snapshot_nif_id
                    if nif_id in self.registry.sessions:
                        nif_obj = self.registry.get_session(nif_id).nif
                        self._gizmo_snapshot.capture_after(nif_obj)
                        self.undo_manager.push(nif_id, self._gizmo_snapshot)
                        _log.info(
                            "Gizmo undo pushed for %s [%s]",
                            self._gizmo_snapshot.description(),
                            nif_id,
                        )
                    self._gizmo_snapshot = None
                    self._gizmo_snapshot_nif_id = None
            self._gizmo_was_active = gizmo_active

            if new_transform is not None and self.selection_mgr.selected:
                node = self.selection_mgr.selected
                node.world_transform = new_transform
                node_nif = (
                    self.registry.get_session(node.nif_id).nif
                    if node.nif_id and node.nif_id in self.registry.sessions
                    else self.nif
                )
                if node.mesh is None and node_nif:
                    if hasattr(node, "_cp_block_id"):
                        self._apply_cp_gizmo(node, new_transform, node_nif)
                    else:
                        self._apply_light_gizmo(node, new_transform, node_nif)
                elif node.block_id >= 0 and node_nif:
                    # Write transform back to NIF block
                    self._apply_node_gizmo(node, new_transform, node_nif)

            # Check gizmo hover AFTER draw() so ImGuizmo has current-frame state
            gizmo_hovered = imguizmo.im_guizmo.is_over()

            # Connect point labels (imgui overlay)
            if self.connect_points.visible and self.renderer._current_vp:
                self.connect_points.draw_labels(
                    self.renderer._current_vp,
                    viewport_pos,
                    size,
                )

            # Light display labels (imgui overlay)
            if self.light_display.visible and self.renderer._current_vp:
                self.light_display.draw_labels(
                    self.renderer._current_vp,
                    viewport_pos,
                    size,
                )

            # Input handling (use cached hover/click state)
            if is_hovered:
                io = imgui.get_io()
                # Click to select (skip if gizmo is active or hovered)
                if is_lmb_clicked and not self.gizmo.is_using() and not gizmo_hovered:
                    if not io.key_alt:  # Alt+click is for light drag
                        self.selection_mgr.pick(
                            io.mouse_pos.x,
                            io.mouse_pos.y,
                            viewport_pos,
                            size,
                            self.camera,
                        )

                # RMB = viewport context menu (pick node + hide/show)
                if is_rmb_clicked and not self.gizmo.is_using() and not gizmo_hovered:
                    self.selection_mgr.pick(
                        io.mouse_pos.x,
                        io.mouse_pos.y,
                        viewport_pos,
                        size,
                        self.camera,
                    )
                    self._viewport_ctx_node = self.selection_mgr.selected
                    imgui.open_popup("##viewport_ctx")

                # Alt+LMB = light drag
                if io.key_alt and imgui.is_mouse_down(0) and not self.gizmo.is_using():
                    dx = io.mouse_delta.x / max(size.x, 1)
                    dy = io.mouse_delta.y / max(size.y, 1)
                    self.lighting.update_key_light_drag(dx, dy)
                elif not self.gizmo.is_using():
                    self.camera.handle_input(io)

        # Viewport context menu (RMB on mesh)
        if imgui.begin_popup("##viewport_ctx"):
            ctx_node = self._viewport_ctx_node
            if ctx_node is not None:
                is_hidden = ctx_node.block_id in self.hidden_block_ids
                if imgui.menu_item("Show" if is_hidden else "Hide", "", False)[0]:
                    self.toggle_node_visibility(ctx_node.block_id)
                imgui.separator()
                imgui.text_disabled(f"[{ctx_node.block_id}] {ctx_node.name}")
            else:
                imgui.text_disabled("(no mesh under cursor)")
            if self.hidden_block_ids:
                imgui.separator()
                if imgui.menu_item("Unhide All", "", False)[0]:
                    self.hidden_block_ids.clear()
                    self._sync_visibility(self.nif_root)
            imgui.end_popup()

        # Status text at bottom
        imgui.set_cursor_pos_y(
            imgui.get_window_height() - imgui.get_text_line_height_with_spacing() - 4
        )
        imgui.text_colored(imgui.ImVec4(0.8, 0.8, 0.8, 1.0), self.status_text)

        # Loading spinner — shown while a NIF is being loaded on background thread.
        # Uses the already-computed viewport_pos and size from the top of this method.
        branch_paste_busy = getattr(self, "_branch_paste_busy", False)
        if (
            self._loading
            or self._attaching
            or branch_paste_busy
        ) and size.x > 0 and size.y > 0:
            draw_list = imgui.get_foreground_draw_list()

            # Semi-transparent dark overlay covering the content area
            draw_list.add_rect_filled(
                viewport_pos,
                imgui.ImVec2(viewport_pos.x + size.x, viewport_pos.y + size.y),
                imgui.get_color_u32((0.0, 0.0, 0.0, 0.55)),
            )

            cx = viewport_pos.x + size.x * 0.5
            cy = viewport_pos.y + size.y * 0.5
            spin_radius = 20.0
            t = imgui.get_time()
            angle_start = math.fmod(t * 3.0, math.tau)
            arc_span = math.pi * 1.3

            # Dim background ring — add_circle takes ImVec2 center in imgui_bundle
            draw_list.add_circle(
                imgui.ImVec2(cx, cy),
                spin_radius,
                imgui.get_color_u32((1.0, 1.0, 1.0, 0.2)),
                32,
                3.0,
            )
            # Rotating bright arc
            draw_list.path_arc_to(
                imgui.ImVec2(cx, cy),
                spin_radius,
                angle_start,
                angle_start + arc_span,
                32,
            )
            draw_list.path_stroke(
                imgui.get_color_u32((1.0, 1.0, 1.0, 1.0)),
                False,
                3.0,
            )

            # Centered filename label
            label = (
                f"Attaching {self._attach_filename}…"
                if self._attaching
                else self._branch_paste_label
                if branch_paste_busy
                else f"Loading {self._loading_filename}…"
            )
            tw = imgui.calc_text_size(label).x
            draw_list.add_text(
                imgui.ImVec2(cx - tw * 0.5, cy + spin_radius + 10),
                imgui.get_color_u32((1.0, 1.0, 1.0, 1.0)),
                label,
            )

        imgui.end()

    def _drop_point_in_viewport(self, x: float | None, y: float | None) -> bool:
        if x is None or y is None:
            return True
        pos = self._viewport_pos
        size = self._viewport_size
        if pos is None or size is None:
            return False
        return pos.x <= x <= pos.x + size.x and pos.y <= y <= pos.y + size.y

    def handle_file_drop(
        self,
        paths: list[str],
        *,
        x: float | None = None,
        y: float | None = None,
    ) -> bool:
        """Open the first dropped NIF-like file when the drop lands in the viewport."""
        if not self._drop_point_in_viewport(x, y):
            return False

        for path in paths:
            candidate = Path(path).expanduser().resolve(strict=False)
            if is_nif_like_path(candidate):
                self.load_nif(str(candidate))
                return True

        self.status_text = "Drop a .nif, .bto, or .btr file to open it"
        return False

    def show_status(self):
        """Called by hello_imgui to render custom content in the status bar."""
        imgui.text(self.status_text)

    # Profile IDs match settings keys directly.
    _PROFILE_TO_SETTINGS_KEY: dict[str, str] = {}

    def _build_texture_dirs(
        self, nif_path=None, game_profile=None
    ) -> tuple[list, list, list]:
        """Build directories to search for textures/materials.

        Delegates to ``creation_lib.textures.texture_dirs.build_texture_dirs`` for the
        standard lookup order.  Falls back to legacy editor_settings.json
        when running outside the toolkit (no ToolkitSettings).
        """
        from creation_lib.textures.texture_dirs import build_texture_dirs

        game_id = game_profile.id if game_profile else "fo4"

        if self._toolkit_settings is not None:
            from app.paths import get_app_root
            return build_texture_dirs(
                self._toolkit_settings,
                game_id=game_id,
                nif_path=nif_path,
                mods_root=get_app_root() / "mods",
            )

        # Legacy fallback: standalone editor without toolkit settings
        texture_dirs: list[Path] = []
        user_archive_dirs: list[Path] = []

        def _add_with_data(target: list, p: Path):
            target.append(p)
            data_sub = p / "Data"
            if data_sub.is_dir():
                target.append(data_sub)

        for p in self._load_raw_editor_settings().get("extra_paths", []):
            _add_with_data(texture_dirs, Path(p))
            _add_with_data(user_archive_dirs, Path(p))

        if nif_path:
            nif_dir = Path(nif_path).parent
            texture_dirs.append(nif_dir)
            p = nif_dir
            for _ in range(8):
                p = p.parent
                if p == p.parent:
                    break
                if p.name.lower() == "data" and (p / "Meshes").is_dir():
                    if p not in texture_dirs:
                        texture_dirs.append(p)
                    break

        return texture_dirs, user_archive_dirs, []

    def _init_ba2_manager(
        self, user_archive_dirs: list[Path], base_archive_dirs: list[Path]
    ):
        """Initialize or re-initialize the BA2 archive manager."""
        from creation_lib.textures.texture_dirs import create_ba2_manager

        self.ba2_manager = create_ba2_manager(
            user_archive_dirs,
            base_archive_dirs,
            existing=self.ba2_manager,
        )

    def _create_ba2_manager(self, user_archive_dirs: list, base_archive_dirs: list):
        """Create a new BA2Manager without closing any existing one.

        Use this instead of _init_ba2_manager() when a background thread may still
        be holding a reference to self.ba2_manager.
        """
        from creation_lib.textures.texture_dirs import create_ba2_manager

        return create_ba2_manager(user_archive_dirs, base_archive_dirs)

    def _detect_game_profile(self, filepath):
        """Quick-load a NIF header to detect the game profile.

        Uses header-only parsing — reads ~200 bytes instead of the full file.
        Returns a GameProfile or None (falls back to FO4 in callers).
        """
        try:
            from creation_lib.nif.nif_file import NifFile
            from creation_lib.core.game_profiles import detect_game, FO4_PROFILE

            nif = NifFile.load_header(filepath)
            profile = nif.detected_game
            if profile is None and hasattr(nif, "header"):
                profile = detect_game(nif.header.bs_version)
            if profile is None:
                _log.info("Unknown BS version in %s, defaulting to FO4", filepath)
                profile = FO4_PROFILE
            else:
                _log.info(
                    "Detected game for %s: %s",
                    Path(filepath).name,
                    profile.display_name,
                )
            return profile
        except Exception as e:
            _log.warning(
                "Game detection failed for %s: %s — defaulting to FO4", filepath, e
            )
            try:
                from creation_lib.core.game_profiles import FO4_PROFILE

                return FO4_PROFILE
            except ImportError:
                return None

    def _apply_game_lighting_preset(self, game_profile) -> None:
        """Switch the lighting tuning preset to match the detected game.

        No-op if game has no mapped preset or if the user is on 'Custom'.
        """
        from .panels.toolbar_tbr import (
            GAME_ID_TO_PRESET,
            LIGHTING_PRESET_NAMES,
            apply_lighting_preset,
        )

        if game_profile is None:
            return
        preset_name = GAME_ID_TO_PRESET.get(game_profile.id)
        if preset_name is None:
            return
        # Don't clobber a user-set Custom preset
        custom_idx = len(LIGHTING_PRESET_NAMES) - 1
        if getattr(self, "_lighting_tuning_idx", 0) == custom_idx:
            return
        if preset_name in LIGHTING_PRESET_NAMES:
            self._lighting_tuning_idx = LIGHTING_PRESET_NAMES.index(preset_name)
            apply_lighting_preset(self, preset_name)
            _log.debug(
                "Auto-switched lighting tuning to '%s' for %s",
                preset_name,
                game_profile.display_name,
            )

    def load_nif(self, filepath, nif_id: str = "main"):
        """Start loading a NIF file on a background thread.

        Returns immediately. Call _poll_loading() each frame to detect completion
        and finalize the session on the UI thread.
        """
        from creation_lib.renderer.nif_loader import prepare_nif_data
        _t0_load = _t_prev = time.perf_counter()

        # Discard any in-flight future — that thread runs to completion but we
        # won't use its result. Do NOT close its ba2_mgr while it may still read.
        self._loading_future = None

        if not Path(filepath).exists():
            from .recent_files import remove as _remove_recent

            _remove_recent(filepath)
            self.status_text = (
                f"File not found (removed from recents): {Path(filepath).name}"
            )
            _log.warning("load_nif: file not found, removed from recents: %s", filepath)
            return

        try:
            # Detect game from NIF header before building texture dirs
            game_profile = self._detect_game_profile(filepath)
            _t = time.perf_counter(); _log.debug("[nif-timing] detect_game: %.1f ms", (_t - _t_prev) * 1000); _t_prev = _t
            self._apply_game_lighting_preset(game_profile)

            texture_dirs, user_archive_dirs, base_archive_dirs = (
                self._build_texture_dirs(filepath, game_profile=game_profile)
            )
            _t = time.perf_counter(); _log.debug("[nif-timing] build_tex_dirs: %.1f ms", (_t - _t_prev) * 1000); _t_prev = _t
            new_ba2_mgr = self._create_ba2_manager(user_archive_dirs, base_archive_dirs)
            _t = time.perf_counter(); _log.debug("[nif-timing] create_ba2_mgr: %.1f ms", (_t - _t_prev) * 1000); _t_prev = _t
            self._loading_ba2_mgr = new_ba2_mgr
            self.ba2_manager = new_ba2_mgr

            # Update default environment cubemap on UI thread (GL call)
            game_id_env = game_profile.id if game_profile else "fo4"
            if self.renderer:
                self.renderer.ensure_game_backend(game_id_env)
                self.renderer.update_default_env(
                    texture_dirs, new_ba2_mgr, game_id=game_id_env
                )
                _t = time.perf_counter(); _log.debug("[nif-timing] update_default_env: %.1f ms", (_t - _t_prev) * 1000); _t_prev = _t

            game_id = game_profile.id if game_profile else "fo4"
            program = (
                (
                    self.renderer.programs.get(game_id)
                    or self.renderer.programs.get("default")
                )
                if self.renderer
                else None
            )
            if not program:
                _log.warning("Default shader not compiled yet, cannot load NIF")
                return

            self._loading_filename = Path(filepath).name
            self._loading_nif_id = nif_id
            self._loading = True
            self.status_text = f"Loading {self._loading_filename}…"

            ba2_snapshot = new_ba2_mgr
            _log.debug("[nif-timing] ui_setup_total: %.1f ms", (time.perf_counter() - _t0_load) * 1000)
            self._loading_future = self._load_executor.submit(
                prepare_nif_data,
                filepath,
                texture_dirs,
                ba2_snapshot,
                nif_id,
                game_profile=game_profile,
            )
            _log.debug(
                "load_nif: submitted background task for %s (game=%s)",
                filepath,
                game_profile.display_name if game_profile else "unknown",
            )
        except Exception as e:
            self._loading = False
            _log.error("Failed to start NIF load: %s", e, exc_info=True)
            self.status_text = f"Error: {e}"

    def _poll_loading(self):
        """Check if a background NIF load has completed; if so, finalize on UI thread.

        Called every frame from gui(). Does nothing if no load is in progress or the
        future is not yet done.
        """
        if not self._loading:
            return
        if self._loading_future is None:
            _log.debug(
                "_poll_loading: _loading=True but future is None — clearing stuck state"
            )
            self._loading = False
            return
        if not self._loading_future.done():
            return

        _log.debug("_poll_loading: future done, starting GPU upload")
        try:
            prepared = self._loading_future.result()
        except Exception as e:
            self._loading = False
            self._loading_future = None
            _log.error("Failed to load NIF (background phase): %s", e, exc_info=True)
            self.status_text = f"Error: {e}"
            return
        finally:
            self._loading_future = None

        from creation_lib.renderer.nif_loader import upload_nif_to_gpu

        game_id = prepared.game_profile.id if prepared.game_profile else "fo4"
        self.renderer.ensure_game_backend(game_id)
        program = self.renderer.programs.get(game_id) or self.renderer.programs.get(
            "default"
        )
        _t0_gpu = _t_prev_gpu = time.perf_counter()
        scene_root, nif = upload_nif_to_gpu(prepared, self.ctx, program)
        _t = time.perf_counter(); _log.debug("[nif-timing] upload_nif_to_gpu: %.1f ms", (_t - _t_prev_gpu) * 1000); _t_prev_gpu = _t

        anim_mgr = self._create_animation_manager()
        anim_mgr.scan(nif)
        _t = time.perf_counter(); _log.debug("[nif-timing] anim_scan: %.1f ms", (_t - _t_prev_gpu) * 1000); _t_prev_gpu = _t
        particle_models, particle_runtime = self._create_particle_runtime(
            nif,
            prepared.nif_id,
            texture_dirs=prepared.texture_dirs,
            ba2_mgr=prepared.ba2_mgr,
        )
        session = NifSession(
            nif_id=prepared.nif_id,
            nif=nif,
            file_path=prepared.filepath,
            scene_root=scene_root,
            anim_manager=anim_mgr,
            game_profile=prepared.game_profile,
            particle_models=particle_models,
            particle_runtime=particle_runtime,
        )
        if prepared.nif_id == "main":
            self.nif_watcher.stop_watching()
            self._nif_reload_pending = None
            self.registry.clear()
        self.registry.add_session(session)
        _t = time.perf_counter(); _log.debug("[nif-timing] session_register: %.1f ms", (_t - _t_prev_gpu) * 1000); _t_prev_gpu = _t
        self.registry.active_id = prepared.nif_id
        self.renderer.scene_root = self.registry.get_session("main").scene_root
        self.renderer.clear_alt_vao_cache()

        # Starfield parallel render engine: when a Starfield NIF loads, build
        # a self-contained SFScene (ui.editor.sf_engine) that bypasses the
        # editor's shared shader/material pipeline entirely. Other games keep
        # the normal draw path.
        try:
            _sf_game_id = prepared.game_profile.id if prepared.game_profile else ""
            if _sf_game_id == "starfield" and prepared.nif_id == "main":
                from pathlib import Path as _Path

                # Extracted dir comes from toolkit settings — the standalone
                # reads a hard-coded EXTRACTED constant, but the editor routes
                # through ToolkitSettings.get_game_paths("starfield").
                _ts = getattr(self, "_toolkit_settings", None)
                _extracted = None
                if _ts is not None:
                    try:
                        _gp = _ts.get_game_paths("starfield") or {}
                        _extracted = _gp.get("extracted_dir")
                    except Exception:
                        _extracted = None
                if not _extracted:
                    _extracted = str(_Path("extracted") / "starfield")
                _exr = _Path("resource/monochrome_studio_02_1k.exr")
                self.renderer.load_sf_scene(
                    prepared.filepath,
                    _extracted,
                    _exr if _exr.exists() else None,
                )
            elif _sf_game_id != "starfield":
                # Releasing any stale SF scene prevents the bypass in
                # render_scene from firing against the wrong data when the
                # user switches to a non-SF NIF.
                self.renderer.unload_sf_scene()
        except Exception:
            _log.exception("Failed to build Starfield render engine scene")
        _log.debug("[nif-timing] TOTAL _poll_loading: %.1f ms", (time.perf_counter() - _t0_gpu) * 1000)
        self.status_text = f"Loaded: {self._loading_filename}"
        self.selection_mgr.clear()
        self.selection_mgr.register_bounds(scene_root)
        center, radius = self._aggregate_bounds(scene_root)
        self._scene_radius = radius
        if radius > 0:
            self.camera.frame_on_bounds(center, radius)
            self.renderer.init_grid(scene_radius=radius)
        from .recent_files import add as _add_recent

        _add_recent(prepared.filepath)
        self.nif_watcher.start_watching(prepared.filepath)
        self._refresh_asset_watchers()
        self.undo_manager.clear()
        self.light_display._needs_rebuild = True

        # Spinner turns off only after ALL work is complete
        self._loading = False

    @staticmethod
    def _watch_key(path: str) -> str:
        return os.path.normcase(os.path.abspath(path))

    def _refresh_asset_watchers(self) -> None:
        texture_watcher = getattr(self, "texture_watcher", None)
        if texture_watcher:
            from creation_lib.renderer.material_pipeline import get_loaded_texture_paths

            tex_paths = get_loaded_texture_paths()
            texture_watcher.start({p: p for p in tex_paths})

        material_watcher = getattr(self, "material_watcher", None)
        if not material_watcher:
            return

        from creation_lib.renderer.material_pipeline import collect_nif_material_paths

        material_paths: dict[str, str] = {}
        material_watch_nif_ids: dict[str, set[str]] = {}
        registry = getattr(self, "registry", None)
        sessions = registry.all_sessions() if registry else []
        for session in sessions:
            try:
                texture_dirs, *_ = self._build_texture_dirs(
                    session.file_path,
                    game_profile=session.game_profile,
                )
                session_materials = collect_nif_material_paths(
                    session.nif,
                    texture_dirs,
                    self.ba2_manager,
                )
            except Exception:
                _log.exception(
                    "Failed to collect material watches for %s",
                    session.file_path,
                )
                continue
            for abs_path, material_key in session_materials.items():
                material_paths[abs_path] = material_key
                material_watch_nif_ids.setdefault(
                    self._watch_key(abs_path), set()
                ).add(session.nif_id)

        self._material_watch_nif_ids = material_watch_nif_ids
        material_watcher.start(material_paths)

    def _handle_texture_reloads(self) -> list[str]:
        texture_watcher = getattr(self, "texture_watcher", None)
        if not texture_watcher:
            return []

        reloaded = []
        for tex_path in texture_watcher.check_reloads():
            from creation_lib.renderer.material_pipeline import reload_texture_inplace

            if self.ctx and reload_texture_inplace(self.ctx, tex_path):
                reloaded.append(tex_path)
        return reloaded

    def _handle_material_reloads(self) -> list[str]:
        material_watcher = getattr(self, "material_watcher", None)
        if not material_watcher or not self.registry.sessions:
            return []

        changed_paths = material_watcher.check_reloads()
        if not changed_paths:
            return []

        from creation_lib.renderer.material_pipeline import invalidate_material_cache

        changed_nif_ids: set[str] = set()
        for path in changed_paths:
            invalidate_material_cache(path)
            changed_nif_ids.update(
                self._material_watch_nif_ids.get(self._watch_key(path), set())
            )
        if not changed_nif_ids:
            changed_nif_ids.add(self.registry.active_id)

        rebuilt = []
        for session in self.registry.all_sessions():
            if session.nif_id not in changed_nif_ids:
                continue
            self.rebuild_scene_from_nif(session.nif_id)
            rebuilt.append(session.nif_id)

        self._refresh_asset_watchers()
        name = Path(changed_paths[0]).name
        if len(changed_paths) == 1:
            self.status_text = f"Reloaded material: {name}"
        else:
            self.status_text = f"Reloaded {len(changed_paths)} materials"
        return rebuilt

    def _poll_attaching(self):
        """Check if a background attach has completed; if so, finalize on UI thread.

        Called every frame from nif_workspace.draw(). Does nothing if no attach
        is in progress or the future is not yet done.
        """
        if not self._attaching:
            return
        if self._attach_future is None:
            self._attaching = False
            return
        if not self._attach_future.done():
            return

        try:
            attach_data = self._attach_future.result()
        except Exception as e:
            self._attaching = False
            self.status_text = f"Attach error: {e}"
            return
        finally:
            self._attach_future = None

        from creation_lib.renderer.nif_loader import upload_nif_to_gpu, _update_world_transforms

        parent_session = self.registry.get_session(attach_data.parent_nif_id)
        game_id = (
            attach_data.prepared.game_profile.id
            if attach_data.prepared.game_profile
            else "fo4"
        )
        self.renderer.ensure_game_backend(game_id)
        program = self.renderer.programs.get(game_id) or self.renderer.programs.get(
            "default"
        )
        scene_root, nif = upload_nif_to_gpu(attach_data.prepared, self.ctx, program)

        anim_mgr = self._create_animation_manager()
        anim_mgr.scan(nif)

        # Cross-game conversion (if needed)
        child_profile = attach_data.prepared.game_profile
        parent_game = getattr(parent_session, "game_profile", None)
        if child_profile and parent_game and child_profile.id != parent_game.id:
            converted = self._native_convert_child_to_parent_game(
                attach_data.prepared.filepath,
                child_profile,
                parent_game,
            )
            if converted is not None:
                nif, _report = converted
                child_profile = parent_game

        # Replace any existing attachment at the same parent + connect point
        for existing in list(self.registry.all_sessions()):
            if (
                existing.attachment_point == attach_data.matched_cp
                and existing.parent_nif_id == attach_data.parent_nif_id
            ):
                self.detach_nif(existing.nif_id, _force=True)
                break

        # Connect point offset + parent CP world transform
        child_cp_name = attach_data.matched_cp.replace("P-", "C-").replace("p-", "c-")
        child_cp_offset = self._find_child_connect_point(nif, child_cp_name)
        cp_world = self._get_cp_world_transform(parent_session, attach_data.matched_cp)

        # Build AttachmentNode + graft
        from .nif_session import AttachmentNode

        nif_id = attach_data.prepared.nif_id
        particle_models, particle_runtime = self._create_particle_runtime(
            nif,
            nif_id,
            texture_dirs=attach_data.prepared.texture_dirs,
            ba2_mgr=attach_data.prepared.ba2_mgr,
        )
        attach_node = AttachmentNode(
            name=f"attach_{attach_data.matched_cp}",
            block_id=-1,
            parent_nif_id=attach_data.parent_nif_id,
            child_nif_id=nif_id,
            connect_point_name=attach_data.matched_cp,
        )
        attach_node.transform = cp_world
        if child_cp_offset is not None:
            attach_node.transform = attach_node.transform * child_cp_offset
        attach_node.children.append(scene_root)
        parent_session.scene_root.children.append(attach_node)

        # Create + register session
        session = NifSession(
            nif_id=nif_id,
            nif=nif,
            file_path=attach_data.prepared.filepath,
            scene_root=scene_root,
            anim_manager=anim_mgr,
            parent_nif_id=attach_data.parent_nif_id,
            attachment_point=attach_data.matched_cp,
            attachment_node=attach_node,
            game_profile=child_profile,
            particle_models=particle_models,
            particle_runtime=particle_runtime,
        )
        self.registry.add_session(session)
        self._refresh_asset_watchers()

        # Rebuild
        _update_world_transforms(self.renderer.scene_root, glm.mat4(1.0))
        self._rebuild_selection_bounds()
        self.connect_points._needs_rebuild = True

        self.status_text = (
            f"Attached {self._attach_filename} at {attach_data.matched_cp}"
        )
        self._attaching = False

    def _rebuild_connect_points(self):
        """Rebuild connect point display if flagged dirty."""
        if not (
            self.connect_points._needs_rebuild
            and self.nif
            and self.renderer
            and self.ctx
        ):
            return
        cp_prog = self.renderer.programs.get("connect_point")
        if not cp_prog:
            return
        sr = getattr(self, "_scene_radius", 0.0)
        if sr <= 0 and self.renderer.scene_root:
            _, sr = self._aggregate_bounds(self.renderer.scene_root)
        self.connect_points.rebuild(self.registry, self.ctx, cp_prog, scene_radius=sr)
        self.connect_points._needs_rebuild = False
        self.selection_mgr.register_extra_nodes(self.connect_points.cp_nodes)

    def _native_convert_child_to_parent_game(self, file_path, child_profile, parent_game):
        if not (child_profile and parent_game and child_profile.id != parent_game.id):
            return None
        try:
            import tempfile

            from creation_lib.nif import native_runtime as nif_native_runtime
            from creation_lib.nif.nif_file import NifFile

            with tempfile.TemporaryDirectory(prefix="modkit_nif_convert_") as tmp:
                out_path = Path(tmp) / Path(file_path).name
                report = nif_native_runtime.convert_nif_file_raw(
                    str(file_path),
                    str(out_path),
                    child_profile.id,
                    parent_game.id,
                    None,
                    {"source_path": str(file_path)},
                )
                if not report.get("supported"):
                    _log.error("Cross-game conversion failed: %s", report.get("errors", []))
                    return None
                return NifFile.load(str(out_path)), report
        except Exception as e:
            _log.error("Cross-game conversion error: %s", e)
            return None

    def _aggregate_bounds(self, node) -> tuple:
        """Compute aggregate bounding sphere of entire scene."""
        centers, radii = [], []

        def _collect(n):
            if n.bound_radius > 0:
                centers.append(n.bound_center)
                radii.append(n.bound_radius)
            for c in n.children:
                _collect(c)

        _collect(node)
        if not centers:
            return glm.vec3(0), 0.0
        avg = sum(centers, glm.vec3(0)) / len(centers)
        max_r = max(glm.length(c - avg) + r for c, r in zip(centers, radii))
        return avg, max_r

    def toggle_node_visibility(self, block_id: int):
        """Toggle display visibility of a scene node (does not affect the NIF file)."""
        if block_id in self.hidden_block_ids:
            self.hidden_block_ids.discard(block_id)
        else:
            self.hidden_block_ids.add(block_id)
        self._sync_visibility(self.nif_root)

    def _on_selection_changed(self, nif_id, block_id):
        """Update active session when selection changes to a different NIF."""
        _log.info(
            "Selection changed: nif_id=%s block_id=%s (active=%s)",
            nif_id,
            block_id,
            self.registry.active_id,
        )
        if nif_id and nif_id in self.registry.sessions:
            self.registry.active_id = nif_id

    def _sync_visibility(self, node):
        """Recursively sync SceneNode.visible flags from hidden_block_ids set."""
        if node is None:
            return
        node.visible = node.block_id not in self.hidden_block_ids
        for child in node.children:
            self._sync_visibility(child)

    def _apply_node_gizmo(self, node, new_world_transform, nif=None):
        """Write gizmo-modified world transform back to a NIF node's local transform fields."""
        nif = nif or self.nif
        block = nif.get_block(node.block_id)
        if not block:
            return

        # Compute local transform: local = inv(parent_world) * new_world
        parent_world = glm.mat4(1.0)
        # Build parent map and walk up
        parent_map = {}
        for b in nif.blocks:
            for child_id in b.get_field("Children") or []:
                if isinstance(child_id, int) and child_id >= 0:
                    parent_map[child_id] = b.block_id
        parent_id = parent_map.get(node.block_id)
        if parent_id is not None:
            # Find the parent SceneNode's world transform
            parent_node = self._find_scene_node(
                self.renderer.scene_root, parent_id, node.nif_id
            )
            if parent_node:
                parent_world = parent_node.world_transform

        local = glm.inverse(parent_world) * new_world_transform

        # Extract translation
        tx, ty, tz = float(local[3][0]), float(local[3][1]), float(local[3][2])
        trans = block.get_field("Translation")
        if isinstance(trans, dict):
            trans["x"], trans["y"], trans["z"] = tx, ty, tz
        else:
            block.set_field("Translation", {"x": tx, "y": ty, "z": tz})

        # Extract scale (length of first column)
        sx = glm.length(glm.vec3(local[0][0], local[0][1], local[0][2]))
        if sx > 0.001:
            block.set_field("Scale", float(sx))

        # Extract rotation (3x3, remove scale)
        rot = block.get_field("Rotation")
        if isinstance(rot, list) and len(rot) == 9:
            inv_s = 1.0 / max(sx, 0.001)
            rot[0] = float(local[0][0]) * inv_s
            rot[1] = float(local[0][1]) * inv_s
            rot[2] = float(local[0][2]) * inv_s
            rot[3] = float(local[1][0]) * inv_s
            rot[4] = float(local[1][1]) * inv_s
            rot[5] = float(local[1][2]) * inv_s
            rot[6] = float(local[2][0]) * inv_s
            rot[7] = float(local[2][1]) * inv_s
            rot[8] = float(local[2][2]) * inv_s

        # Recompute bounding sphere
        from creation_lib.renderer.nif_loader import _compute_bounds_from_verts

        local_verts = getattr(node, "_local_verts", None)
        if local_verts is not None and len(local_verts) > 0:
            node.bound_center, node.bound_radius = _compute_bounds_from_verts(
                local_verts, new_world_transform
            )

    def _find_scene_node(self, root, block_id, nif_id):
        """Find a SceneNode by block_id and nif_id in the scene graph."""
        if root is None:
            return None
        if root.block_id == block_id and root.nif_id == nif_id:
            return root
        for child in root.children:
            result = self._find_scene_node(child, block_id, nif_id)
            if result:
                return result
        return None

    def _apply_light_gizmo(self, light_node, new_transform, nif=None):
        """Write gizmo-modified world transform back to the NIF light block."""
        nif = nif or self.nif
        block = nif.get_block(light_node.block_id)
        if not block:
            return

        # Compute local transform via parent world matrix (handles rotation/scale)
        parent_world = self._get_light_parent_world_matrix(nif, light_node.block_id)
        local = glm.inverse(parent_world) * new_transform

        # Extract local translation
        lx = float(local[3][0])
        ly = float(local[3][1])
        lz = float(local[3][2])

        trans = block.get_field("Translation")
        if isinstance(trans, dict):
            trans["x"] = lx
            trans["y"] = ly
            trans["z"] = lz
        else:
            block.set_field("Translation", {"x": lx, "y": ly, "z": lz})

        # Keep pick sphere in sync with world position
        wx = float(new_transform[3][0])
        wy = float(new_transform[3][1])
        wz = float(new_transform[3][2])
        light_node.bound_center = glm.vec3(wx, wy, wz)

        # Rebuild light display so icon + shader data update
        self.light_display._needs_rebuild = True

    def _apply_cp_gizmo(self, cp_node, new_transform, nif=None):
        """Write gizmo-modified world position back to the connect point's Translation."""
        nif = nif or self.nif
        cp_block = nif.get_block(cp_node._cp_block_id)
        if not cp_block:
            return

        connect_points = cp_block.get_field("Connect Points") or []
        idx = cp_node._cp_index
        if idx >= len(connect_points):
            return
        cp = connect_points[idx]
        if not isinstance(cp, dict):
            return

        # World translation from gizmo
        wx = float(new_transform[3][0])
        wy = float(new_transform[3][1])
        wz = float(new_transform[3][2])

        # Convert world → local: undo owner world rotation and translation
        owner_world_rot = cp_node._cp_owner_world_rot  # 3x3 numpy array
        import numpy as np

        owner_world_pos = _get_owner_world_pos(nif, cp_node._cp_owner_id)
        world_offset = np.array([wx, wy, wz]) - np.array(owner_world_pos)
        # local = inv(owner_rot) @ world_offset = owner_rot.T @ world_offset (orthonormal)
        local_pos = owner_world_rot.T @ world_offset

        trans = cp.get("Translation")
        if isinstance(trans, dict):
            trans["x"] = float(local_pos[0])
            trans["y"] = float(local_pos[1])
            trans["z"] = float(local_pos[2])
        else:
            cp["Translation"] = {
                "x": float(local_pos[0]),
                "y": float(local_pos[1]),
                "z": float(local_pos[2]),
            }

        # Update pick sphere
        cp_node.bound_center = glm.vec3(wx, wy, wz)
        cp_node._cp_owner_world_rot = owner_world_rot  # unchanged

        # Rebuild CP display so the marker moves
        self.connect_points._needs_rebuild = True

        # Sync any child NIFs attached to this connect point
        nif_id = cp_node.nif_id or "main"
        cp_name = cp_node.name  # e.g. "P-Barrel"
        for child_session in self.registry.get_children(nif_id):
            if (
                child_session.attachment_point == cp_name
                and child_session.attachment_node
            ):
                parent_session = self.registry.get_session(nif_id)
                new_cp_world = self._get_cp_world_transform(parent_session, cp_name)
                child_session.attachment_node.transform = new_cp_world
                from creation_lib.renderer.nif_loader import _update_world_transforms

                _update_world_transforms(
                    child_session.attachment_node,
                    parent_session.scene_root.world_transform,
                )

    def _get_light_parent_world_matrix(self, nif, block_id: int) -> glm.mat4:
        """Return the full world transform matrix of the parent of a light block."""
        if not nif:
            return glm.mat4(1.0)
        # Build parent map
        parent_map = {}
        for b in nif.blocks:
            if not nif.schema.is_subtype_of(b.type_name, "NiNode"):
                continue
            for field in ("Children", "Effects"):
                for ref in b.get_field(field) or []:
                    rid = (
                        int(ref)
                        if isinstance(ref, (int, float))
                        else int(ref.get("value", -1))
                    )
                    if rid >= 0:
                        parent_map[rid] = b.block_id
        parent_id = parent_map.get(block_id)
        if parent_id is None:
            return glm.mat4(1.0)

        # Walk from root to parent, accumulating transforms
        chain = []
        bid = parent_id
        while bid is not None:
            chain.append(bid)
            bid = parent_map.get(bid)
        chain.reverse()

        world = glm.mat4(1.0)
        for bid in chain:
            block = nif.get_block(bid)
            if not block:
                continue
            trans = block.get_field("Translation") or {}
            tx = float(trans.get("x", 0))
            ty = float(trans.get("y", 0))
            tz = float(trans.get("z", 0))
            scale = float(block.get_field("Scale") or 1.0)
            rot = block.get_field("Rotation") or {}
            # Build local 4x4 from TRS
            local = glm.mat4(1.0)
            # Rotation (nif.xml Matrix33: m[col][row])
            local[0][0] = float(rot.get("m11", 1)) * scale
            local[0][1] = float(rot.get("m12", 0)) * scale
            local[0][2] = float(rot.get("m13", 0)) * scale
            local[1][0] = float(rot.get("m21", 0)) * scale
            local[1][1] = float(rot.get("m22", 1)) * scale
            local[1][2] = float(rot.get("m23", 0)) * scale
            local[2][0] = float(rot.get("m31", 0)) * scale
            local[2][1] = float(rot.get("m32", 0)) * scale
            local[2][2] = float(rot.get("m33", 1)) * scale
            local[3][0] = tx
            local[3][1] = ty
            local[3][2] = tz
            world = world * local
        return world

    def open_collision_dialog(
        self,
        node_block_id: int,
        source_block_ids: list[int] | None = None,
    ) -> None:
        """Open the Generate Collision modal targeting the given node block."""
        dlg = getattr(self, "collision_dialog", None)
        if dlg is not None:
            dlg.open(node_block_id, source_block_ids=source_block_ids)

    def rebuild_scene_from_nif(self, nif_id: str | None = None):
        """Rebuild scene graph for a session after undo/redo/mutation."""
        if not self.registry.sessions or not self.renderer:
            return
        from creation_lib.renderer.nif_loader import (
            rebuild_scene_from_nif,
            _update_world_transforms,
        )

        nid = nif_id or self.registry.active_id
        session = self.registry.get_session(nid)
        texture_dirs, *_ = self._build_texture_dirs(
            session.file_path, game_profile=session.game_profile
        )

        game_id = session.game_profile.id if session.game_profile else "fo4"
        self.renderer.ensure_game_backend(game_id)
        program = self.renderer.programs.get(game_id) or self.renderer.programs.get(
            "default"
        )
        scene_root = rebuild_scene_from_nif(
            session.nif,
            self.ctx,
            program,
            texture_dirs,
            self.ba2_manager,
            nif_id=nid,
            game_profile=session.game_profile,
        )
        session.scene_root = scene_root

        # Re-graft into main scene graph if this is a child
        if session.attachment_node:
            session.attachment_node.children = [scene_root]
        elif nid == "main":
            self.renderer.scene_root = scene_root

        # Re-apply hidden block visibility — new SceneNodes default to visible=True
        self._sync_visibility(scene_root)

        # Invalidate anim manager node cache — old SceneNode refs are now stale
        session.anim_manager.scan(session.nif)
        previous_particle_runtime = getattr(session, "particle_runtime", None)
        session.particle_models, session.particle_runtime = self._create_particle_runtime(
            session.nif,
            nid,
            texture_dirs=texture_dirs,
            ba2_mgr=self.ba2_manager,
        )
        restore_playback = getattr(session.particle_runtime, "restore_playback_from", None)
        if callable(restore_playback) and previous_particle_runtime is not None:
            restore_playback(previous_particle_runtime)
        session.anim_manager._node_cache.clear()
        session.anim_manager._rest_transforms.clear()

        self.renderer.clear_alt_vao_cache()
        self._rebuild_selection_bounds()
        self.light_display._needs_rebuild = True

    def _rebuild_selection_bounds(self):
        """Re-register all scene nodes for picking."""
        main_root = (
            self.registry.get_session("main").scene_root
            if "main" in self.registry.sessions
            else None
        )
        if main_root:
            self.selection_mgr.clear()
            self.selection_mgr.register_bounds(main_root)

    def _process_keybindings(self):
        if not self.active:
            return
        io = imgui.get_io()
        if io.want_capture_keyboard:
            return

        # File operations — dynamic: multi-NIF swaps Save/SaveAs for SaveAll/SaveAllAs
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.s):
            if self.registry.has_multiple_nifs:
                if io.key_shift:
                    self.toolbar._save_all_as()
                else:
                    self._save_all()
            else:
                if io.key_shift:
                    self.toolbar._save_as()
                else:
                    self._save()
        if imgui.is_key_pressed(imgui.Key.o) and not io.key_ctrl:
            self._open_file_dialog()

        # Camera
        if imgui.is_key_pressed(imgui.Key.f):
            if self.nif_root:
                center, radius = self._aggregate_bounds(self.nif_root)
                if radius > 0:
                    self.camera.frame_on_bounds(center, radius)

        # Undo/redo
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.z):
            self._undo()
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.y):
            self._redo()

        # Render view toggles (1-5)
        if self.render_mode_mgr:
            for i in range(1, 6):
                key = getattr(imgui.Key, f"_{i}", None)
                if key and imgui.is_key_pressed(key):
                    self._toggle_render_mode(RenderMode(i))

        # Gizmo modes — Blender: G/R/S, others: W/E/R
        nav = getattr(self.camera, "nav_style", "default")
        if nav == "blender":
            if imgui.is_key_pressed(imgui.Key.g):
                self.gizmo.set_operation(TRANSLATE)
            if imgui.is_key_pressed(imgui.Key.r):
                self.gizmo.set_operation(ROTATE)
            if imgui.is_key_pressed(imgui.Key.s):
                self.gizmo.set_operation(SCALE)
        else:
            if imgui.is_key_pressed(imgui.Key.w):
                self.gizmo.set_operation(TRANSLATE)
            if imgui.is_key_pressed(imgui.Key.e):
                self.gizmo.set_operation(ROTATE)
            if imgui.is_key_pressed(imgui.Key.r):
                self.gizmo.set_operation(SCALE)

        # Command palette
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.p):
            self.show_command_palette = not self.show_command_palette

        # Escape — deactivate gizmo first, then deselect
        if imgui.is_key_pressed(imgui.Key.escape):
            if self.gizmo.manipulate_active:
                self.gizmo.deactivate_manipulate()
            else:
                self.selection_mgr.deselect()

    def _toggle_render_mode(self, mode: RenderMode) -> None:
        if self.render_mode_mgr:
            self.render_mode_mgr.toggle_mode(mode)

    def _save(self):
        """Save — single-NIF: immediate. Multi-NIF: popup."""
        if not self.registry.sessions:
            return
        # Block saving read-only sessions
        if self.registry.active_session.read_only:
            self.status_text = "Cannot save: read-only session"
            return
        if self.registry.has_multiple_nifs:
            self._show_save_dialog = True
        else:
            self._save_session("main")

    def _save_all(self):
        """Save all dirty NIFs."""
        for session in self.registry.all_sessions():
            if session.dirty and not session.read_only:
                self._save_session(session.nif_id)

    def _save_session(self, nif_id: str):
        """Save a single session to disk."""
        session = self.registry.get_session(nif_id)
        try:
            session.nif.save(session.file_path)
            self.nif_watcher.mark_saved(session.file_path)
            session.dirty = False
            self.status_text = f"Saved: {Path(session.file_path).name}"
            _log.info("Saved NIF: %s", session.file_path)
        except Exception as e:
            _log.error("Failed to save %s: %s", nif_id, e)
            self.status_text = f"Save error: {e}"

    def _draw_about_window(self):
        """Draw the Help -> About popup."""
        if not self._show_about:
            return
        imgui.open_popup("About NIF Editor")
        flags = (
            imgui.WindowFlags_.no_resize.value
            | imgui.WindowFlags_.always_auto_resize.value
        )
        if imgui.begin_popup_modal("About NIF Editor", None, flags)[0]:
            imgui.text_colored(imgui.ImVec4(0.9, 0.8, 0.5, 1.0), "NIF Editor")
            imgui.separator()

            imgui.spacing()
            imgui.text("Slider Controls")
            imgui.separator()
            imgui.bullet_text("Ctrl+Click  — type a value directly into any slider")
            imgui.bullet_text("Click+drag  — scrub the slider")
            imgui.bullet_text("Sliders clamp to their displayed min/max range")

            imgui.spacing()
            imgui.text("Keyboard Shortcuts")
            imgui.separator()
            imgui.bullet_text("Ctrl+Z / Ctrl+Y  — Undo / Redo")
            imgui.bullet_text("Ctrl+S           — Save active NIF")
            imgui.bullet_text("Ctrl+O           — Open NIF")
            imgui.bullet_text("Delete           — Remove selected block")
            imgui.bullet_text("F                — Frame selected")

            imgui.spacing()
            if imgui.button("Close", imgui.ImVec2(120, 0)):
                self._show_about = False
                imgui.close_current_popup()
            imgui.end_popup()
        else:
            self._show_about = False

    def _draw_nif_reload_dialog(self):
        """Draw 'NIF changed on disk — reload?' modal prompt."""
        if self._nif_reload_pending is None:
            return
        imgui.open_popup("##nif_changed")
        flags = (
            imgui.WindowFlags_.no_resize.value
            | imgui.WindowFlags_.always_auto_resize.value
        )
        opened, _ = imgui.begin_popup_modal("##nif_changed", None, flags)
        if opened:
            name = Path(self._nif_reload_pending).name
            imgui.text(f'"{name}" was modified externally.')
            imgui.text("Reload the file?")
            imgui.spacing()
            if imgui.button("Reload", imgui.ImVec2(120, 0)):
                path = self._nif_reload_pending
                self._nif_reload_pending = None
                self.load_nif(path)
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Dismiss", imgui.ImVec2(120, 0)):
                self._nif_reload_pending = None
                imgui.close_current_popup()
            imgui.end_popup()

    def _draw_save_dialog(self):
        """Draw multi-NIF save dialog with per-session checkboxes."""
        if not self._show_save_dialog:
            return
        imgui.open_popup("Save NIFs")
        if imgui.begin_popup_modal("Save NIFs")[0]:
            for session in self.registry.all_sessions():
                name = Path(session.file_path).name
                role = (
                    "(main)"
                    if not session.parent_nif_id
                    else f"({session.attachment_point})"
                )
                dirty = " *" if session.dirty else ""
                label = f"{name} {role}{dirty}"
                # Pre-check dirty sessions
                if session.nif_id not in self._save_dialog_checks:
                    self._save_dialog_checks[session.nif_id] = session.dirty
                changed, val = imgui.checkbox(
                    label, self._save_dialog_checks[session.nif_id]
                )
                if changed:
                    self._save_dialog_checks[session.nif_id] = val

            if imgui.button("Save Selected"):
                for nid, checked in self._save_dialog_checks.items():
                    if checked:
                        self._save_session(nid)
                self._show_save_dialog = False
                self._save_dialog_checks.clear()
            imgui.same_line()
            if imgui.button("Cancel"):
                self._show_save_dialog = False
                self._save_dialog_checks.clear()
            imgui.end_popup()

    def _draw_detach_dialog(self):
        """Draw save/discard/cancel dialog for detaching a dirty NIF."""
        if not self._show_detach_dialog or not self._pending_detach:
            return
        nif_id = self._pending_detach
        imgui.open_popup("Detach NIF")
        if imgui.begin_popup_modal("Detach NIF")[0]:
            try:
                session = self.registry.get_session(nif_id)
                name = Path(session.file_path).name
            except KeyError:
                name = nif_id
            imgui.text(f"{name} has unsaved changes.")
            if imgui.button("Save and Detach"):
                self._save_session(nif_id)
                self.detach_nif(nif_id, _force=True)
                self._show_detach_dialog = False
                self._pending_detach = None
            imgui.same_line()
            if imgui.button("Discard and Detach"):
                self.detach_nif(nif_id, _force=True)
                self._show_detach_dialog = False
                self._pending_detach = None
            imgui.same_line()
            if imgui.button("Cancel##detach"):
                self._show_detach_dialog = False
                self._pending_detach = None
            imgui.end_popup()

    def attach_nif(self, file_path: str, parent_nif_id: str, connect_point: str) -> str:
        """Attach a child NIF to a parent's connect point.

        Returns the new nif_id, or empty string on failure/cancel.
        """
        _log.info(
            "attach_nif: file=%s parent=%s cp=%s",
            file_path,
            parent_nif_id,
            connect_point,
        )
        try:
            parent_session = self.registry.get_session(parent_nif_id)
        except KeyError:
            _log.error("attach_nif: parent session '%s' not found", parent_nif_id)
            return ""

        nif_id = self.registry.next_child_id()
        _log.info("attach_nif: assigned nif_id=%s", nif_id)

        # Load the child NIF
        try:
            from creation_lib.renderer.nif_loader import load_nif_to_scene

            child_profile = self._detect_game_profile(file_path)
            texture_dirs, *_ = self._build_texture_dirs(
                file_path, game_profile=child_profile
            )
            child_game_id = child_profile.id if child_profile else "fo4"
            self.renderer.ensure_game_backend(child_game_id)
            scene_root, nif = load_nif_to_scene(
                file_path,
                self.ctx,
                self.renderer.programs.get(child_game_id)
                or self.renderer.programs.get("default"),
                texture_dirs,
                ba2_mgr=self.ba2_manager,
                nif_id=nif_id,
                game_profile=child_profile,
            )
            _log.info(
                "attach_nif: loaded %d blocks from %s", len(nif.blocks), file_path
            )
        except Exception:
            _log.exception("attach_nif: failed to load NIF %s", file_path)
            return ""

        # Scan animations
        anim_mgr = self._create_animation_manager()
        anim_mgr.scan(nif)

        # Cross-game auto-conversion: convert child NIF to parent's game format
        parent_game = getattr(parent_session, "game_profile", None)
        if child_profile and parent_game and child_profile.id != parent_game.id:
            source_name = child_profile.display_name
            converted = self._native_convert_child_to_parent_game(
                file_path,
                child_profile,
                parent_game,
            )
            if converted is not None:
                nif, report = converted
                child_profile = parent_game
                warnings = report.get("warnings", []) or []
                changes = report.get("changes", []) or []
                _log.info(
                    "Cross-game attachment: converted %s -> %s (%d changes, %d warnings)",
                    source_name,
                    parent_game.display_name,
                    len(changes),
                    len(warnings),
                )
                for w in warnings:
                    _log.warning("  Conversion warning: %s", w)

        # Validate connect point — look for matching c-record in child
        child_cp_name = connect_point.replace("P-", "C-").replace("p-", "c-")
        _log.info("attach_nif: looking for child CP '%s'", child_cp_name)
        child_cp_offset = self._find_child_connect_point(nif, child_cp_name)
        if child_cp_offset is None:
            _log.warning(
                "attach_nif: no matching child CP '%s' found in %s "
                "(attaching at parent CP origin)",
                child_cp_name,
                file_path,
            )

        # Compute parent CP world transform
        cp_world = self._get_cp_world_transform(parent_session, connect_point)
        _log.info("attach_nif: parent CP world transform: %s", cp_world)

        # Create AttachmentNode
        from .nif_session import AttachmentNode

        attach_node = AttachmentNode(
            name=f"attach_{connect_point}",
            block_id=-1,
            parent_nif_id=parent_nif_id,
            child_nif_id=nif_id,
            connect_point_name=connect_point,
        )
        attach_node.transform = cp_world
        if child_cp_offset is not None:
            attach_node.transform = attach_node.transform * child_cp_offset
        attach_node.children.append(scene_root)

        # Graft into parent scene graph
        parent_session.scene_root.children.append(attach_node)
        _log.info("attach_nif: grafted into parent scene graph")

        # Starfield: also load the attachment through SfBackend so its
        # meshes appear in sf_scene.meshes with the PBR shader. The FO4
        # graft above stays — picking, animation, and the connect-points
        # UI all walk scene_root, which still needs the FO4-shaped node
        # tree. The ``_sf_loaded`` marker tells SfBackend.render_full to
        # skip this attachment in the FO4 fallback pass so it doesn't
        # double-render.
        try:
            _parent_game = getattr(parent_session, "game_profile", None)
            if _parent_game and _parent_game.id == "starfield":
                from creation_lib.renderer.backends.sf_backend import SfBackend

                if isinstance(self.renderer.backend, SfBackend):
                    import numpy as _np

                    _xform = attach_node.transform
                    _xform_np = _np.array(
                        [[_xform[c][r] for c in range(4)] for r in range(4)],
                        dtype=_np.float32,
                    )
                    sf_meshes = self.renderer.backend.attach_nif(
                        Path(file_path), _xform_np
                    )
                    if sf_meshes:
                        attach_node._sf_loaded = True
                        # Stash the list so detach_nif can remove these
                        # exact meshes from sf_scene.meshes later.
                        attach_node._sf_meshes = sf_meshes
                        _log.info(
                            "attach_nif: SF native load OK — %d meshes added "
                            "to sf_scene",
                            len(sf_meshes),
                        )
                    else:
                        _log.warning(
                            "attach_nif: SF native load returned no meshes — "
                            "falling back to FO4 attach pass for visibility"
                        )
        except Exception:
            _log.exception(
                "attach_nif: SF native load raised — FO4 fallback will render"
            )

        # Create session
        particle_models, particle_runtime = self._create_particle_runtime(
            nif,
            nif_id,
            texture_dirs=texture_dirs,
            ba2_mgr=self.ba2_manager,
        )
        session = NifSession(
            nif_id=nif_id,
            nif=nif,
            file_path=file_path,
            scene_root=scene_root,
            anim_manager=anim_mgr,
            parent_nif_id=parent_nif_id,
            attachment_point=connect_point,
            attachment_node=attach_node,
            game_profile=child_profile,
            particle_models=particle_models,
            particle_runtime=particle_runtime,
        )
        self.registry.add_session(session)
        self._refresh_asset_watchers()

        # Rebuild world transforms and selection bounds
        from creation_lib.renderer.nif_loader import _update_world_transforms

        _update_world_transforms(self.renderer.scene_root, glm.mat4(1.0))
        self._rebuild_selection_bounds()
        self.connect_points._needs_rebuild = True

        _log.info(
            "attach_nif: SUCCESS — %s attached as %s at %s",
            Path(file_path).name,
            nif_id,
            connect_point,
        )
        self.status_text = f"Attached: {Path(file_path).name}"
        return nif_id

    def attach_nif_auto(self, filepath: str) -> None:
        """Submit async attach to background thread.

        Raises ValueError immediately if no main NIF is loaded (sync guard).
        All other errors (no CPs, no match) surface via _poll_attaching.
        If the matched CP is already occupied, the old attachment is replaced.
        """
        try:
            self.registry.get_session("main")
        except KeyError:
            raise ValueError("No main NIF loaded — open a NIF before attaching")

        parent_session = self.registry.active_session

        # Snapshot state on UI thread before submitting
        parent_cp_names: set[str] = set()
        for block in parent_session.nif.blocks:
            if block.type_name == "BSConnectPoint::Parents":
                connect_points = block.get_field("Connect Points") or []
                for cp in connect_points:
                    if isinstance(cp, dict):
                        cp_name = cp.get("Name", "")
                        if isinstance(cp_name, list):
                            cp_name = "".join(str(c) for c in cp_name)
                        parent_cp_names.add(cp_name)

        child_profile = self._detect_game_profile(filepath)
        texture_dirs, *_ = self._build_texture_dirs(
            filepath, game_profile=child_profile
        )
        nif_id = self.registry.next_child_id()

        from creation_lib.renderer.nif_loader import prepare_attach_data

        self._attach_future = self._load_executor.submit(
            prepare_attach_data,
            filepath,
            texture_dirs,
            self.ba2_manager,
            nif_id,
            parent_cp_names,
            set(),  # don't block on occupied CPs — _poll_attaching replaces them
            child_profile,
            parent_session.nif_id,
        )
        self._attaching = True
        self._attach_filename = Path(filepath).name
        self.status_text = f"Attaching {self._attach_filename}…"

    def bash_nif(self, filepath: str) -> None:
        """Bash (merge) a NIF file into the currently loaded root NIF."""
        from creation_lib.nif.nif_file import NifFile
        from .nif_bash import bash_nif

        if not self.nif_file:
            self.status_text = "No NIF loaded — open a NIF before bashing"
            return

        try:
            source = NifFile.load(filepath)
        except Exception as e:
            self.status_text = f"Failed to load source NIF: {e}"
            return

        result = bash_nif(self, source, source_path=filepath)

        if result.error:
            self.status_text = f"Bash failed: {result.error}"
            return

        self.rebuild_scene_from_nif()

        filename = Path(filepath).name
        msg = f"Bashed {filename} ({result.blocks_added} blocks added)"
        if result.skipped:
            msg += f", {len(result.skipped)} skipped"
        self.status_text = msg

    def _open_nif_read_only(self, path: str) -> None:
        """Open a NIF as a read-only session (for ADDN preview)."""
        try:
            from creation_lib.renderer.nif_loader import load_nif_to_scene

            game_profile = self._detect_game_profile(path)
            texture_dirs, *_ = self._build_texture_dirs(path, game_profile=game_profile)
            game_id = game_profile.id if game_profile else "fo4"
            if self.renderer:
                self.renderer.ensure_game_backend(game_id)
            program = (
                (
                    self.renderer.programs.get(game_id)
                    or self.renderer.programs.get("default")
                )
                if self.renderer
                else None
            )
            if not program:
                _log.warning("Cannot open read-only NIF: no shader program")
                return

            nif_id = self.registry.next_child_id()
            scene_root, nif = load_nif_to_scene(
                path,
                self.ctx,
                program,
                texture_dirs,
                ba2_mgr=self.ba2_manager,
                nif_id=nif_id,
                game_profile=game_profile,
            )

            anim_mgr = self._create_animation_manager()
            anim_mgr.scan(nif)

            particle_models, particle_runtime = self._create_particle_runtime(
                nif,
                nif_id,
                texture_dirs=texture_dirs,
                ba2_mgr=self.ba2_manager,
            )
            session = NifSession(
                nif_id=nif_id,
                nif=nif,
                file_path=path,
                scene_root=scene_root,
                anim_manager=anim_mgr,
                game_profile=game_profile,
                read_only=True,
                particle_models=particle_models,
                particle_runtime=particle_runtime,
            )
            self.registry.add_session(session)
            self.registry.active_id = nif_id

            # Add to scene graph and rebuild
            if self.renderer and self.renderer.scene_root:
                self.renderer.scene_root.children.append(scene_root)
                from creation_lib.renderer.nif_loader import _update_world_transforms

                _update_world_transforms(self.renderer.scene_root, glm.mat4(1.0))
                self._rebuild_selection_bounds()

            self.status_text = f"Opened (read-only): {Path(path).name}"
            _log.info("Opened read-only NIF: %s as %s", path, nif_id)
        except Exception as e:
            _log.error("Failed to open read-only NIF %s: %s", path, e, exc_info=True)
            self.status_text = f"Error opening NIF: {e}"

    def _find_child_connect_point(self, nif, cp_name: str):
        """Find a child connect point by name. Returns offset transform or None.

        BSConnectPoint::Children has "Point Name" field — an array of strings.
        Children have no position data (just names), so we return identity if matched.
        """
        _log.debug("_find_child_connect_point: looking for '%s'", cp_name)
        target = cp_name.replace("C-", "").replace("c-", "")
        for block in nif.blocks:
            if block.type_name == "BSConnectPoint::Children":
                point_names = block.get_field("Point Name") or []
                if isinstance(point_names, str):
                    point_names = [point_names]
                _log.debug(
                    "  found BSConnectPoint::Children with names: %s", point_names
                )
                for name in point_names:
                    if not isinstance(name, str):
                        name = str(name)
                    _log.debug("  checking name='%s' against target='%s'", name, target)
                    if (
                        name == cp_name
                        or name == target
                        or name == f"C-{target}"
                        or name == f"c-{target}"
                    ):
                        _log.info("  matched child CP '%s'", name)
                        # Children have no position offset — return identity
                        return glm.mat4(1.0)
        _log.debug("_find_child_connect_point: no match found")
        return None

    def _get_cp_world_transform(self, session, cp_name: str):
        """Get the world transform of a named parent connect point."""
        from creation_lib.renderer.overlays.connect_point import compute_cp_world_transform

        nif = session.nif
        world_pos, world_rot = compute_cp_world_transform(nif, cp_name)
        if world_pos is not None:
            # Build glm.mat4 from position + 3x3 rotation
            mat = glm.mat4(1.0)
            for r in range(3):
                for c in range(3):
                    mat[c][r] = float(world_rot[r, c])
            mat[3] = glm.vec4(
                float(world_pos[0]), float(world_pos[1]), float(world_pos[2]), 1.0
            )
            return mat
        _log.warning(
            "_get_cp_world_transform: CP '%s' not found, using identity", cp_name
        )
        return glm.mat4(1.0)

    def detach_nif(self, nif_id: str, save: bool = False, _force: bool = False):
        """Detach a child NIF. Prompts save/discard if dirty."""
        session = self.registry.get_session(nif_id)
        if session.dirty and not save and not _force:
            self._pending_detach = nif_id
            self._show_detach_dialog = True
            return

        if save and session.dirty:
            self._save_session(nif_id)

        # Detach grandchildren first
        for child in self.registry.get_children(nif_id):
            self.detach_nif(child.nif_id, save=save, _force=True)

        # Remove AttachmentNode from parent scene graph
        if session.attachment_node and session.parent_nif_id:
            try:
                parent = self.registry.get_session(session.parent_nif_id)
                parent.scene_root.children = [
                    c
                    for c in parent.scene_root.children
                    if c is not session.attachment_node
                ]
            except KeyError:
                pass
            # Also remove the SF-native mesh copies (if attach_nif loaded
            # the child through SfBackend on a Starfield session). Without
            # this, the meshes stay in sf_scene.meshes after detach and
            # keep rendering.
            sf_meshes = getattr(session.attachment_node, "_sf_meshes", None)
            if sf_meshes:
                try:
                    from creation_lib.renderer.backends.sf_backend import SfBackend

                    if isinstance(self.renderer.backend, SfBackend):
                        self.renderer.backend.detach_meshes(sf_meshes)
                except Exception:
                    _log.exception("detach_nif: SfBackend.detach_meshes failed")

        # Clean up undo stack
        self.undo_manager.filter_nif(nif_id)

        self.nif_watcher.unwatch_session(session.file_path, registry=self.registry)

        # Remove from registry
        self.registry.remove_session(nif_id)
        self._refresh_asset_watchers()

        # Rebuild
        if self.renderer and self.renderer.scene_root:
            from creation_lib.renderer.nif_loader import _update_world_transforms

            _update_world_transforms(self.renderer.scene_root, glm.mat4(1.0))
        self._rebuild_selection_bounds()
        self.connect_points._needs_rebuild = True
        _log.info("Detached NIF: %s", nif_id)

    def _undo(self):
        if self.undo_manager.can_undo:
            # Peek at which NIF the top action belongs to before popping
            nif_id = (
                self.undo_manager._undo_stack[-1][0]
                if self.undo_manager._undo_stack
                else None
            )
            self.undo_manager.undo()
            if nif_id and nif_id in self.registry.sessions:
                self.rebuild_scene_from_nif(nif_id)
            else:
                self.rebuild_scene_from_nif()
            self.status_text = "Undo"

    def _redo(self):
        if self.undo_manager.can_redo:
            nif_id = (
                self.undo_manager._redo_stack[-1][0]
                if self.undo_manager._redo_stack
                else None
            )
            self.undo_manager.redo()
            if nif_id and nif_id in self.registry.sessions:
                self.rebuild_scene_from_nif(nif_id)
            else:
                self.rebuild_scene_from_nif()
            self.status_text = "Redo"

    def close_nif(self):
        """Close the active NIF session and reset app to blank state."""
        self.registry.clear()
        if self.renderer:
            self.renderer.scene_root = None
            self.renderer.clear_alt_vao_cache()
        self.selection_mgr.clear()
        self.undo_manager.clear()
        self.nif_watcher.stop_watching()
        texture_watcher = getattr(self, "texture_watcher", None)
        if texture_watcher:
            texture_watcher.stop()
        material_watcher = getattr(self, "material_watcher", None)
        if material_watcher:
            material_watcher.stop()
        self._material_watch_nif_ids = {}
        self._nif_reload_pending = None
        self._loading = False
        self._loading_future = None
        self._branch_paste_busy = False
        self._branch_paste_pending = None
        self._branch_paste_label = ""
        self.status_text = ""

    def _open_file_dialog(self):
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file

            filepath = pick_file(
                "Open NIF File",
                NIF_LIKE_FILETYPES,
            )
            if filepath:
                self.load_nif(filepath)
        except Exception:
            pass

    def _load_settings(self):
        if SETTINGS_PATH.exists():
            try:
                with open(SETTINGS_PATH) as f:
                    s = json.load(f)
                self.camera.nav_style = s.get("nav_style", "3dsmax")
                self.camera.fov = s.get("fov", 45.0)
                from creation_lib.renderer.nif_importer import ImportOptions

                self.import_options = ImportOptions.from_dict(
                    s.get("import_options", {})
                )
                self._saved_panel_visibility = s.get("panel_visibility", {})
                # Render toggles — stashed until setup() builds self.renderer
                toggles = s.get("render_toggles", {})
                self._pending_render_toggles = {
                    "shadows": toggles.get("shadows", True),
                    "ssao": toggles.get("ssao", False),
                    "show_vertices": toggles.get("show_vertices", False),
                }
                self._show_collision_saved = s.get("show_collision", False)
                self._show_lights_saved = s.get("show_lights", True)
            except Exception:
                pass

    def _load_raw_editor_settings(self) -> dict:
        """Read editor_settings.json; returns {} on any error. Used for standalone fallback."""
        try:
            if SETTINGS_PATH.exists():
                with open(SETTINGS_PATH) as f:
                    return json.load(f)
        except Exception:
            pass
        return {}

    def _save_settings(self):
        # Clean up BA2 manager
        if self.ba2_manager:
            self.ba2_manager.close_all()

        # Load existing settings to avoid clobbering keys from SettingsPanel._persist()
        existing = {}
        if SETTINGS_PATH.exists():
            try:
                with open(SETTINGS_PATH) as f:
                    existing = json.load(f)
            except Exception:
                pass
        existing.pop("extra_paths", None)

        # Build panel_visibility snapshot (only if panels have been initialized)
        panel_visibility = existing.get("panel_visibility", {})
        if self._panels_initialized:
            panel_visibility = {
                "validation": self.validation._visible,
                "skeleton_ops": self.skeleton_ops._visible,
                "animation_editor": self.animation_editor._visible,
            }

        existing.update(
            {
                "nav_style": self.camera.nav_style,
                "fov": self.camera.fov,
                "import_options": self.import_options.to_dict(),
                "panel_visibility": panel_visibility,
                "render_toggles": (
                    {
                        "shadows": self.renderer.toggles.shadows,
                        "ssao": self.renderer.toggles.ssao,
                        "show_vertices": self.renderer.toggles.show_vertices,
                    }
                    if getattr(self, "renderer", None) is not None
                    else {
                        "shadows": self._pending_render_toggles["shadows"],
                        "ssao": self._pending_render_toggles["ssao"],
                        "show_vertices": self._pending_render_toggles["show_vertices"],
                    }
                ),
                "show_collision": (
                    self.renderer._show_collision
                    if self.renderer
                    else self._show_collision_saved
                ),
                "show_lights": self.light_display.visible,
            }
        )
        try:
            with open(SETTINGS_PATH, "w") as f:
                json.dump(existing, f, indent=2)
        except Exception:
            pass


def _get_owner_world_pos(nif, owner_id: int) -> tuple:
    """Return world position of a NIF node (used for CP gizmo writeback)."""
    parent_map = {}
    for b in nif.blocks:
        if not nif.schema.is_subtype_of(b.type_name, "NiNode"):
            continue
        for ref in b.get_field("Children") or []:
            rid = (
                int(ref) if isinstance(ref, (int, float)) else int(ref.get("value", -1))
            )
            if rid >= 0:
                parent_map[rid] = b.block_id
    from creation_lib.renderer.overlays.connect_point import _compute_world_position

    return _compute_world_position(nif, owner_id, parent_map)


class _BufferLogHandler(logging.Handler):
    """Log handler that forwards messages to hello_imgui's log widget."""

    _LEVEL_MAP = None  # Lazy-init to avoid import-time hello_imgui access

    def __init__(self):
        super().__init__()
        self.setFormatter(logging.Formatter("[%(name)s] %(message)s"))

    def emit(self, record):
        try:
            if _BufferLogHandler._LEVEL_MAP is None:
                _BufferLogHandler._LEVEL_MAP = {
                    logging.DEBUG: hello_imgui.LogLevel.debug,
                    logging.INFO: hello_imgui.LogLevel.info,
                    logging.WARNING: hello_imgui.LogLevel.warning,
                    logging.ERROR: hello_imgui.LogLevel.error,
                    logging.CRITICAL: hello_imgui.LogLevel.error,
                }
            msg = self.format(record)
            level = self._LEVEL_MAP.get(record.levelno, hello_imgui.LogLevel.info)
            hello_imgui.log(level, msg)
        except Exception:
            pass


# The NIF editor runs only as part of the toolkit.
# Launch via: uv run python -m ui.toolkit
