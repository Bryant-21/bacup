"""Folder-first target setup and compact switcher within the native manager."""
from imgui_bundle import imgui
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.forms import pick_file
from creation_lib.ui.widgets.modern import action_button, heading, scaled


class TargetsUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.name, self.folder, self.edit_id, self.archive, self.nxm = "", "", "", "", ""
        from .f4se_root_ui import RootSetupUI
        self.root_setup = RootSetupUI(workspace)

    def switcher(self):
        m = self.workspace.manager
        selected = next((item for item in m.targets if item.id == m.target_id), None)
        imgui.begin_disabled(m.busy)
        imgui.set_next_item_width(min(scaled(300), max(scaled(120), imgui.get_content_region_avail().x - scaled(150))))
        if imgui.begin_combo("##launch_target", selected.name if selected else "Original executables"):
            if imgui.selectable("Original executables##target_original", not m.target_id)[0]:
                self.workspace._action(lambda: m.select_target(""))
            for item in m.targets:
                imgui.push_id(item.id)
                if imgui.selectable(item.name + "##target_choice", item.id == m.target_id)[0]:
                    self.workspace._action(lambda value=item.id: m.select_target(value))
                imgui.set_item_tooltip(item.game_root + "\n" + item.f4se.get("reason", ""))
                imgui.pop_id()
            imgui.end_combo()
        imgui.same_line()
        if action_button("FO4 folders", enabled=bool(m.library)):
            self.workspace._dialog = "Launch targets"
            imgui.close_current_popup()
        imgui.end_disabled()

    def draw(self):
        m = self.workspace.manager
        heading("FO4 launch folders")
        imgui.text_wrapped("One modlist, several Steam FO4 folders. Each keeps local INIs, plugins, saves and output; matching F4SE is prepared privately with optional recoverable root placement.")
        selected = next((item for item in m.targets if item.id == m.target_id), None)
        imgui.begin_disabled(m.busy or m.runtime_active)
        footer_rows = 3.5 if m.root_setup_review else 2.5
        imgui.begin_child("launch_target_details", (0, -imgui.get_frame_height_with_spacing() * footer_rows))
        try:
            self._details(selected)
        finally:
            imgui.end_child()
        if m.root_setup_review:
            self.root_setup.footer()
        else:
            self._folder_footer(selected)
        imgui.end_disabled()

    def _folder_footer(self, selected):
        m = self.workspace.manager
        if action_button("Review FO4 folder", primary=True, enabled=bool(self.name and self.folder)):
            self.workspace._action(lambda: m.review_target(self.name, self.folder, self.edit_id))
        if m.target_review:
            imgui.same_line()
            if action_button("Use FO4 folder", primary=True):
                nxm = self.nxm
                self.nxm = ""
                self.workspace._action(lambda: m.apply_target(archive=self.archive, nxm=nxm))
            imgui.same_line()
            if action_button("Cancel folder review"):
                m.target_review = None
        elif selected:
            imgui.same_line()
            if selected.f4se.get("managed") and selected.f4se.get("state") == "ready" and not selected.f4se.get("root_setup"):
                if action_button("Review game-root F4SE setup", primary=True):
                    self.workspace._action(m.review_target_root_setup)
            elif action_button("Prepare matching F4SE", primary=True, enabled=not selected.f4se.get("root_setup")):
                nxm = self.nxm
                self.nxm = ""
                self.workspace._action(lambda: m.prepare_target_f4se(archive=self.archive, nxm=nxm))

    def _details(self, selected):
        m = self.workspace.manager
        if selected and m.root_setup_review:
            self.root_setup.details(selected)
            return
        if selected:
            imgui.text_unformatted("Selected: " + selected.name)
            imgui.text_wrapped(selected.game_root)
            imgui.text_wrapped(selected.f4se.get("reason", ""))
            if action_button("Edit selected folder", enabled=not m.busy and not m.runtime_active):
                self.name, self.folder, self.edit_id = selected.name, selected.game_root, selected.id
                m.target_review = None
            self.root_setup.details(selected)
            for label, mode in (("Edit launch arguments", "Arguments"), ("Mod source for this target", "Source"), ("INI and save preferences", "Preferences"), ("Remove launch target", "Remove")):
                if action_button(label, enabled=not m.busy and not m.runtime_active and (mode != "Source" or bool(m.selected_id))):
                    self.workspace._action(lambda value=mode: self.workspace.target_customization.open(value))
        from creation_lib.mod_manager.target_removal import removed
        if m.library and imgui.collapsing_header("Removed launch targets"):
            for item in removed(m.library):
                imgui.push_id(item.id)
                if action_button("Review restore " + item.name, enabled=not m.busy and not m.runtime_active):
                    self.workspace._action(lambda value=item.id: self.workspace.target_customization.open("Remove", value))
                imgui.pop_id()
        if action_button("New FO4 folder", enabled=not m.busy and not m.runtime_active):
            self.name, self.folder, self.edit_id = "", "", ""
            m.target_review = None
        imgui.begin_disabled(m.busy or m.runtime_active)
        try:
            for label, attribute in (("Target name", "name"), ("FO4 folder", "folder")):
                imgui.text_unformatted(label)
                imgui.set_next_item_width(-1)
                changed, value = imgui.input_text("##target_" + attribute, getattr(self, attribute))
                if changed:
                    setattr(self, attribute, value)
                    m.target_review = None
            if action_button("Browse FO4 folder"):
                value = pick_folder("Choose a FO4 installation folder", self.folder)
                if value:
                    self.folder = str(value)
                    if not self.name:
                        from pathlib import Path
                        self.name = Path(value).name
                    m.target_review = None
            plan = m.target_review
            if plan:
                runtime = plan.target.f4se
                build = ".".join(map(str, runtime.get("build", []))) or "unknown"
                imgui.separator()
                imgui.text_unformatted("Detected FO4 build: " + build)
                imgui.text_unformatted("Matching F4SE: " + runtime.get("version", "unavailable"))
                imgui.text_wrapped(runtime.get("reason", ""))
                imgui.text_wrapped("Use folder saves this local target, then prepares missing F4SE. An offline or entitlement failure leaves a saved, unready target for retry. No game is started by setup.")
            if imgui.collapsing_header("Cached archive or Nexus browser link"):
                imgui.text_wrapped("Legacy 1.10.163 uses the official direct download. Newer releases use Nexus; Premium can prepare directly with the connected session account. Free accounts need the exact official file's Mod Manager Download link. No entitlement is bypassed.")
                imgui.set_next_item_width(-1)
                _, self.archive = imgui.input_text("Cached official archive##f4se_archive", self.archive)
                if action_button("Browse F4SE archive"):
                    value = pick_file("Choose official F4SE archive", [("Archives", "*.7z;*.zip")])
                    if value:
                        self.archive = str(value)
                imgui.set_next_item_width(-1)
                _, self.nxm = imgui.input_text("Session browser link##f4se_nxm", self.nxm, imgui.InputTextFlags_.password)
        finally:
            imgui.end_disabled()
