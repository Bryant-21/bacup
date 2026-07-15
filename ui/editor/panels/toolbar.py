"""Toolbar panel — imgui menu bar for File/View/Tools operations."""

import os

from imgui_bundle import hello_imgui, imgui

from creation_lib.ui.widgets.pick_folder import pick_file, pick_folder, pick_save_file
from ui.editor.nif_file_types import NIF_LIKE_FILETYPES, is_nif_like_path


class ToolbarPanel:
    """imgui main menu bar with File/View/Tools menus."""

    def __init__(self, app):
        self.app = app
        self._show_import_options = False
        self._import_source_path = ""
        self._show_import_new_modal = False
        self._import_new_source_path = ""
        self._import_new_game_idx = 0

    def draw_menu_items(self, include_help: bool = True):
        """Draw menu items only — for toolkit mode where host owns the menu bar."""
        self._file_menu()
        self._edit_menu()
        self._view_menu()
        self._tools_menu()
        self._debug_menu()
        if include_help:
            self._help_menu()

        # Right-aligned info: [filename] [FPS]
        win_w = imgui.get_window_width()
        right_edge = win_w

        fps_visible = self._is_fps_visible()
        if fps_visible:
            io = imgui.get_io()
            fps_text = f"{io.framerate:.0f} FPS"
            fps_w = imgui.calc_text_size(fps_text).x + 16
            right_edge -= fps_w
            imgui.same_line(right_edge)
            fps = io.framerate
            if fps >= 50:
                color = imgui.ImVec4(0.4, 0.9, 0.4, 1.0)
            elif fps >= 25:
                color = imgui.ImVec4(0.9, 0.9, 0.4, 1.0)
            else:
                color = imgui.ImVec4(0.9, 0.4, 0.4, 1.0)
            imgui.text_colored(color, fps_text)

        if self.app.current_path:
            from pathlib import Path
            name = Path(self.app.current_path).name
            name_w = imgui.calc_text_size(name).x + 16
            right_edge -= name_w
            imgui.same_line(right_edge)
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0), name)


    def draw(self):
        """Draw the main menu bar (standalone mode)."""
        if imgui.begin_main_menu_bar():
            self.draw_menu_items()
            imgui.end_main_menu_bar()

        self._render_import_options_modal()
        self._render_import_new_modal()

    def _file_menu(self):
        if imgui.begin_menu("File"):
            if imgui.menu_item("New NIF", "", False)[0]:
                self.app.new_blank_nif()

            imgui.separator()

            if imgui.menu_item("Open...", "O", False)[0]:
                self.app._open_file_dialog()

            self._open_recent_submenu()

            imgui.separator()

            has_nif = bool(self.app.nif_file)
            has_multi = has_nif and self.app.registry.has_multiple_nifs

            if has_multi:
                if imgui.menu_item("Save All", "Ctrl+S", False)[0]:
                    self.app._save_all()
                if imgui.menu_item("Save All As...", "Ctrl+Shift+S", False)[0]:
                    self._save_all_as()
            else:
                if imgui.menu_item("Save", "Ctrl+S", False, has_nif)[0]:
                    self.app._save()
                if imgui.menu_item("Save As...", "Ctrl+Shift+S", False, has_nif)[0]:
                    self._save_as()

            imgui.separator()

            if imgui.menu_item("Close NIF", "", False, has_nif)[0]:
                self.app.close_nif()

            imgui.separator()

            if imgui.menu_item("Attach NIF...", "", False, bool(self.app.nif_file))[0]:
                self._attach_nif()

            if imgui.menu_item("Bash NIF...", "", False, bool(self.app.nif_file))[0]:
                self._bash_nif()

            if imgui.menu_item("Browse NIFs...", "", False)[0]:
                self._browse_nifs()

            imgui.separator()

            if imgui.begin_menu("Export", enabled=bool(self.app.nif_file)):
                if imgui.menu_item("Export NIF As...", "", False)[0]:
                    self._export_nif_as()
                if imgui.menu_item("Export Selection to OBJ...", "", False)[0]:
                    self._export_obj()
                if imgui.menu_item("Export Scene to FBX...", "", False)[0]:
                    self._export_fbx()
                imgui.end_menu()

            if imgui.begin_menu("Import"):
                if imgui.menu_item("New...", "", False, not has_nif)[0]:
                    self._import_new_start()
                imgui.separator()
                if imgui.menu_item("NIF (Merge)...", "", False, has_nif)[0]:
                    self._import_nif_start()
                imgui.separator()
                if imgui.menu_item("Into Selection...", "", False, has_nif)[0]:
                    self._import_into_selection()
                if imgui.menu_item("FBX as New Shapes...", "", False, has_nif)[0]:
                    self._import_fbx_scene()
                imgui.end_menu()

            imgui.separator()

            if imgui.menu_item("Quit", "Esc", False)[0]:
                import sys
                sys.exit()

            imgui.end_menu()

    def _open_recent_submenu(self):
        from pathlib import Path
        from ui.editor.recent_files import get_list, clear

        recents = get_list()
        enabled = bool(recents)
        if imgui.begin_menu("Open Recent", enabled):
            for i, path in enumerate(recents):
                label = f"{Path(path).name}##{i}"
                # Show truncated directory as tooltip hint in the label
                if imgui.menu_item(label, "", False)[0]:
                    self.app.load_nif(path)
                if imgui.is_item_hovered():
                    imgui.set_tooltip(path)

            if recents:
                imgui.separator()
                if imgui.menu_item("Clear Recent", "", False)[0]:
                    clear()

            imgui.end_menu()

    def _edit_menu(self):
        if imgui.begin_menu("Edit"):
            um = self.app.undo_manager

            undo_label = f"Undo: {um.undo_description}" if um.can_undo else "Undo"
            if imgui.menu_item(undo_label, "Ctrl+Z", False, um.can_undo)[0]:
                self.app._undo()

            redo_label = f"Redo: {um.redo_description}" if um.can_redo else "Redo"
            if imgui.menu_item(redo_label, "Ctrl+Shift+Z", False, um.can_redo)[0]:
                self.app._redo()

            imgui.end_menu()

    def _is_fps_visible(self) -> bool:
        """Check if FPS counter should be shown."""
        sp = getattr(self.app, 'settings_panel', None)
        if sp:
            return getattr(sp, '_fps_visible', True)
        return True


    def _view_menu(self):
        if imgui.begin_menu("View"):
            # Camera views
            if imgui.menu_item("Frame All", "F", False)[0]:
                if self.app.nif_root:
                    center, radius = self.app._aggregate_bounds(self.app.nif_root)
                    if radius > 0:
                        self.app.camera.frame_on_bounds(center, radius)

            if imgui.menu_item("Front View", "Numpad 1", False)[0]:
                self.app.camera.set_front()

            if imgui.menu_item("Side View", "Numpad 3", False)[0]:
                self.app.camera.set_side()

            if imgui.menu_item("Top View", "Numpad 7", False)[0]:
                self.app.camera.set_top()

            imgui.separator()

            # Render modes
            if hasattr(self.app, 'render_mode_mgr') and self.app.render_mode_mgr:
                from creation_lib.renderer.render_modes import RenderMode
                rm = self.app.render_mode_mgr
                for label, shortcut, mode in [
                    ("Textured", "1", RenderMode.TEXTURED),
                    ("Wireframe", "2", RenderMode.WIREFRAME),
                    ("Normals", "3", RenderMode.NORMALS),
                    ("UV Checker", "4", RenderMode.UV_CHECKER),
                    ("Unlit", "5", RenderMode.UNLIT),
                ]:
                    checked = rm.is_enabled(mode)
                    changed, val = imgui.checkbox(f"{label} ({shortcut})", checked)
                    if changed:
                        rm.set_enabled(mode, val)

            imgui.separator()

            # Display toggles
            sp = getattr(self.app, 'settings_panel', None)
            if sp:
                changed, val = imgui.checkbox("Show Grid", sp._grid_visible)
                if changed:
                    sp._grid_visible = val
                    sp._toggle_grid(val)
                    sp._persist()

            cp_display = getattr(self.app, 'connect_point_display', None)
            if cp_display is not None:
                changed, val = imgui.checkbox("Show Connect Points", cp_display.visible)
                if changed:
                    cp_display.visible = val

            skel_panel = getattr(self.app, 'skeleton_panel', None)
            if skel_panel:
                changed, val = imgui.checkbox("Show Bones", skel_panel._bones_visible)
                if changed:
                    skel_panel._bones_visible = val
                    if val:
                        nif = self.app.nif_file
                        if nif:
                            skel_panel._draw_bone_lines(nif)
                    else:
                        skel_panel._clear_bone_lines()

            renderer = getattr(self.app, 'renderer', None)
            if renderer is not None:
                changed, val = imgui.checkbox("Show Vertices", renderer.toggles.show_vertices)
                if changed:
                    renderer.toggles.show_vertices = val

                changed, val = imgui.checkbox("Show Collision", renderer._show_collision)
                if changed:
                    renderer._show_collision = val
                    renderer._collision_dirty = True

            light_display = getattr(self.app, 'light_display', None)
            if light_display is not None:
                changed, val = imgui.checkbox("Show Lights", light_display.visible)
                if changed:
                    light_display.visible = val

            imgui.end_menu()

    def _tools_menu(self):
        if imgui.begin_menu("Tools"):
            if imgui.menu_item("Deselect All", "", False)[0]:
                if hasattr(self.app, 'selection_mgr'):
                    self.app.selection_mgr.deselect()

            imgui.separator()

            if imgui.menu_item("Validate NIF", "", False, bool(self.app.nif_file))[0]:
                vp = getattr(self.app, 'validation_panel', None)
                if vp:
                    vp.validate()

            imgui.separator()

            # Collision submenu — operates on the root node (block 0).
            has_nif = bool(self.app.nif_file)
            has_coll = False
            if has_nif:
                try:
                    root_block = self.app.nif_file.get_block(0)
                    if root_block is not None:
                        coll_ref = root_block.get_field("Collision Object")
                        has_coll = isinstance(coll_ref, int) and coll_ref >= 0
                except Exception:
                    has_coll = False
            if imgui.begin_menu("Collision", has_nif):
                gen_label = (
                    "Regenerate Collision on Root..." if has_coll
                    else "Generate Collision on Root..."
                )
                if imgui.menu_item(gen_label, "", False, has_nif)[0]:
                    if hasattr(self.app, "open_collision_dialog"):
                        self.app.open_collision_dialog(0)
                if imgui.menu_item("Remove Collision from Root", "", False, has_coll)[0]:
                    self._remove_root_collision()
                imgui.end_menu()

            imgui.separator()

            if imgui.menu_item("Reset Light Position", "", False)[0]:
                lighting = getattr(self.app, 'lighting', None)
                if lighting:
                    lighting.key_heading = 30.0
                    lighting.key_pitch = -55.0
                    lighting._update_directions()

            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "Hold Alt + LMB drag to move light")

            imgui.end_menu()

    def _remove_root_collision(self):
        """Remove the collision object from the NIF root node."""
        nif = self.app.nif_file
        if not nif:
            return
        try:
            from creation_lib.nif.operations.collision import remove_collision
            result = remove_collision(nif, node_block_id=0)
        except Exception as exc:
            self.app.status_text = f"Remove collision error: {exc}"
            return
        if getattr(result, "success", False):
            if hasattr(self.app, "_nif_dirty"):
                self.app._nif_dirty = True
            try:
                self.app.rebuild_scene_from_nif()
            except Exception:
                pass
            renderer = getattr(self.app, "renderer", None)
            if renderer is not None:
                try:
                    renderer._collision_dirty = True
                except Exception:
                    pass
            self.app.status_text = result.description or "Collision removed"
        else:
            self.app.status_text = f"Remove collision failed: {result.description}"

    def _debug_menu(self):
        if imgui.begin_menu("Scene"):
            app = self.app

            # Texture/lighting toggles via renderer.toggles
            if hasattr(app, 'renderer') and app.renderer:
                t = app.renderer.toggles
                for label, attr in [
                    ("Vertex Colors", "vertex_colors"),
                ]:
                    cur = getattr(t, attr)
                    changed, val = imgui.checkbox(label, cur)
                    if changed:
                        setattr(t, attr, val)

                imgui.separator()

                for label, attr in [
                    ("Diffuse Texture", "diffuse"),
                    ("Normal Map",      "normal"),
                    ("Specular Map",    "specular"),
                    ("Environment Map", "env_map"),
                ]:
                    cur = getattr(t, attr)
                    changed, val = imgui.checkbox(label, cur)
                    if changed:
                        setattr(t, attr, val)

                imgui.separator()

                # Post-processing effects
                changed, val = imgui.checkbox("Ambient Occlusion", t.ssao)
                if changed:
                    t.ssao = val

                changed, val = imgui.checkbox("Shadows", t.shadows)
                if changed:
                    t.shadows = val
                    # Mark shadow map dirty when toggled on
                    if val:
                        app.renderer._shadow_dirty = True

            imgui.separator()

            # Lighting tuning
            imgui.text_colored(imgui.ImVec4(0.6, 0.8, 1.0, 1.0), "Lighting Tuning")

            # Preset selector
            from .toolbar_tbr import LIGHTING_PRESET_NAMES, LIGHTING_PRESETS, apply_lighting_preset
            current_idx = getattr(app, '_lighting_tuning_idx', 0)
            imgui.set_next_item_width(150)
            changed, new_idx = imgui.combo(
                "Lighting Preset", current_idx, LIGHTING_PRESET_NAMES
            )
            if changed:
                app._lighting_tuning_idx = new_idx
                apply_lighting_preset(app, LIGHTING_PRESET_NAMES[new_idx])
                # Persist
                sp = getattr(app, 'settings_panel', None)
                if sp:
                    sp._persist()

            # Light type (bulb color temperature)
            from creation_lib.renderer.lighting import LIGHT_TYPE_KEYS, LIGHT_TYPE_LABELS
            lighting = getattr(app, 'lighting', None)
            if lighting:
                current_lt = LIGHT_TYPE_KEYS.index(lighting.light_type) if lighting.light_type in LIGHT_TYPE_KEYS else 0
                imgui.set_next_item_width(150)
                changed, new_lt = imgui.combo(
                    "Light Type", current_lt, LIGHT_TYPE_LABELS
                )
                if changed:
                    lighting.set_light_type(LIGHT_TYPE_KEYS[new_lt])
                    sp = getattr(app, 'settings_panel', None)
                    if sp:
                        sp._light_type_idx = new_lt
                        sp._persist()

            # Individual sliders
            w = 150
            # Slider ranges: SF rendering uses a darker baseline than FO4
            # (uBrightnessScale=0.1, env_intensity=8.0 calibrated to the
            # standalone tools/sf_render_test.py defaults), so users need to
            # push env/spec/exposure/ambient further to dial in SF parity.
            # Ctrl+click any slider to type an exact value.
            _dbg_target = getattr(app, 'renderer', app)
            for label, attr, lo, hi, fmt in [
                ("Env Boost",      "_dbg_envBoost",     0.0, 20.0, "%.2f"),
                ("Metal F0",       "_dbg_metalF0",      0.0,  1.0, "%.2f"),
                ("Diffuse Bleed",  "_dbg_diffuseBleed", 0.0,  2.0, "%.2f"),
                ("Exposure",       "_dbg_exposure",     0.5, 40.0, "%.2f"),
                ("Spec Boost",     "_dbg_specBoost",    0.0, 20.0, "%.2f"),
                ("Ambient Boost",  "_dbg_ambientBoost", 0.0, 10.0, "%.2f"),
            ]:
                imgui.set_next_item_width(w)
                changed, val = imgui.slider_float(
                    label, getattr(_dbg_target, attr, 1.0), lo, hi, fmt
                )
                if changed:
                    setattr(_dbg_target, attr, val)
                    # Mark as custom if user tweaks a slider
                    app._lighting_tuning_idx = len(LIGHTING_PRESET_NAMES) - 1  # "Custom"

            imgui.separator()

            # Light position sliders
            lighting = getattr(app, 'lighting', None)
            if lighting:
                imgui.text_colored(imgui.ImVec4(0.6, 0.8, 1.0, 1.0), "Light Position")

                imgui.set_next_item_width(w)
                changed, val = imgui.slider_float(
                    "Key Heading", lighting.key_heading, -180.0, 180.0, "%.1f"
                )
                if changed:
                    lighting.key_heading = val
                    lighting._update_directions()
                    lighting._shadow_dirty = True

                imgui.set_next_item_width(w)
                changed, val = imgui.slider_float(
                    "Key Pitch", lighting.key_pitch, -89.0, 89.0, "%.1f"
                )
                if changed:
                    lighting.key_pitch = val
                    lighting._update_directions()
                    lighting._shadow_dirty = True

                imgui.set_next_item_width(w)
                changed, val = imgui.slider_float(
                    "Fill Heading", lighting.fill_heading, -180.0, 180.0, "%.1f"
                )
                if changed:
                    lighting.fill_heading = val
                    lighting._update_directions()

                imgui.set_next_item_width(w)
                changed, val = imgui.slider_float(
                    "Fill Pitch", lighting.fill_pitch, -89.0, 89.0, "%.1f"
                )
                if changed:
                    lighting.fill_pitch = val
                    lighting._update_directions()

                # Show current values (updates live when using Alt+LMB drag)
                imgui.text_colored(
                    imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
                    f"Key: H={lighting.key_heading:.1f} P={lighting.key_pitch:.1f}  "
                    f"Fill: H={lighting.fill_heading:.1f} P={lighting.fill_pitch:.1f}"
                )

                changed, val = imgui.checkbox("Mirror Light", lighting.mirror_light)
                if changed:
                    lighting.mirror_light = val

                changed, val = imgui.checkbox("Skylight", lighting.skylight)
                if changed:
                    lighting.skylight = val

                imgui.set_next_item_width(w)
                changed, val = imgui.slider_float(
                    "Key Intensity", lighting.key_intensity, 0.1, 5.0, "%.2f"
                )
                if changed:
                    lighting.key_intensity = val
                    lighting._shadow_dirty = True

                if imgui.button("Reset Light Position"):
                    lighting.key_heading = 110.0
                    lighting.key_pitch = -21.0
                    lighting.fill_heading = -135.0
                    lighting.fill_pitch = -15.0
                    lighting._update_directions()
                    lighting._shadow_dirty = True

            imgui.separator()

            skel_panel = getattr(app, 'skeleton_panel', None)
            if skel_panel:
                if imgui.menu_item("Show Skeleton Tools", "", False)[0]:
                    skel_panel.show()

            imgui.end_menu()

    def _settings_item(self):
        """Top-level Settings menu bar button — toggles the settings panel."""
        if imgui.menu_item("Settings", "", False)[0]:
            sp = getattr(self.app, 'settings_panel', None)
            if sp:
                sp.toggle()

    def _help_menu(self):
        if imgui.begin_menu("Help"):
            overlay = getattr(self.app, 'controls_overlay', None)
            if overlay:
                vis = overlay.visible
                clicked, new_vis = imgui.checkbox("Show Controls", vis)
                if clicked:
                    overlay.visible = new_vis

                imgui.separator()

            if imgui.menu_item("About", "", False)[0]:
                self.app._show_about = True
            imgui.end_menu()

    def _export_obj(self):
        """Export selected shape to OBJ."""
        props = getattr(self.app, 'properties_panel', None)
        block_id = getattr(props, '_selected_block_id', None) if props else None
        if block_id is None:
            self.app.status_text = ("Select a shape to export")
            return

        nif = self.app.nif_file
        block = nif.get_block(block_id) if nif else None
        if not block or not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
            self.app.status_text = ("Select a BSTriShape to export")
            return

        try:
            name = block.get_field("Name") or f"shape_{block_id}"
            if isinstance(name, list):
                name = "".join(str(c) for c in name)

            filepath = pick_save_file(
                "Export to OBJ",
                [("OBJ files", "*.obj"), ("All files", "*.*")],
                default_ext=".obj",
                initialfile=f"{name}.obj",
            )

            if filepath:
                from ui.editor.exporters.obj_export import export_shape_to_obj
                count = export_shape_to_obj(nif, block_id, filepath)
                if count >= 0:
                    self.app.status_text = (f"Exported {count} triangles to OBJ")
                else:
                    self.app.status_text = ("Export failed")
        except Exception as e:
            self.app.status_text = (f"Export error: {e}")

    def _export_fbx(self):
        """Export full scene to FBX."""
        try:
            from pathlib import Path

            initial = Path(self.app.current_path).stem if self.app.current_path else "export"

            filepath = pick_save_file(
                "Export to FBX",
                [("FBX Binary", "*.fbx"), ("All files", "*.*")],
                default_ext=".fbx",
                initialfile=f"{initial}.fbx",
            )

            if filepath:
                from ui.editor.exporters.fbx_export import export_fbx
                nif = self.app.nif_file
                result = export_fbx(nif, filepath, nif_path=self.app.current_path)
                if result:
                    self.app.status_text = f"Exported FBX to {filepath}"
                else:
                    self.app.status_text = "FBX export failed"
        except Exception as e:
            self.app.status_text = f"FBX export error: {e}"

    def _import_into_selection(self):
        """Import OBJ or FBX into the selected BSTriShape."""
        props = getattr(self.app, 'properties_panel', None)
        block_id = getattr(props, '_selected_block_id', None) if props else None
        if block_id is None:
            self.app.status_text = "Select a shape to import into"
            return

        nif = self.app.nif_file
        block = nif.get_block(block_id) if nif else None
        if not block or not nif.schema.is_subtype_of(block.type_name, "BSTriShape"):
            self.app.status_text = "Select a BSTriShape to import into"
            return

        try:
            filepath = pick_file(
                "Import Mesh into Selection",
                [
                    ("Mesh files", "*.obj *.fbx"),
                    ("OBJ files", "*.obj"),
                    ("FBX files", "*.fbx"),
                    ("All files", "*.*"),
                ],
            )

            if not filepath:
                return

            ext = filepath.rsplit(".", 1)[-1].lower()
            from creation_lib.nif.actions import SnapshotAction

            if ext == "obj":
                from ui.editor.exporters.obj_export import import_obj_to_shape
                cmd = SnapshotAction(_description="Import OBJ")
                cmd.capture_before(nif)
                count = import_obj_to_shape(nif, block_id, filepath)
                label = "OBJ"
            elif ext == "fbx":
                from ui.editor.exporters.fbx_import import import_fbx_to_shape
                cmd = SnapshotAction(_description="Import FBX")
                cmd.capture_before(nif)
                count = import_fbx_to_shape(nif, block_id, filepath)
                label = "FBX"
            else:
                self.app.status_text = f"Unsupported format: .{ext}"
                return

            if count >= 0:
                cmd.capture_after(nif)
                self.app.undo_manager.push(self.app.registry.active_id, cmd)
                self.app._nif_dirty = True
                self.app.rebuild_scene_from_nif()
                self.app.status_text = f"Imported {count} vertices from {label}"
            else:
                self.app.status_text = f"{label} import failed"
        except Exception as e:
            self.app.status_text = f"Import error: {e}"

    def _import_fbx_scene(self):
        """Import all FBX meshes as new BSTriShape blocks."""
        nif = self.app.nif_file
        if not nif:
            self.app.status_text = ("No NIF loaded")
            return

        try:
            filepath = pick_file(
                "Import FBX Scene",
                [("FBX files", "*.fbx"), ("All files", "*.*")],
            )

            if filepath:
                from creation_lib.nif.actions import SnapshotAction
                from ui.editor.exporters.fbx_import import import_fbx_scene

                cmd = SnapshotAction(_description="Import FBX Scene")
                cmd.capture_before(nif)
                count = import_fbx_scene(nif, filepath)
                if count >= 0:
                    cmd.capture_after(nif)
                    self.app.undo_manager.push(self.app.registry.active_id, cmd)
                    self.app._nif_dirty = True
                    self.app.rebuild_scene_from_nif()
                    self.app.status_text = (f"Imported {count} shapes from FBX")
                else:
                    self.app.status_text = ("FBX scene import failed")
        except Exception as e:
            self.app.status_text = (f"FBX import error: {e}")

    def _save_as(self):
        """Save with file dialog."""
        try:
            filepath = pick_save_file(
                "Save NIF As",
                [("NIF files", "*.nif"), ("All files", "*.*")],
                default_ext=".nif",
            )

            if filepath and self.app.nif_file:
                self.app.nif_file.save(filepath)
                # Update the active session's file path
                self.app.registry.active_session.file_path = filepath
                self.app.status_text = (f"Saved: {filepath}")
        except Exception as e:
            self.app.status_text = (f"Save error: {e}")

    def _save_all_as(self):
        """Save All As — show a Save dialog for each open NIF (prefilled with its name)."""
        try:
            from pathlib import Path

            sessions = self.app.registry.all_sessions()
            saved = 0
            for session in sessions:
                original_name = Path(session.file_path).name
                new_path = pick_save_file(
                    f"Save {original_name} As",
                    [("NIF files", "*.nif"), ("All files", "*.*")],
                    default_ext=".nif",
                    initialfile=original_name,
                )
                if not new_path:
                    continue
                try:
                    session.nif.save(new_path)
                    session.file_path = new_path
                    session.dirty = False
                    saved += 1
                except Exception as e:
                    self.app.status_text = f"Save error ({original_name}): {e}"
                    return

            self.app.status_text = f"Saved {saved} NIF(s)"
        except Exception as e:
            self.app.status_text = f"Save All As error: {e}"

    def _attach_nif(self):
        """Attach NIF — pick a file, auto-detect parent connect point, attach."""
        try:
            filepath = pick_file(
                "Attach NIF",
                NIF_LIKE_FILETYPES,
            )

            if not filepath:
                return

            self.app.attach_nif_auto(filepath)

        except Exception as e:
            self.app.status_text = f"Attach error: {e}"

    def _bash_nif(self):
        """Bash NIF — pick a file and merge it into the current root."""
        try:
            filepath = pick_file(
                "Bash NIF into current",
                NIF_LIKE_FILETYPES,
            )

            if not filepath:
                return

            self.app.bash_nif(filepath)

        except Exception as e:
            self.app.status_text = f"Bash error: {e}"

    def _browse_nifs(self):
        """Open the NIF browser popup."""
        from app.paths import get_db_dir
        from creation_lib.db.store import GameDataStore
        # Use the active NIF session's game if available, else fall back to fo4
        try:
            game_id = self.app.registry.active_session.game_profile.id
        except Exception:
            game_id = "fo4"
        if GameDataStore(db_dir=str(get_db_dir()), game=game_id).is_available("nifs"):
            self.app._nif_browser.open(game_id)
        else:
            self.app.status_text = "NIF index not available (run preprocess_nifs.py)"

    def _render_import_options_modal(self):
        """Render the NIF import options modal popup."""
        if self._show_import_options:
            imgui.open_popup("Import NIF Options")
            self._show_import_options = False

        opened, visible = imgui.begin_popup_modal(
            "Import NIF Options", True,
            imgui.WindowFlags_.always_auto_resize
        )
        if opened:
            opts = self.app.import_options

            _, opts.import_geometry = imgui.checkbox("Import Geometry", opts.import_geometry)
            _, opts.import_animations = imgui.checkbox("Import Animations", opts.import_animations)
            _, opts.import_connect_points = imgui.checkbox("Import Connect Points", opts.import_connect_points)
            _, opts.import_root_extra_data = imgui.checkbox("Import Root Extra Data", opts.import_root_extra_data)

            imgui.separator()

            if imgui.button("Import", 120, 0):
                self._do_import_nif()
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Cancel", 120, 0):
                imgui.close_current_popup()

            imgui.end_popup()

    def _import_new_start(self):
        """Open file dialog for Import > New: pick a NIF/FBX/OBJ to import into a fresh NIF."""
        try:
            filepath = pick_file(
                "Import into New NIF",
                [
                    NIF_LIKE_FILETYPES[0],
                    ("FBX files", "*.fbx"),
                    ("OBJ files", "*.obj"),
                    ("All files", "*.*"),
                ],
            )

            if filepath:
                self._import_new_source_path = filepath
                self._show_import_new_modal = True
        except Exception as e:
            self.app.status_text = f"Import error: {e}"

    def _render_import_new_modal(self):
        """Render the game-picker modal for Import > New."""
        from creation_lib.core.game_profiles import GAME_PROFILES

        if self._show_import_new_modal:
            imgui.open_popup("Import into New NIF")
            self._show_import_new_modal = False

        opened, visible = imgui.begin_popup_modal(
            "Import into New NIF", True,
            imgui.WindowFlags_.always_auto_resize,
        )
        if not opened:
            return

        game_keys = sorted(GAME_PROFILES.keys())
        game_labels = [GAME_PROFILES[k].display_name for k in game_keys]

        imgui.text("Target game:")
        imgui.set_next_item_width(220)
        changed, self._import_new_game_idx = imgui.combo(
            "##new_game", self._import_new_game_idx, game_labels,
        )

        imgui.spacing()
        import os
        imgui.text_colored(
            imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
            os.path.basename(self._import_new_source_path),
        )
        imgui.spacing()
        imgui.separator()

        if imgui.button("Import", 120, 0):
            game_id = game_keys[self._import_new_game_idx]
            self._do_import_new(game_id, self._import_new_source_path)
            imgui.close_current_popup()
        imgui.same_line()
        if imgui.button("Cancel", 120, 0):
            imgui.close_current_popup()

        imgui.end_popup()

    def _do_import_new(self, game_id: str, filepath: str):
        """Create a new NIF for game_id and import filepath into it."""
        import os
        from creation_lib.nif.nif_file import NifFile
        from creation_lib.core.game_profiles import get_profile
        from ui.editor.nif_session import NifSession
        from creation_lib.renderer.nif_loader import rebuild_scene_from_nif

        try:
            game_profile = get_profile(game_id)
        except KeyError:
            self.app.status_text = f"Unknown game: {game_id}"
            return

        nif = NifFile.new(game_id)
        ext = filepath.rsplit(".", 1)[-1].lower()

        try:
            if is_nif_like_path(filepath):
                source = NifFile.load(filepath)
                from creation_lib.renderer.nif_importer import import_nif
                # Temporarily register a bare session so import_nif can find app.nif
                self.app.registry.clear()
                _tmp_scene = rebuild_scene_from_nif(
                    nif, self.app.ctx,
                    self.app.renderer.programs.get(game_id) or self.app.renderer.programs.get("default"),
                    [], self.app.ba2_manager,
                    game_profile=game_profile,
                )
                _tmp_anim = self.app._create_animation_manager()
                _tmp_particle_models, _tmp_particle_runtime = (
                    self.app._create_particle_runtime(nif, "main")
                )
                _tmp_session = NifSession(
                    nif_id="main", nif=nif, file_path="untitled.nif",
                    scene_root=_tmp_scene, anim_manager=_tmp_anim,
                    game_profile=game_profile,
                    particle_models=_tmp_particle_models,
                    particle_runtime=_tmp_particle_runtime,
                )
                self.app.registry.add_session(_tmp_session)
                self.app.registry.active_id = "main"
                result = import_nif(self.app, source, self.app.import_options)
                if result.error:
                    self.app.status_text = f"Import failed: {result.error}"
                    return
                count_msg = f"Imported {result.imported_count} blocks from {os.path.basename(filepath)}"

            elif ext == "fbx":
                from ui.editor.exporters.fbx_import import import_fbx_scene
                count = import_fbx_scene(nif, filepath)
                if count < 0:
                    self.app.status_text = "FBX import failed"
                    return
                count_msg = f"Imported {count} shapes from {os.path.basename(filepath)}"

            elif ext == "obj":
                from ui.editor.exporters.obj_export import import_obj_to_shape
                # Add a BSTriShape child to the BSFadeNode root (block 0)
                shape = nif.add_block("BSTriShape", {"Name": "ImportedMesh"})
                root_block = nif.get_block(0)
                children = root_block.get_field("Children") or []
                children.append({"block_id": shape.block_id})
                root_block.set_field("Children", children)
                root_block.set_field("Num Children", len(children))
                count = import_obj_to_shape(nif, shape.block_id, filepath)
                if count < 0:
                    self.app.status_text = "OBJ import failed"
                    return
                count_msg = f"Imported {count} vertices from {os.path.basename(filepath)}"

            else:
                self.app.status_text = f"Unsupported format: .{ext}"
                return

        except Exception as e:
            self.app.status_text = f"Import error: {e}"
            return

        # Build the final session (NIF path may now contain merged data)
        program = (self.app.renderer.programs.get(game_id)
                   or self.app.renderer.programs.get("default"))
        texture_dirs, *_ = self.app._build_texture_dirs(game_profile=game_profile)
        scene_root = rebuild_scene_from_nif(
            nif, self.app.ctx, program, texture_dirs, self.app.ba2_manager,
            game_profile=game_profile,
        )
        anim_mgr = self.app._create_animation_manager()
        anim_mgr.scan(nif)
        particle_models, particle_runtime = self.app._create_particle_runtime(
            nif, "main"
        )
        session = NifSession(
            nif_id="main", nif=nif, file_path="untitled.nif",
            scene_root=scene_root, anim_manager=anim_mgr,
            game_profile=game_profile,
            particle_models=particle_models,
            particle_runtime=particle_runtime,
        )
        self.app.registry.clear()
        self.app.registry.add_session(session)
        self.app.registry.active_id = "main"
        self.app.renderer.scene_root = scene_root
        self.app.renderer.clear_alt_vao_cache()
        self.app.selection_mgr.clear()
        self.app.selection_mgr.register_bounds(scene_root)
        self.app.undo_manager.clear()
        self.app.status_text = count_msg

    def _import_nif_start(self):
        """Open file dialog to pick source NIF, then show options modal."""
        try:
            filepath = pick_file(
                "Import NIF",
                NIF_LIKE_FILETYPES,
            )

            if filepath:
                self._import_source_path = filepath
                self._show_import_options = True
        except Exception as e:
            self.app.status_text = f"Import error: {e}"

    def _do_import_nif(self):
        """Execute the NIF import after options are confirmed."""
        from creation_lib.renderer.nif_importer import import_nif
        from creation_lib.nif.nif_file import NifFile

        try:
            source = NifFile.load(self._import_source_path)
        except Exception as e:
            self.app.status_text = f"Failed to load source NIF: {e}"
            return

        result = import_nif(self.app, source, self.app.import_options)

        if result.error:
            self.app.status_text = f"Import failed: {result.error}"
            return

        # Save updated options
        self.app._save_settings()

        # Rebuild viewport
        self.app.rebuild_scene_from_nif()

        # Status message
        filename = os.path.basename(self._import_source_path)
        msg = f"Imported {result.imported_count} blocks from {filename}"
        if result.skipped:
            msg += f" ({len(result.skipped)} skipped)"
        self.app.status_text = msg

    def _export_nif_as(self):
        """Export current NIF to a new file (does not change working file)."""
        try:
            filepath = pick_save_file(
                "Export NIF As",
                [("NIF files", "*.nif"), ("All files", "*.*")],
                default_ext=".nif",
            )

            if filepath:
                self.app.nif.save(filepath)
                filename = os.path.basename(filepath)
                self.app.status_text = f"Exported to {filename}"
        except Exception as e:
            self.app.status_text = f"Export error: {e}"
