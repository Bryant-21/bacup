"""Bone Editor workspace — wraps ui.bone_editor for the toolkit."""
from __future__ import annotations

import logging

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from creation_lib.ui.shell import BaseWorkspace, make_window

_log = logging.getLogger("toolkit.bone_editor")
_NS = "##bone_editor"


class BoneEditorWorkspace(BaseWorkspace):
    name = "Bulk Editor"
    icon = "BNE"
    id = "bone_editor"
    user_guide_body = """
Load a reference body and apply bone edits to meshes in bulk.
Use the setup, bones, and apply panels to prepare and commit pose changes.
"""

    def get_dockable_windows(self):
        return [
            make_window(f"Viewport{_NS}", "MainDockSpace"),
            make_window(f"Setup{_NS}", "LeftDock"),
            make_window(f"Apply{_NS}", "LeftDockBottom"),
            make_window(f"Bones{_NS}", "RightDock"),
        ]

    def initialize(self) -> None:
        from ui.bone_editor.bone_editor_app import BoneEditorApp

        self._app = BoneEditorApp(toolkit_settings=self._toolkit_settings)
        self._app.setup()
        self._app._init_panels()
        self._app._first_frame = False
        self._initialized = True

        if self._pending_settings:
            self._apply_saved_paths(self._pending_settings)
            self._pending_settings = None

        self._bind_panels({
            f"Viewport{_NS}": self._app.viewport_panel.draw,
            f"Setup{_NS}": self._app.setup_panel.draw,
            f"Bones{_NS}": self._app.bone_panel.draw,
            f"Apply{_NS}": self._app.apply_panel.draw,
        })
        _log.info("Bone Editor workspace initialized")

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw([
                f"Viewport{_NS}", f"Setup{_NS}",
                f"Bones{_NS}", f"Apply{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        app = self._app
        if app is None:
            return

        def _btn(icon: str) -> bool:
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        sess = app.pose_session
        playback = getattr(app, "playback", None)
        is_playing = playback is not None and playback.is_active()
        is_looping = playback is not None and playback.is_looping()
        # Undo/redo disabled during playback — editing is inert anyway,
        # but this makes the disabled state visible in the toolbar.
        has_undo = sess is not None and sess.can_undo() and not is_playing
        has_redo = sess is not None and sess.can_redo() and not is_playing

        # Undo
        if not has_undo:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_ROTATE_LEFT}##be_undo"):
            sess.undo()
        if not has_undo:
            imgui.end_disabled()
        imgui.set_item_tooltip("Undo (Ctrl+Z)")
        imgui.same_line()

        # Redo
        if not has_redo:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_ROTATE_RIGHT}##be_redo"):
            sess.redo()
        if not has_redo:
            imgui.end_disabled()
        imgui.set_item_tooltip("Redo (Ctrl+Y)")
        imgui.same_line()

        imgui.text("|")
        imgui.same_line()

        # Character / preset body picker — opens the same popup the Setup
        # panel uses, by flipping its `_show_preset_popup` flag. The popup
        # itself lives inside setup_panel.draw() and is queued for display
        # via imgui.open_popup the same frame.
        has_skel = app.skeleton is not None
        if has_skel:
            imgui.push_style_color(
                imgui.Col_.button, imgui.ImVec4(0.2, 0.5, 0.2, 1.0))
        if _btn(f"{fa.ICON_FA_PERSON}##be_char"):
            if app.setup_panel is not None:
                app.setup_panel._show_preset_popup = True
        if has_skel:
            imgui.pop_style_color()
        char_label = (
            f"Character: {app.skeleton.bone_count} bones"
            if has_skel else "Load Reference Body"
        )
        imgui.set_item_tooltip(char_label)

        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        # Playback: Play (one-shot), Loop, Stop.
        has_frames = playback is not None and playback.has_frames()
        can_play = has_frames and not is_playing

        if not can_play:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_PLAY}##be_play"):
            playback.play(loop=False)
        if not can_play:
            imgui.end_disabled()
        imgui.set_item_tooltip(
            "Play once (disabled — no animation loaded)"
            if not has_frames else "Play animation once"
        )
        imgui.same_line()

        # Loop toggle: green tint when active.
        loop_enabled = has_frames
        if is_looping:
            imgui.push_style_color(
                imgui.Col_.button, imgui.ImVec4(0.2, 0.5, 0.2, 1.0))
        if not loop_enabled:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_REPEAT}##be_loop"):
            if is_looping:
                playback.stop()
            else:
                playback.play(loop=True)
        if not loop_enabled:
            imgui.end_disabled()
        if is_looping:
            imgui.pop_style_color()
        imgui.set_item_tooltip(
            "Loop animation (click to toggle)"
            if has_frames else "Loop (disabled — no animation loaded)"
        )
        imgui.same_line()

        # Stop: only enabled while playing.
        if not is_playing:
            imgui.begin_disabled()
        if _btn(f"{fa.ICON_FA_STOP}##be_stop"):
            playback.stop()
        if not is_playing:
            imgui.end_disabled()
        imgui.set_item_tooltip("Stop playback")

    def draw(self) -> None:
        if self._initialized and self._app is not None and self._app.active:
            # Advance animation playback before the viewport draws. The
            # controller handles is-active gating and auto-stops one-shot
            # playback at the last frame (clearing playback_pose so the
            # viewport snaps back to baseline * user-delta).
            playback = getattr(self._app, "playback", None)
            if playback is not None and playback.is_active():
                dt = float(imgui.get_io().delta_time)
                playback.update(dt)
            self._app.gui()

    def on_activate(self) -> None:
        super().on_activate()

    def on_deactivate(self) -> None:
        super().on_deactivate()

    def cleanup(self) -> None:
        pass

    def get_settings_defaults(self) -> dict:
        return {}

    def collect_settings(self) -> dict:
        return {}

    def apply_settings(self, data: dict) -> None:
        if not self._initialized:
            self._pending_settings = data
            return
        self._apply_saved_paths(data)

    def _apply_saved_paths(self, data: dict) -> None:
        # Honor any saved paths from prior session
        sp = data.get("paths") or {}
        if not self._app or not self._app.setup_panel:
            return
        if "skeleton" in sp:
            self._app.setup_panel._skeleton_path = sp["skeleton"]
        if "body" in sp:
            self._app.setup_panel._body_mesh_path = sp["body"]
        if "weapon" in sp:
            self._app.setup_panel._weapon_mesh_path = sp["weapon"]
        if "anim_folder" in sp:
            self._app.setup_panel._anim_folder_path = sp["anim_folder"]
        if "output_folder" in sp:
            self._app.setup_panel._output_folder_path = sp["output_folder"]
