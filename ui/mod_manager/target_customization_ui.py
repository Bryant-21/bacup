"""Focused target options/source/removal reviews with fixed actions, using existing styling."""
from imgui_bundle import imgui
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.modern import action_button, heading

from creation_lib.mod_manager import target_mod_sources, target_options, target_removal, target_preferences


class CustomizationUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.mode, self.plan, self.context = "", None, None
        self.arguments, self.source, self.version = "", "", ""
        self.file_id = ""
        self.enabled, self.ack, self.reset = 0, False, False
        self.restore_id = ""
        self.ini_scope, self.save_scope = 0, 0

    def open(self, mode, restore_id=""):
        m = self.workspace.manager
        self.mode, self.plan, self.ack, self.reset = mode, None, False, False
        self.restore_id = restore_id
        self.context = (m.library.library_id, m.target_id, m.profile_id, m.selected_id)
        if mode == "Arguments":
            self.arguments = target_options.value(m.library, m.target_id)["arguments"]
        elif mode == "Source":
            override = target_mod_sources.records(m.library, m.target_id).get(m.selected_id, {})
            self.source, self.version = override.get("source", ""), override.get("version", "")
            self.file_id = str(override["nexus_file_id"]) if override.get("nexus_file_id") is not None else ""
            self.enabled = {None: 0, True: 1, False: 2}[override.get("enabled")]
        elif mode == "Preferences":
            preferences = target_preferences.value(m.library, m.target_id)
            self.ini_scope, self.save_scope = int(preferences["ini"] == "shared"), int(preferences["saves"] == "shared")
        self.workspace._dialog = "Target customization"

    def review(self):
        m = self.workspace.manager
        library, target_id, profile_id, mod_id = m.library, m.target_id, m.profile_id, m.selected_id
        mode, arguments, source, version, enabled, reset, restore_id = self.mode, self.arguments, self.source, self.version, (None, True, False)[self.enabled], self.reset, self.restore_id
        ini_scope, save_scope = ("target", "shared")[self.ini_scope], ("target", "shared")[self.save_scope]
        from creation_lib.mod_manager.paths import ManagerError
        raw_file_id = self.file_id.strip()
        if mode == "Source" and raw_file_id and (not raw_file_id.isascii() or not raw_file_id.isdecimal() or len(raw_file_id) > 12):
            raise ManagerError("Enter a numeric Nexus file ID, 0 for unknown, or leave it blank.")
        file_id = int(raw_file_id) if raw_file_id else None
        self.plan, self.ack = None, False
        def work(cancel, progress):
            if mode == "Arguments":
                return target_options.review(library, target_id, arguments, cancel)
            if mode == "Source":
                return target_mod_sources.review(library, target_id, profile_id, mod_id, source=source, version=version, enabled=enabled, nexus_file_id=file_id, reset=reset, cancel=cancel)
            if mode == "Preferences":
                return target_preferences.review(library, target_id, profile_id, ini=ini_scope, saves=save_scope, cancel=cancel)
            return target_removal.review(library, restore_id or target_id, restore=bool(restore_id), cancel=cancel)
        m.submit("Reviewing target customization", work, lambda value: setattr(self, "plan", value))

    def apply(self):
        m, plan, mode = self.workspace.manager, self.plan, self.mode
        library = m.library
        def work(cancel, progress):
            if mode == "Arguments":
                return target_options.apply(library, plan, cancel)
            if mode == "Source":
                return target_mod_sources.apply(library, plan, accept_identity=self.ack, cancel=cancel)
            if mode == "Preferences":
                return target_preferences.apply(library, plan, cancel)
            return target_removal.apply(library, plan, cancel)
        def complete(_):
            m.refresh()
            self.plan, self.workspace._dialog = None, ""
            m.status = "Target customization saved. Shared mod payloads and retained state are unchanged; native boot remains unverified."
        m.submit("Applying reviewed target customization", work, complete)

    def draw(self):
        m = self.workspace.manager
        context = (m.library.library_id, m.target_id, m.profile_id, m.selected_id) if m.library else None
        if context != self.context:
            self.plan, self.ack = None, False
            imgui.text_wrapped("Target/profile/selection changed. Reopen customization for the current selection.")
            return
        heading("Restore launch target" if self.restore_id else "Target " + self.mode.lower())
        imgui.begin_child("target_customization_details", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            imgui.begin_disabled(m.busy or m.runtime_active)
            if self.mode == "Arguments":
                imgui.text_wrapped("F4SE options: -v, -waitforclose, -priority above_normal/below_normal/normal/idle/high, -affinity <positive mask>. Other bootstrap/Steam/editor/debugger modes are blocked. Empty clears optional arguments.")
                imgui.text_unformatted("Target arguments")
                imgui.set_next_item_width(-1)
                changed, self.arguments = imgui.input_text("##target_arguments", self.arguments)
                if changed:
                    self.plan = None
            elif self.mode == "Source":
                row = m.rows_by_id.get(m.selected_id)
                imgui.text_wrapped(f"Choose another version of {row.name if row else 'the selected mod'} for this FO4 target. Mod order and organization stay shared. Enable policy applies across this target's logical profiles. Launched programs can modify referenced files.")
                for label, attribute in (("Variant source folder", "source"), ("Variant version label", "version"), ("Target Nexus file ID (blank: inherit shared source; 0: unknown)", "file_id")):
                    imgui.text_unformatted(label)
                    imgui.set_next_item_width(-1)
                    changed, value = imgui.input_text("##target_" + attribute, getattr(self, attribute))
                    setattr(self, attribute, value)
                    if changed:
                        self.plan, self.ack = None, False
                if action_button("Browse target mod source"):
                    path = pick_folder("Choose ordinary local version of this same mod", self.source)
                    if path:
                        self.source, self.plan, self.ack = str(path), None, False
                        self.file_id = "0"
                imgui.text_wrapped("A variant folder never inherits the shared file ID. A positive ID is your declared association with this mod's Nexus page; it does not verify the folder's payload.")
                changed, self.enabled = imgui.combo("Enable policy##target_enabled", self.enabled, ["Use logical profile", "Enabled for target", "Disabled for target"])
                changed_reset, self.reset = imgui.checkbox("Reset source/version/enable to shared defaults", self.reset)
                if changed or changed_reset:
                    self.plan, self.ack = None, False
            elif self.mode == "Preferences":
                imgui.text_wrapped("Use INIs and saves from this target or the shared manager profile. Switching leaves both folders intact. Shared INIs can be edited from Original executables. Plugins, output and logs stay local to the target.")
                changed_ini, self.ini_scope = imgui.combo("INI scope##target_ini_scope", self.ini_scope, ["Target-local INIs", "Shared manager profile INIs"])
                changed_save, self.save_scope = imgui.combo("Save scope##target_save_scope", self.save_scope, ["Target-local saves", "Shared manager profile saves"])
                if changed_ini or changed_save:
                    self.plan = None
                imgui.text_wrapped("Edit target-local Fallout4Prefs.ini for display settings.")
                if action_button("Edit local display INI", enabled=m.target_scopes["ini"] == "target"):
                    self.workspace._profile_filename = "Fallout4Prefs.ini"
                    self.workspace._dialog = "Profile config"
            else:
                imgui.text_wrapped("Restore retained target metadata, then revalidate its game/runtime before Run. No owned/external files are replaced." if self.restore_id else "Removal archives only target metadata. All target INIs, saves, output, F4SE stages and external game/source files remain retained. Restore managed game-root setup first. Removed targets can be restored below FO4 folders; no permanent state cleanup is performed.")
            imgui.end_disabled()
            if self.plan:
                imgui.separator()
                if self.mode == "Arguments":
                    imgui.text_wrapped("Reviewed arguments: " + (self.plan.arguments or "(none)"))
                elif self.mode == "Source":
                    record = self.plan.record
                    policy = "Use logical profile" if record.get("enabled") is None else "Enabled for target" if record["enabled"] else "Disabled for target"
                    imgui.text_wrapped(f"{self.plan.name}\nShared source: {self.plan.shared_source}\nTarget source: {record.get('source') or 'Shared source'}\nVersion: {record.get('version') or 'Shared metadata'}\nEnable: {policy}\n{self.plan.files:,} files / {self.plan.bytes:,} bytes. Identity/size/time inventory is checked again; content equivalence is not certified.")
                    imgui.text_wrapped(f"Target Nexus file ID: {record.get('nexus_file_id') if record.get('nexus_file_id') is not None else 'Unknown for variant; inherited for shared source'}")
                    if not record:
                        imgui.text_unformatted("Reset to shared source, version and logical enable defaults.")
                elif self.mode == "Preferences":
                    imgui.text_wrapped("INIs: " + ("Target-local" if self.plan.record["ini"] == "target" else "Shared manager profile") + "\nSaves: " + ("Target-local" if self.plan.record["saves"] == "target" else "Shared manager profile"))
                else:
                    imgui.text_wrapped(self.plan.name + "\nRetained state: " + self.plan.retained_root)
        finally:
            imgui.end_child()
        if action_button("Review target customization", primary=not self.plan, enabled=not m.busy and not m.runtime_active):
            self.workspace._action(self.review)
        required = bool(self.plan and (self.mode == "Remove" or self.mode == "Source" and self.plan.record.get("source")))
        if required:
            _, self.ack = imgui.checkbox("This folder is the intended version of the same mod" if self.mode == "Source" else "Archive or restore this target metadata; retain all files", self.ack)
        if self.plan and action_button("Apply target customization", primary=True, enabled=not m.busy and not m.runtime_active and (not required or self.ack)):
            self.workspace._action(self.apply)
        imgui.same_line()
        if action_button("Cancel target customization", enabled=not m.busy):
            self.plan, self.workspace._dialog = None, ""
