"""Controls overlay — semi-transparent HUD showing cloth maker controls."""

from imgui_bundle import imgui


class ControlsOverlay:
    """Displays navigation and brush controls as a bottom-right overlay."""

    _FONT_SCALE = 0.90

    def __init__(self, app):
        self.app = app
        self.visible = False
        self._last_h = 0  # actual window height from previous frame

    def draw(self):
        if not self.visible:
            return

        # Camera controls
        camera_controls = [
            ("Alt + LMB Drag", "Orbit"),
            ("MMB Drag", "Pan"),
            ("Scroll", "Zoom"),
            ("F", "Frame Mesh"),
        ]

        # Brush controls (only when a brush is active)
        brush_active = self.app._is_brush_active()
        brush_controls = [
            ("LMB", "Paint"),
            ("Shift + LMB", "Erase"),
        ]

        # General shortcuts
        shortcuts = [
            ("Ctrl+I", "Import NIF"),
            ("Ctrl+E", "Export NIF"),
            ("F1", "Help"),
            ("H", "Toggle Controls"),
        ]

        # Position in bottom-right of the VIEWPORT
        pad = 16
        vp_pos = getattr(self.app, '_viewport_pos', None)
        vp_size = getattr(self.app, '_viewport_size', None)
        if vp_pos is not None and vp_size is not None:
            anchor_x = vp_pos.x + vp_size.x
            anchor_y = vp_pos.y + vp_size.y
        else:
            io = imgui.get_io()
            anchor_x = io.display_size.x
            anchor_y = io.display_size.y

        panel_w = 240
        # Estimate height from previous frame or line count
        total_lines = len(camera_controls) + len(shortcuts) + 3  # headers + separators
        if brush_active:
            total_lines += len(brush_controls) + 2
        panel_h = self._last_h if self._last_h > 0 else total_lines * 20 + 16

        # Clamp so overlay stays inside the viewport bounds
        pos_x = anchor_x - panel_w - pad
        pos_y = anchor_y - panel_h - pad
        if vp_pos is not None:
            pos_y = max(pos_y, vp_pos.y + pad)

        imgui.set_next_window_pos((pos_x, pos_y))
        imgui.set_next_window_size((panel_w, 0))
        imgui.set_next_window_bg_alpha(0.6)

        flags = (
            imgui.WindowFlags_.no_title_bar.value
            | imgui.WindowFlags_.no_resize.value
            | imgui.WindowFlags_.no_move.value
            | imgui.WindowFlags_.no_scrollbar.value
            | imgui.WindowFlags_.no_saved_settings.value
            | imgui.WindowFlags_.no_focus_on_appearing.value
            | imgui.WindowFlags_.no_nav.value
            | imgui.WindowFlags_.always_auto_resize.value
            | imgui.WindowFlags_.no_docking.value
        )

        small_font = getattr(self.app, 'small_font', None)
        if small_font:
            imgui.push_font(small_font, small_font.legacy_size * self._FONT_SCALE)

        expanded, _ = imgui.begin("##cloth_controls_overlay", True, flags)
        self._last_h = imgui.get_window_size().y
        if expanded:
            # Camera header
            imgui.text_colored(imgui.ImVec4(0.7, 0.85, 1.0, 1.0), "Camera")
            imgui.separator()

            for key, action in camera_controls:
                imgui.text_colored(imgui.ImVec4(0.9, 0.9, 0.6, 1.0), key)
                imgui.same_line(150)
                imgui.text(action)

            # Brush section (only when active)
            if brush_active:
                imgui.spacing()
                imgui.text_colored(imgui.ImVec4(0.7, 0.85, 1.0, 1.0), "Brush")
                imgui.separator()

                for key, action in brush_controls:
                    imgui.text_colored(imgui.ImVec4(0.9, 0.9, 0.6, 1.0), key)
                    imgui.same_line(150)
                    imgui.text(action)

            # General shortcuts
            imgui.spacing()
            imgui.text_colored(imgui.ImVec4(0.7, 0.85, 1.0, 1.0), "Shortcuts")
            imgui.separator()

            for key, action in shortcuts:
                imgui.text_colored(imgui.ImVec4(0.9, 0.9, 0.6, 1.0), key)
                imgui.same_line(150)
                imgui.text(action)

        imgui.end()
        if small_font:
            imgui.pop_font()
