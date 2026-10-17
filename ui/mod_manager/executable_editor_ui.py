"""Visible MO2-style executable list and transactional local editing draft."""
from dataclasses import replace
import uuid

from imgui_bundle import imgui
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.forms import pick_file
from creation_lib.ui.widgets.modern import action_button, scaled

from creation_lib.mod_manager.executable_catalog import instance, preferences, snapshot
from creation_lib.mod_manager.launcher import Executable, MODES


class ExecutableEditor:
    def __init__(self, launcher):
        self.launcher = launcher
        self.items, self.selected_id, self.default = [], "", ""
        self.expected, self.instance_id, self.owner = "", "", None

    def open(self, selected=None):
        m = self.launcher.manager
        self.owner = m.library
        self.instance_id = instance(m)
        self.items = list(m.executables)
        self.expected = snapshot(m.library, self.instance_id) if m.library else ""
        self.default = preferences(m.library, self.instance_id)["default"] if m.library else ""
        match = next((item for item in self.items if selected and (item.id == selected.id or item.binary.casefold() == selected.binary.casefold())), None)
        self.select(match.id if match else self.items[0].id if self.items else "")

    def select(self, identifier):
        self.selected_id = identifier
        item = next((item for item in self.items if item.id == identifier), None)
        if item:
            for field, attribute in (("title", "title"), ("binary", "binary"), ("arguments", "arguments"), ("working_directory", "cwd"), ("mode", "mode"), ("hidden", "hidden"), ("steam_app_id", "steam_app_id")):
                setattr(self.launcher, attribute, getattr(item, field))
            self.launcher.edit_id = identifier

    def capture(self):
        launcher = self.launcher
        self.items = [replace(item, title=launcher.title, binary=launcher.binary, arguments=launcher.arguments, working_directory=launcher.cwd,
                              mode=launcher.mode, hidden=launcher.hidden, steam_app_id=launcher.steam_app_id) if item.id == self.selected_id else item for item in self.items]

    def draw(self):
        launcher, m, w = self.launcher, self.launcher.manager, self.launcher.workspace
        if self.owner is not m.library:
            self.open(launcher.selected())
        imgui.text_wrapped("Edit a local copy of your executable configurations. Save commits the entire list, order and default; Cancel discards this draft.")
        available = imgui.get_content_region_avail()
        height = max(scaled(100), available.y - imgui.get_frame_height_with_spacing() - scaled(12))
        left = min(scaled(280), max(scaled(190), available.x * .29))
        imgui.begin_child("executable_list", (left, height), imgui.ChildFlags_.borders)
        try:
            imgui.text_unformatted("Executables")
            imgui.begin_child("executable_entries", (0, -imgui.get_frame_height_with_spacing() * 3))
            try:
                for item in self.items:
                    imgui.push_id(item.id)
                    suffix = "  (default)" if item.id == self.default else "  (hidden)" if item.hidden else ""
                    if imgui.selectable(item.title + suffix + "###entry", item.id == self.selected_id, size=(0, imgui.get_frame_height()))[0]:
                        self.capture()
                        self.select(item.id)
                    imgui.set_item_tooltip(item.binary)
                    imgui.pop_id()
            finally:
                imgui.end_child()
            if action_button("Add"):
                self.capture()
                item = Executable(uuid.uuid4().hex, "New executable", "", instance_id=self.instance_id)
                self.items.append(item)
                self.select(item.id)
            imgui.same_line()
            if action_button("Remove", enabled=bool(self.selected_id)):
                self.items = [item for item in self.items if item.id != self.selected_id]
                if self.default == self.selected_id:
                    self.default = ""
                self.select(self.items[0].id if self.items else "")
            position = next((i for i, item in enumerate(self.items) if item.id == self.selected_id), -1)
            for delta, label in ((-1, "Move up"), (1, "Move down")):
                if delta == 1:
                    imgui.same_line()
                if action_button(label, enabled=position >= 0 and 0 <= position + delta < len(self.items)):
                    self.capture()
                    self.items[position], self.items[position + delta] = self.items[position + delta], self.items[position]
            if action_button("Set as default", enabled=bool(self.selected_id) and not launcher.hidden):
                self.default = self.selected_id
        finally:
            imgui.end_child()
        imgui.same_line()
        imgui.begin_child("executable_fields", (0, height), imgui.ChildFlags_.borders)
        try:
            if self.selected_id:
                for label, attribute in (("Title", "title"), ("Executable", "binary"), ("Working directory", "cwd"), ("Arguments", "arguments"), ("Steam application ID (optional)", "steam_app_id")):
                    imgui.text_unformatted(label)
                    browse = attribute in {"binary", "cwd"}
                    imgui.set_next_item_width(max(scaled(80), imgui.get_content_region_avail().x - scaled(94)) if browse else -1)
                    _, value = imgui.input_text("##exe_" + attribute, getattr(launcher, attribute))
                    setattr(launcher, attribute, value)
                    if browse:
                        imgui.same_line()
                        if action_button("Browse##" + attribute):
                            value = pick_file("Choose executable", [("Windows executable", "*.exe")]) if attribute == "binary" else pick_folder("Choose working directory", getattr(launcher, attribute))
                            if value:
                                setattr(launcher, attribute, str(value))
                imgui.text_unformatted("Launch mode")
                imgui.set_next_item_width(-1)
                changed, position = imgui.combo("##exe_mode", list(MODES).index(launcher.mode), list(MODES.values()))
                if changed:
                    launcher.mode = list(MODES)[position]
                changed, launcher.hidden = imgui.checkbox("Hide from launcher", launcher.hidden)
                if changed and launcher.hidden and self.default == self.selected_id:
                    self.default = ""
                imgui.spacing()
                imgui.text_wrapped("F4SE uses the validated Fallout 4 installation beside its loader. A manager-staged loader uses its explicit launch target. Direct mode uses the normal filesystem.")
                if m.target_id:
                    imgui.text_wrapped("The current launch target keeps its own settings, saves and output. Managed loader packages are configured in FO4 folders.")
            else:
                imgui.text_wrapped("Add an executable to configure its title, paths and arguments.")
        finally:
            imgui.end_child()
        imgui.begin_disabled(m.busy or m.runtime_active)
        if action_button("Save and close", primary=True, enabled=bool(m.library)):
            self.capture()
            def save():
                m.save_executables(self.instance_id, self.expected, self.items, self.default)
                if m.target_id:
                    m.select_target("")
                launcher.selected_id = self.default or self.selected_id
                w._dialog = ""
            w._action(save)
        imgui.end_disabled()
        imgui.same_line()
        if action_button("Cancel"):
            w._dialog = ""
