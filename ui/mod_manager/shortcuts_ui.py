"""Atomic draft command binding preferences with explicit collision errors."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button

from . import shortcuts
from creation_lib.mod_manager.paths import ManagerError


class ShortcutsUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.draft, self.error = None, ""
        self.expected = None

    def open(self):
        self.draft, self.error = dict(self.workspace._shortcuts), ""
        self.expected = dict(self.draft)
        self.workspace._dialog = "Shortcuts"
        self.workspace._tab = "Shortcuts"

    def draw(self):
        w = self.workspace
        if self.draft is None:
            self.open()
        imgui.text_wrapped("Bind manager commands, for example Ctrl+Shift+J or F6. Empty disables a command. Shared chords are allowed between separate Mod and Plugin panes. Standard text editing and arrow navigation stay with the toolkit.")
        if self.error:
            imgui.text_wrapped(self.error)
        imgui.begin_child("shortcut_preferences", (0, -imgui.get_frame_height_with_spacing() * 2))
        try:
            for command, title in shortcuts.LABELS.items():
                imgui.push_id(command)
                imgui.text_unformatted(title)
                imgui.set_next_item_width(-1)
                _, self.draft[command] = imgui.input_text("##binding_" + command, self.draft[command])
                imgui.pop_id()
        finally:
            imgui.end_child()
        if action_button("Reset shortcut draft"):
            self.draft, self.error = dict(shortcuts.DEFAULTS), ""
        from .toolbar_ui import _next_button
        _next_button("Save shortcuts")
        if action_button("Save shortcuts", primary=True):
            try:
                if w._shortcuts != self.expected:
                    raise ManagerError("Shortcut preferences changed while this draft was open. Cancel and reopen it.")
                w._shortcuts = shortcuts.validate(self.draft)
                w._dialog, self.draft, self.error = "", None, ""
            except ManagerError as error:
                self.error = str(error)
        _next_button("Cancel shortcuts")
        if action_button("Cancel shortcuts"):
            w._dialog, self.draft, self.error = "", None, ""
