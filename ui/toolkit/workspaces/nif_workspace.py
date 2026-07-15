"""NIF Editor workspace — wraps ui.editor.app.NifEditorApp for the toolkit."""

from __future__ import annotations

import logging

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from ui.editor.docking_layout import get_nif_dockable_windows
from ui.editor.particles.runtime import PARTICLE_PREVIEW_SEQUENCE
from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.nif")

# Panel name suffix
_NS = "##nif"

# Map of panel attr -> namespaced window name
_PANEL_NAMES = {
    "scene_tree": f"Scene Tree{_NS}",
    "nif_file_browser": f"File Browser{_NS}",
    "properties": f"Properties{_NS}",
    "texture_editor": f"Texture Set Editor{_NS}",
    "skeleton_ops": f"Skeleton Tools{_NS}",
    "validation": f"Validation{_NS}",
    "batch_operations": f"Batch Operations{_NS}",
    "animation_editor": f"Animation Editor{_NS}",
    "particle_systems": f"Particle Systems{_NS}",
}


class _PlaybackStatus:
    def __init__(self, is_playing: bool):
        self.is_playing = is_playing


class NifWorkspace(BaseWorkspace):
    """Workspace wrapper for the NIF editor."""

    name = "NIF Editor"
    icon = "NIF"
    id = "nif"

    def get_user_guide(self):
        from ui.editor.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide("NIF Editor User Guide", USER_GUIDE_MARKDOWN, "nif_user_guide")

    def get_dockable_windows(self) -> list[hello_imgui.DockableWindow]:
        windows = get_nif_dockable_windows()
        windows.append(make_window(f"Help{_NS}", "RightDock", is_visible=False))
        return windows

    def get_required_addons(self) -> dict:
        return {"with_implot": True}  # Animation editor uses ImPlot

    def initialize(self) -> None:
        from ui.editor.app import NifEditorApp
        from ui.editor.panels.help_panel import HelpPanel

        self._help_panel = HelpPanel()
        self._app = NifEditorApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False

        # Override panel window names with ##nif suffix
        self._app._viewport_label = f"Viewport{_NS}"
        for attr, name in _PANEL_NAMES.items():
            panel = getattr(self._app, attr, None)
            if panel:
                panel.window_name = name

        # Bind panel draw methods to DockableWindows so hello_imgui's docking
        # loop calls them — this ensures DockBuilder tab placement works.
        self._bind_dockable_windows()

        # Apply any settings that arrived before initialization
        if self._pending_settings:
            self._apply_saved_settings(self._pending_settings)
            self._pending_settings = None

        self._initialized = True
        _log.info("NIF workspace initialized")

    def _apply_saved_settings(self, settings: dict) -> None:
        """Apply settings to the live app instance."""
        if "nav_style" in settings:
            self._app.camera.nav_style = settings["nav_style"]
        if "fov" in settings:
            self._app.camera.fov = settings["fov"]

    @staticmethod
    def _force_visible_draw(panel):
        """Wrap a legacy panel's draw so its internal _visible flag stays True.

        Some panels (skeleton_ops, validation) gate rendering on _visible and
        set it False when the user clicks X.  In the toolkit, DockableWindow
        controls visibility — the panel should always render when called.
        """
        def _draw():
            panel._visible = True
            panel.draw()
        return _draw

    def _bind_dockable_windows(self):
        """Wire panel draw methods into DockableWindow gui_functions."""
        panel_map = {
            f"Viewport{_NS}": self._app._draw_viewport,
            f"Scene Tree{_NS}": self._app.scene_tree.draw,
            f"File Browser{_NS}": self._app.nif_file_browser.draw,
            f"Properties{_NS}": self._app.properties.draw,
            f"Texture Set Editor{_NS}": self._app.texture_editor.draw,
            f"Skeleton Tools{_NS}": self._force_visible_draw(self._app.skeleton_ops),
            f"Validation{_NS}": self._force_visible_draw(self._app.validation),
            f"Animation Editor{_NS}": self._force_visible_draw(
                self._app.animation_editor
            ),
            f"Particle Systems{_NS}": self._force_visible_draw(
                self._app.particle_systems
            ),
            f"Batch Operations{_NS}": self._app.batch_operations.draw,
            f"Help{_NS}": self._help_panel.draw,
        }
        self._bind_panels(panel_map)
        _log.info("Bound %d panel draw methods to DockableWindows", len(panel_map))

    def draw_menu(self) -> None:
        if self._app is not None and not hasattr(self._app, "toolbar"):
            self._app._init_panels()
        self._app.toolbar.draw_menu_items(include_help=False)
        self._app.toolbar._render_import_options_modal()
        if self._view_helper:
            self._view_helper.draw([
                "Viewport##nif",
                "Scene Tree##nif",
                "File Browser##nif",
                "Properties##nif",
                "Texture Set Editor##nif",
                "Skeleton Tools##nif",
                "Validation##nif",
                "Batch Operations##nif",
                "Animation Editor##nif",
                "Particle Systems##nif",
                f"Help{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        from imgui_bundle import imgui, icons_fontawesome_6 as fa
        from creation_lib.renderer.render_modes import RenderMode

        def _btn(icon):
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        def _sep():
            imgui.same_line()
            imgui.text("|")
            imgui.same_line()

        def _active_btn(label: str, active: bool) -> bool:
            if active:
                imgui.push_style_color(
                    imgui.Col_.button, imgui.ImVec4(0.2, 0.4, 0.7, 1.0)
                )
            clicked = _btn(label)
            if active:
                imgui.pop_style_color()
            return clicked

        def _icon(name: str, fallback: str) -> str:
            return getattr(fa, name, fallback)

        if self._app is None:
            return
        has_nif = bool(self._app.registry.sessions)
        is_multi = bool(self._app.registry.has_multiple_nifs)
        save_label = "Save All" if is_multi else "Save"
        save_as_label = "Save All As" if is_multi else "Save As"

        # Save / Save All
        if not has_nif:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_FLOPPY_DISK):
            if is_multi:
                self._app._save_all()
            else:
                self._app._save()
        if not has_nif:
            imgui.end_disabled()
        imgui.set_item_tooltip(f"{save_label} (Ctrl+S)")

        imgui.same_line()

        # Save As / Save All As
        if not has_nif:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_FILE_EXPORT):
            if is_multi:
                self._app.toolbar._save_all_as()
            else:
                self._app.toolbar._save_as()
        if not has_nif:
            imgui.end_disabled()
        if is_multi:
            imgui.set_item_tooltip("Save All As (Ctrl+Shift+S)")
        else:
            imgui.set_item_tooltip("Save As (Ctrl+Shift+S)")

        imgui.same_line()

        # Open
        if _btn(fa.ICON_FA_FOLDER_OPEN):
            self._app._open_file_dialog()
        imgui.set_item_tooltip("Open")

        imgui.same_line()

        # Undo
        can_undo = self._app.undo_manager.can_undo
        if not can_undo:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_ROTATE_LEFT):
            self._app._undo()
        if self._app.undo_manager.undo_description:
            imgui.set_item_tooltip(f"Undo: {self._app.undo_manager.undo_description}")
        else:
            imgui.set_item_tooltip("Undo")
        if not can_undo:
            imgui.end_disabled()

        imgui.same_line()

        # Redo
        can_redo = self._app.undo_manager.can_redo
        if not can_redo:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_ROTATE_RIGHT):
            self._app._redo()
        if self._app.undo_manager.redo_description:
            imgui.set_item_tooltip(f"Redo: {self._app.undo_manager.redo_description}")
        else:
            imgui.set_item_tooltip("Redo")
        if not can_redo:
            imgui.end_disabled()

        _sep()

        lighting_on = bool(getattr(self._app, "_toggle_lighting", True))
        lighting_icon = (
            _icon("ICON_FA_LIGHTBULB", "Lit")
            if lighting_on else
            _icon("ICON_FA_MOON", "Unlit")
        )
        if _active_btn(f"{lighting_icon}##lighting", lighting_on):
            self._app._toggle_lighting = not lighting_on
        imgui.set_item_tooltip("Toggle lighting")

        imgui.same_line()

        rm = getattr(self._app, "render_mode_mgr", None)
        current_mode = rm.mode if rm else None

        if rm is None:
            imgui.begin_disabled()
        if _active_btn(f"{_icon('ICON_FA_CUBE', 'WF')}##wireframe", current_mode == RenderMode.WIREFRAME):
            rm.set_mode(
                RenderMode.TEXTURED
                if current_mode == RenderMode.WIREFRAME
                else RenderMode.WIREFRAME
            )
        if rm is None:
            imgui.end_disabled()
        imgui.set_item_tooltip("Toggle wireframe")

        imgui.same_line()

        if rm is None:
            imgui.begin_disabled()
        if _active_btn("UV##uv", current_mode == RenderMode.UV_CHECKER):
            rm.set_mode(
                RenderMode.TEXTURED
                if current_mode == RenderMode.UV_CHECKER
                else RenderMode.UV_CHECKER
            )
        if rm is None:
            imgui.end_disabled()
        imgui.set_item_tooltip("Toggle UV checker")

        imgui.same_line()

        if rm is None:
            imgui.begin_disabled()
        if _active_btn(f"{_icon('ICON_FA_COMPASS', 'N')}##normals", current_mode == RenderMode.NORMALS):
            rm.set_mode(
                RenderMode.TEXTURED
                if current_mode == RenderMode.NORMALS
                else RenderMode.NORMALS
            )
        if rm is None:
            imgui.end_disabled()
        imgui.set_item_tooltip("Toggle normals view")

        _sep()

        if _btn("L##cam_left"):
            self._app.camera.set_left()
        imgui.set_item_tooltip("Left view")

        imgui.same_line()

        if _btn("R##cam_right"):
            self._app.camera.set_right()
        imgui.set_item_tooltip("Right view")

        imgui.same_line()

        if _btn("F##cam_front"):
            self._app.camera.set_front()
        imgui.set_item_tooltip("Front view")

        imgui.same_line()

        if _btn("B##cam_back"):
            self._app.camera.set_back()
        imgui.set_item_tooltip("Back view")

        imgui.same_line()

        if _btn("T##cam_top"):
            self._app.camera.set_top()
        imgui.set_item_tooltip("Top view")

        imgui.same_line()

        has_scene = bool(self._app and self._app.nif_root)
        if not has_scene:
            imgui.begin_disabled()
        if _btn("A##cam_frame_all"):
            self._frame_all()
        if not has_scene:
            imgui.end_disabled()
        imgui.set_item_tooltip("Frame all")

        imgui.same_line()

        has_selection = bool(
            self._app
            and getattr(self._app.selection_mgr, "selected", None)
            and getattr(self._app.selection_mgr.selected, "bound_radius", 0.0) > 0
        )
        if not has_selection:
            imgui.begin_disabled()
        if _btn("S##cam_frame_selected"):
            self._frame_selected()
        if not has_selection:
            imgui.end_disabled()
        imgui.set_item_tooltip("Frame selected")

        _sep()

        sp = getattr(self._app, "settings_panel", None)
        grid_on = bool(sp and sp._grid_visible)
        if sp is None:
            imgui.begin_disabled()
        if _active_btn(f"{_icon('ICON_FA_BORDER_ALL', 'Grid')}##grid", grid_on):
            sp._grid_visible = not sp._grid_visible
            sp._toggle_grid(sp._grid_visible)
            sp._persist()
        if sp is None:
            imgui.end_disabled()
        imgui.set_item_tooltip("Toggle grid")

        imgui.same_line()

        renderer = getattr(self._app, "renderer", None)
        collision_on = bool(renderer and renderer._show_collision)
        if renderer is None:
            imgui.begin_disabled()
        if _active_btn(f"{_icon('ICON_FA_DRAW_POLYGON', 'Col')}##collision", collision_on):
            renderer._show_collision = not renderer._show_collision
            renderer._collision_dirty = True
        if renderer is None:
            imgui.end_disabled()
        imgui.set_item_tooltip("Toggle collision")

        imgui.same_line()

        overlay = getattr(self._app, "controls_overlay", None)
        overlay_on = bool(overlay and overlay.visible)
        if overlay is None:
            imgui.begin_disabled()
        if _active_btn(f"{_icon('ICON_FA_KEYBOARD', 'HUD')}##controls_overlay", overlay_on):
            overlay.visible = not overlay.visible
        if overlay is None:
            imgui.end_disabled()
        imgui.set_item_tooltip("Toggle controls overlay")

        imgui.same_line()

        if _btn(f"{_icon('ICON_FA_SUN', 'Light')}##reset_light"):
            self._reset_light()
        imgui.set_item_tooltip("Reset light position")

        seq_map = self._get_animation_sequences()
        seq_names = list(seq_map.keys())
        if seq_names:
            _sep()
            mgr = self._get_active_anim_mgr()
            current_seq = self._get_toolbar_current_sequence(seq_names, mgr)
            current_idx = seq_names.index(current_seq) if current_seq in seq_names else 0

            is_playing, is_paused = self._get_toolbar_playback_state(current_seq, mgr)
            play_icon = _icon("ICON_FA_PAUSE", "||") if is_playing else _icon("ICON_FA_PLAY", ">")
            if _btn(f"{play_icon}##anim_play_pause"):
                if is_playing:
                    coord = getattr(self._app, "anim_coordinator", None)
                    if coord:
                        coord.pause()
                    elif mgr:
                        mgr.pause()
                elif is_paused:
                    coord = getattr(self._app, "anim_coordinator", None)
                    if coord:
                        coord.resume()
                    elif mgr:
                        mgr.resume()
                else:
                    sequence_name = seq_names[current_idx]
                    coord = getattr(self._app, "anim_coordinator", None)
                    if coord:
                        coord.play(sequence_name)
                    elif mgr:
                        mgr.play(sequence_name)
            imgui.set_item_tooltip("Play / pause animation")

            imgui.same_line()

            loop_on = bool(mgr and mgr.loop)
            if _active_btn(f"{_icon('ICON_FA_ROTATE', 'Loop')}##anim_loop", loop_on):
                self._set_animation_loop(not loop_on)
            imgui.set_item_tooltip("Toggle animation loop")

            imgui.same_line()
            imgui.set_next_item_width(220)
            changed, new_idx = imgui.combo("##anim_sequence", current_idx, seq_names)
            if changed and 0 <= new_idx < len(seq_names):
                self._select_animation_sequence(seq_names[new_idx])

    def _toggle_help_panel(self):
        dp = hello_imgui.get_runner_params().docking_params
        for w in dp.dockable_windows:
            if w.label == f"Help{_NS}":
                w.is_visible = not w.is_visible
                break

    def toggle_user_guide(self) -> None:
        self._toggle_help_panel()

    def _frame_all(self) -> None:
        if not self._app or not self._app.nif_root:
            return
        center, radius = self._app._aggregate_bounds(self._app.nif_root)
        if radius > 0:
            self._app.camera.frame_on_bounds(center, radius)

    def _frame_selected(self) -> None:
        if not self._app:
            return
        node = getattr(self._app.selection_mgr, "selected", None)
        if node is None or getattr(node, "bound_radius", 0.0) <= 0:
            return
        self._app.camera.frame_on_bounds(node.bound_center, node.bound_radius)

    def _reset_light(self) -> None:
        if not self._app:
            return
        lighting = getattr(self._app, "lighting", None)
        if lighting:
            lighting.key_heading = 30.0
            lighting.key_pitch = -55.0
            lighting._update_directions()

    def _get_active_anim_mgr(self):
        if not self._app:
            return None
        registry = getattr(self._app, "registry", None)
        if registry:
            try:
                return registry.active_session.anim_manager
            except (KeyError, AttributeError):
                return None
        return getattr(self._app, "animation_mgr", None)

    def _get_animation_sequences(self) -> dict[str, list[str]]:
        if not self._app:
            return {}
        coord = getattr(self._app, "anim_coordinator", None)
        if coord:
            return coord.get_all_sequences()
        mgr = self._get_active_anim_mgr()
        if mgr:
            return {name: ["main"] for name in mgr.get_sequences()}
        return {}

    def _get_toolbar_current_sequence(self, seq_names: list[str], mgr) -> str:
        if not seq_names:
            return ""
        coord = getattr(self._app, "anim_coordinator", None) if self._app else None
        coord_sequence = getattr(coord, "current_sequence_name", None)
        if coord_sequence in seq_names:
            return coord_sequence
        current_sequence = getattr(mgr, "current_sequence", None)
        current_name = getattr(current_sequence, "name", None)
        if current_name in seq_names:
            return current_name
        return seq_names[0]

    def _get_particle_runtimes(self):
        if not self._app:
            return []
        registry = getattr(self._app, "registry", None)
        if registry is None:
            return []
        try:
            sessions = registry.all_sessions()
        except (KeyError, AttributeError):
            return []
        runtimes = []
        for session in sessions:
            runtime = getattr(session, "particle_runtime", None)
            if runtime is not None and getattr(runtime, "has_particles", False):
                runtimes.append(runtime)
        return runtimes

    def _get_toolbar_playback_state(self, sequence_name: str, mgr) -> tuple[bool, bool]:
        if sequence_name == PARTICLE_PREVIEW_SEQUENCE:
            runtimes = self._get_particle_runtimes()
            if any(bool(getattr(runtime, "is_paused", False)) for runtime in runtimes):
                return False, True
            if any(bool(getattr(runtime, "is_playing", False)) for runtime in runtimes):
                return True, False
            if runtimes:
                return False, False
        return (
            bool(mgr and getattr(mgr, "is_playing", False)),
            bool(
                mgr
                and getattr(mgr, "is_paused", False)
                and getattr(mgr, "current_sequence", None) is not None
            ),
        )

    def _select_animation_sequence(self, sequence_name: str) -> None:
        if not self._app:
            return
        coord = getattr(self._app, "anim_coordinator", None)
        if coord:
            coord.select(sequence_name)
            return
        registry = getattr(self._app, "registry", None)
        if registry:
            for session in registry.all_sessions():
                if (
                    sequence_name == PARTICLE_PREVIEW_SEQUENCE
                    or session.anim_manager.has_sequence(sequence_name)
                ):
                    session.anim_manager.select_sequence(sequence_name)
        else:
            mgr = self._get_active_anim_mgr()
            if mgr:
                mgr.select_sequence(sequence_name)

    def _set_animation_loop(self, enabled: bool) -> None:
        if not self._app:
            return
        registry = getattr(self._app, "registry", None)
        if registry:
            for session in registry.all_sessions():
                session.anim_manager.loop = enabled
        else:
            mgr = self._get_active_anim_mgr()
            if mgr:
                mgr.loop = enabled

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return

        io = imgui.get_io()
        if imgui.is_key_pressed(imgui.Key.f1) and not io.want_text_input:
            self._toggle_help_panel()

        # Drain completed background NIF loads on UI thread
        self._app._poll_loading()
        self._app._poll_attaching()
        poll_branch_paste = getattr(self._app, "_poll_branch_paste_into_new", None)
        if callable(poll_branch_paste):
            poll_branch_paste()

        # Process keybindings (already guarded by active check)
        self._app._process_keybindings()

        # Animation update
        from imgui_bundle import hello_imgui
        io = imgui.get_io()
        anim_mgr = self._update_animation_frame(io.delta_time)
        if anim_mgr:
            # Force continuous rendering while animation is playing
            if anim_mgr.is_playing:
                hello_imgui.get_runner_params().fps_idling.enable_idling = False
            elif not hello_imgui.get_runner_params().fps_idling.enable_idling:
                hello_imgui.get_runner_params().fps_idling.enable_idling = True

        # Docked panels (Viewport, Scene Tree, Properties, etc.) are drawn
        # by hello_imgui via DockableWindow gui_functions bound in
        # _bind_dockable_windows(). Do NOT draw them here.

        # Floating / non-docked panels only
        self._app.controls_overlay.draw()
        if self._app.show_command_palette:
            self._app.command_palette.draw()
        # Generate Collision modal (opened from scene tree or toolbar).
        coll_dlg = getattr(self._app, "collision_dialog", None)
        if coll_dlg is not None:
            coll_dlg.draw()
        particle_builder = getattr(self._app, "particle_builder_popup", None)
        if particle_builder is not None:
            particle_builder.draw()

        # NIF browser modal — pass current connect points, render, drain signals
        if hasattr(self._app, '_nif_browser'):
            browser = self._app._nif_browser
            # Pass current NIF's connect points to the browser
            cp_names = []
            if self._app.nif_file:
                for block in self._app.nif_file.blocks:
                    if block.type_name == "BSConnectPoint::Children":
                        names = block.get_field("Point Name") or []
                        if isinstance(names, str):
                            names = [names]
                        cp_names.extend(str(n) for n in names)
            browser.set_connect_points(cp_names)
            browser.render()
            # Drain output signals
            if browser.open_path:
                path = browser.open_path
                browser.open_path = None
                self._app.load_nif(path)
            elif browser.attach_info:
                path, cp_name = browser.attach_info
                browser.attach_info = None
                self._app.attach_nif(path, "main", cp_name)
            elif browser.bash_path:
                path = browser.bash_path
                browser.bash_path = None
                self._app.bash_nif(path)
            elif browser.addn_open_path:
                path = browser.addn_open_path
                browser.addn_open_path = None
                self._app._open_nif_read_only(path)

        # Connect point / light display rebuilds
        self._app._rebuild_connect_points()

        if (self._app.light_display._needs_rebuild
                and self._app.nif and self._app.renderer and self._app.ctx):
            cp_prog = self._app.renderer.programs.get("connect_point")
            if cp_prog:
                self._app.light_display.rebuild(self._app.nif, self._app.ctx, cp_prog)
                self._app.light_display._needs_rebuild = False
                self._app.lighting.point_lights = self._app.light_display.point_lights
                self._app.selection_mgr.register_extra_nodes(self._app.light_display.light_nodes)

        # NIF file watcher — prompt or auto-reload based on settings
        reload_path = self._app.nif_watcher.check_reload()
        if reload_path:
            sp = getattr(self._app, 'settings_panel', None)
            if sp and sp.nif_reload_prompt:
                if self._app._nif_reload_pending is None:
                    self._app._nif_reload_pending = reload_path
            else:
                self._app.load_nif(reload_path)

        self._app._handle_material_reloads()
        self._app._handle_texture_reloads()

        # NIF reload dialog (modal)
        self._app._draw_nif_reload_dialog()

    def _update_animation_frame(self, delta_time: float):
        coord = getattr(self._app, "anim_coordinator", None)
        if coord is None:
            anim_mgr = getattr(self._app, "animation_mgr", None)
            if anim_mgr is None:
                return None

            renderer = getattr(self._app, "renderer", None)
            scene_root = getattr(renderer, "scene_root", None) or self._app.nif_root
            anim_mgr.update(delta_time, scene_root)
            if getattr(anim_mgr, "_dirty", False) and scene_root:
                import glm
                from creation_lib.renderer.nif_loader import _update_world_transforms

                _update_world_transforms(scene_root, glm.mat4(1.0))
                if renderer is not None:
                    renderer._collision_dirty = True
            return _PlaybackStatus(bool(getattr(anim_mgr, "is_playing", False)))

        coord.update(delta_time)
        registry = getattr(self._app, "registry", None)
        all_sessions = getattr(registry, "all_sessions", None)
        if callable(all_sessions):
            sessions = all_sessions()
        else:
            active_session = getattr(registry, "active_session", None)
            sessions = [active_session] if active_session is not None else []

        any_playing = False
        any_dirty = False
        for session in sessions:
            anim_mgr = getattr(session, "anim_manager", None)
            runtime = getattr(session, "particle_runtime", None)
            runtime_playing = bool(getattr(runtime, "is_playing", False)) and not bool(
                getattr(runtime, "is_paused", False)
            )
            any_playing = (
                any_playing
                or bool(getattr(anim_mgr, "is_playing", False))
                or runtime_playing
            )
            any_dirty = any_dirty or bool(getattr(anim_mgr, "_dirty", False))

        renderer = getattr(self._app, "renderer", None)
        scene_root = getattr(renderer, "scene_root", None) or self._app.nif_root
        if any_dirty and scene_root:
            import glm
            from creation_lib.renderer.nif_loader import _update_world_transforms

            _update_world_transforms(scene_root, glm.mat4(1.0))
            if renderer is not None:
                renderer._collision_dirty = True
        return _PlaybackStatus(any_playing)

    def on_activate(self) -> None:
        from imgui_bundle import hello_imgui
        hello_imgui.get_runner_params().fps_idling.fps_idle = 10.0
        self.active = True
        if self._app:
            self._app.active = True
            # Resume animation if it was playing
            if self._app.animation_mgr:
                self._app.animation_mgr.resume()
        _log.info("NIF workspace activated")

    def on_deactivate(self) -> None:
        self.active = False
        if self._app:
            self._app.active = False
            # Pause animation playback while inactive
            if self._app.animation_mgr:
                self._app.animation_mgr.pause()
        _log.info("NIF workspace deactivated")

    def cleanup(self) -> None:
        if self._app:
            self._app._save_settings()

    def draw_settings(self) -> None:
        """Draw NIF editor settings (controls, appearance) in the global Settings window."""
        from imgui_bundle import imgui

        sp = getattr(self._app, 'settings_panel', None) if self._app else None
        if not sp:
            imgui.text("NIF editor not initialized.")
            return

        # -- Controls --
        imgui.text("Controls")
        imgui.separator()
        imgui.spacing()

        changed, sp._nav_style_idx = imgui.combo(
            "Navigation Style", sp._nav_style_idx, sp._NAV_LABELS
        )
        if changed:
            key = sp._NAV_KEYS[sp._nav_style_idx]
            if hasattr(self._app, 'camera'):
                self._app.camera.set_nav_style(key)
            sp._persist()

        # Quick reference
        nav = sp._NAV_KEYS[sp._nav_style_idx]
        imgui.spacing()
        imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0), "Quick Reference:")
        if nav == "3dsmax":
            sp._hint("Alt + MMB Drag", "Orbit")
            sp._hint("MMB Drag", "Pan")
            sp._hint("Scroll", "Zoom")
        elif nav == "blender":
            sp._hint("MMB Drag", "Orbit")
            sp._hint("Shift + MMB Drag", "Pan")
            sp._hint("Scroll", "Zoom")
        else:
            sp._hint("Ctrl + LMB Drag", "Orbit")
            sp._hint("MMB Drag", "Pan")
            sp._hint("Scroll", "Zoom")
        sp._hint("LMB Click", "Select")
        sp._hint("RMB Click", "Context Menu")
        sp._hint("Alt + LMB Drag", "Rotate Light")
        sp._hint("W", "Move Gizmo")

        imgui.spacing()
        imgui.spacing()

        # -- Appearance --
        imgui.text("Appearance")
        imgui.separator()
        imgui.spacing()

        col = imgui.ImVec4(sp._bg_color[0], sp._bg_color[1], sp._bg_color[2], 1.0)
        changed, col = imgui.color_edit3("Background", col)
        if changed:
            sp._bg_color = [col.x, col.y, col.z]
            renderer = getattr(self._app, 'renderer', None)
            if renderer is not None:
                renderer.bg_color = tuple(sp._bg_color)
            sp._persist()

        changed, sp._grid_visible = imgui.checkbox("Show Grid", sp._grid_visible)
        if changed:
            sp._toggle_grid(sp._grid_visible)
            sp._persist()

        changed, sp._fps_visible = imgui.checkbox("Show FPS Counter", sp._fps_visible)
        if changed:
            sp._persist()

        imgui.spacing()
        changed, sp._outline_style_idx = imgui.combo(
            "Selection Outline", sp._outline_style_idx, sp._OUTLINE_LABELS
        )
        if changed:
            sp._persist()

        if sp.outline_style != "none":
            col = imgui.ImVec4(sp._outline_color[0], sp._outline_color[1], sp._outline_color[2], 1.0)
            changed, col = imgui.color_edit3("Outline Color", col)
            if changed:
                sp._outline_color = [col.x, col.y, col.z]
                sp._persist()

        imgui.spacing()
        imgui.spacing()

        # -- File Watching --
        imgui.text("File Watching")
        imgui.separator()
        imgui.spacing()

        changed, sp._nif_reload_prompt = imgui.checkbox(
            "Prompt before reloading changed NIF files", sp._nif_reload_prompt
        )
        if changed:
            sp._persist()
        imgui.text_colored(
            imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
            "  When disabled, NIF files reload automatically.",
        )

    def get_settings_defaults(self) -> dict:
        return {
            "nav_style": "3dsmax",
            "fov": 45.0,
            "bg_color": [0.18, 0.18, 0.20],
            "grid_visible": True,
            "panel_visibility": {
                "animation": True,
                "validation": False,
                "skeleton_ops": False,
            },
        }

    def apply_settings(self, settings: dict) -> None:
        if self._app:
            self._apply_saved_settings(settings)
        else:
            self._pending_settings = settings

    def collect_settings(self) -> dict:
        if not self._app:
            return self._pending_settings or {}
        s = {
            "nav_style": self._app.camera.nav_style,
            "fov": self._app.camera.fov,
        }
        if self._app._panels_initialized:
            s["panel_visibility"] = {
                "validation": self._app.validation._visible,
                "skeleton_ops": self._app.skeleton_ops._visible,
            }
        return s

    def open_file(self, path: str) -> None:
        self._app.load_nif(path)

    def handle_file_drop(
        self,
        paths: list[str],
        *,
        x: float | None = None,
        y: float | None = None,
    ) -> bool:
        if self._app is None:
            return False
        return self._app.handle_file_drop(paths, x=x, y=y)
