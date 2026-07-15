"""Controls overlay — semi-transparent HUD showing navigation controls."""

from imgui_bundle import imgui


class ControlsOverlay:
    """Displays current navigation controls as a bottom-right overlay."""

    _FONT_SCALE = 0.90

    def __init__(self, app):
        self.app = app
        self.visible = True
        self._last_h = 0  # actual window height from previous frame

    def draw(self):
        if not self.visible:
            return

        nav = getattr(self.app.camera, 'nav_style', 'default')

        # Build control lines based on nav style
        # Common controls shared across all nav styles
        common = [
            ("LMB Click", "Select"),
            ("RMB Click", "Context Menu"),
            ("Alt + LMB Drag", "Rotate Light"),
        ]

        if nav == 'blender':
            nav_controls = [
                ("MMB Drag", "Orbit"),
                ("Shift + MMB Drag", "Pan"),
                ("Scroll", "Zoom"),
            ]
            gizmo_keys = [
                ("G", "Move Gizmo"),
                ("R", "Rotate Gizmo"),
                ("S", "Scale Gizmo"),
            ]
        elif nav == '3dsmax':
            nav_controls = [
                ("Alt + MMB Drag", "Orbit"),
                ("MMB Drag", "Pan"),
                ("Scroll", "Zoom"),
            ]
            gizmo_keys = [
                ("W", "Move Gizmo"),
                ("E", "Rotate Gizmo"),
                ("R", "Scale Gizmo"),
            ]
        else:  # default
            nav_controls = [
                ("Ctrl + LMB Drag", "Orbit"),
                ("MMB Drag", "Pan"),
                ("Scroll", "Zoom"),
            ]
            gizmo_keys = [
                ("W", "Move Gizmo"),
                ("E", "Rotate Gizmo"),
                ("R", "Scale Gizmo"),
            ]

        controls = common + nav_controls + gizmo_keys

        shortcuts = [
            ("F", "Frame All"),
            ("O", "Open File"),
            ("Ctrl+S", "Save"),
            ("Esc", "Quit"),
        ]

        # Position in bottom-right of the VIEWPORT only (not the whole window)
        # Use the viewport's imgui content region, not its full extent
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
        # Use last frame's actual height for accurate positioning; fall back to estimate
        panel_h = self._last_h if self._last_h > 0 else (len(controls) + len(shortcuts) + 3) * 20 + 16

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

        expanded, _ = imgui.begin("##controls_overlay", True, flags)
        self._last_h = imgui.get_window_size().y
        if expanded:
            # Navigation header
            style_label = nav.capitalize() if nav != '3dsmax' else '3ds Max'
            imgui.text_colored(imgui.ImVec4(0.7, 0.85, 1.0, 1.0), f"Navigation ({style_label})")
            imgui.separator()

            for key, action in controls:
                imgui.text_colored(imgui.ImVec4(0.9, 0.9, 0.6, 1.0), key)
                imgui.same_line(150)
                imgui.text(action)

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
