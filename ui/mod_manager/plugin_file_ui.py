"""Explicit owned plugin file review, apply/cancel and guarded undo."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button

from creation_lib.mod_manager import plugin_file_actions as actions
from creation_lib.mod_manager.paths import ManagerError
from .toolbar_ui import _next_button


class PluginFileUI:
    def __init__(self, workspace):
        self.workspace, self.plan, self.last = workspace, None, ""
        self.name, self.new, self.action = "", "", "rename"

    def open(self, name):
        w, m = self.workspace, self.workspace.manager
        plugin = next((p for p in (m.plugin_catalog.plugins if m.plugin_catalog else ()) if p.name == name), None)
        reason = actions.unavailable(m.library, m.plugin_catalog, plugin)
        if reason:
            raise ManagerError(reason)
        self.library, self.catalog, self.plugin = m.library, m.plugin_catalog, plugin
        self.plan, self.name, self.new = None, name, name
        self.action = "unghost" if plugin.ghosted else "rename"
        w._tab = "Owned plugin file"

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        imgui.text_wrapped("Owned plugin: " + self.name)
        imgui.text_wrapped("Review one inactive owned plugin. Apply retains its original file for guarded Undo.")
        enabled = m.library is self.library and m.plugin_catalog is self.catalog and not m.busy and not m.runtime_active
        imgui.begin_disabled(not enabled)
        imgui.begin_child("owned_plugin_fields", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            if imgui.collapsing_header("Action coverage and safety checks"):
                imgui.text_wrapped("Deactivate in every local profile/target first. Rename preserves the extension, updates local filename lists, and refuses dependent masters and matching INI/archive sidecars. Other enabled profiles require a separate refactoring workflow. Ghosted plugins cannot be activated. Future activation/order needs a fresh Details review.")
            choices = ["unghost"] if self.plugin.ghosted else ["rename", "ghost", "esm", "no-esm"]
            changed, index = imgui.combo("Owned file action", choices.index(self.action) if self.action in choices else 0, choices)
            if changed:
                self.action, self.plan = choices[index], None
            if self.action == "rename":
                imgui.set_next_item_width(-1)
                changed, self.new = imgui.input_text("New plugin filename", self.new)
                if changed:
                    self.plan = None
            imgui.text_wrapped("Master flag: inspected non-light ESP only. ESL conversion and other flags are unsupported here.")
            if self.plan:
                imgui.text_wrapped(f"{self.plan.action}: {self.plan.old} -> {self.plan.new} | {self.plan.original['size']:,} bytes. Backup retains just this plugin.")
        finally:
            imgui.end_child()
        if action_button("Review owned plugin action"):
            self.plan = None
            def complete(plan):
                self.plan = plan
                m.status = "Owned plugin file action reviewed. Apply or Cancel."
            m.submit("Reviewing owned plugin file", lambda cancel, progress: actions.review(self.library, self.catalog, self.plugin, self.action, self.new, cancel), complete)
        if self.plan:
            _next_button("Apply owned plugin action")
            if action_button("Apply owned plugin action", primary=True):
                plan = self.plan
                def complete(identifier):
                    self.last, self.plan = identifier, None
                    m.refresh()
                    w._tab = "Library"
                    m.status = "Owned plugin changed. Its single-file backup supports guarded Undo. Refresh plugin Details before saving activation/order."
                m.submit("Applying owned plugin file", lambda cancel, progress: actions.apply(self.library, plan, cancel), complete)
        imgui.end_disabled()
        if self.last and m.library is getattr(self, "library", None):
            if action_button("Undo last owned plugin file action", enabled=not m.busy and not m.runtime_active):
                self.undo()
        _next_button("Cancel owned plugin action")
        if action_button("Cancel owned plugin action", enabled=not m.busy):
            self.plan, w._tab = None, "Library"

    def undo(self):
        m = self.workspace.manager
        if not self.last or m.library is not self.library:
            raise ManagerError("There is no owned plugin action in this session to undo.")
        identifier = self.last
        def complete(_):
            self.last = ""
            m.refresh()
            m.status = "Owned plugin file and local filename metadata restored. Refresh plugin Details."
        m.submit("Restoring owned plugin file", lambda cancel, progress: actions.undo(self.library, identifier, cancel), complete)
