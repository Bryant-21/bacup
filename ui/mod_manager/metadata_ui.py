"""Named, frozen display-title and owned-removal dialogs."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button, heading

from creation_lib.mod_manager.paths import ManagerError


class MetadataUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.mod_id, self.title = "", ""
        self.remove_ack, self.remove_token = False, ""
        self.removing = False
        self.reference_ack, self.reference_token = False, ""
        self.field, self.value, self.field_token = "", "", ""
        self.field_saving = False
        self.group_path, self.group_token, self.group_saving = "", "", False
        self.empty_name, self.empty_after, self.empty_saving = "", "", False

    def new_mod(self, after=""):
        self.empty_name, self.empty_after, self.empty_saving = "", after, False
        self.workspace._dialog = "Create empty mod"

    def draw_empty(self):
        w, m = self.workspace, self.workspace.manager
        if self.empty_saving and not m.busy and m.job_state == "completed":
            w._selected_mods = {m.selected_id}
            w._tree_scroll_selected = True
            w.close_workflow()
            return
        heading("Create empty mod")
        imgui.text_wrapped("Add an empty folder to your library. Open it to add loose files or plugins.")
        anchor = m.rows_by_id.get(self.empty_after)
        imgui.text_disabled("Insert below: " + anchor.name if anchor else "Insert at the end of the library")
        imgui.set_next_item_width(-1)
        if imgui.is_window_appearing():
            imgui.set_keyboard_focus_here()
        _, self.empty_name = imgui.input_text_with_hint("##empty_mod_name", "Mod name", self.empty_name)
        if action_button("Create mod", primary=True, enabled=bool(self.empty_name.strip()) and not m.busy and not m.runtime_active):
            w._action(lambda: m.create_empty_mod(self.empty_name, self.empty_after))
            self.empty_saving = m.busy
        imgui.same_line()
        if action_button("Cancel", enabled=not m.busy):
            w.close_workflow()

    def rename_group(self, group):
        self.group_path, self.group_token, self.group_saving = group, "", False
        self.workspace.manager.inspect_group_name(group)
        self.workspace._tab = "Rename group"

    def draw_group(self):
        w, m = self.workspace, self.workspace.manager
        heading("Rename local organization group", large=True)
        imgui.text_wrapped("This moves the group and every nested descendant in this profile. Mod titles, source folders and other profiles stay intact. Undo restores the previous hierarchy.")
        plan = m.group_review
        if not plan:
            if self.group_saving and not m.busy and m.job_state == "completed":
                w._dialog = ""
            else:
                imgui.text_disabled("Reviewing hierarchy..." if m.busy else "Reopen the group to review it again.")
            return
        if self.group_token != plan.expected:
            self.group_token, self.group_path = plan.expected, plan.original
        imgui.text_wrapped(f"{plan.original}: {plan.affected} profile entries including descendants")
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.set_next_item_width(-1)
        _, self.group_path = imgui.input_text("##local_group_path", self.group_path)
        imgui.end_disabled()
        if action_button("Rename reviewed group", primary=True, enabled=not m.busy and not m.runtime_active):
            w._action(lambda: m.rename_group(self.group_path))
            self.group_saving = m.busy
        imgui.same_line()
        if action_button("Cancel group rename", enabled=not m.busy):
            m.group_review, w._dialog = None, ""

    def edit_field(self, mod_id, field):
        w, m = self.workspace, self.workspace.manager
        m.selected_id = mod_id
        if field == "name":
            self.rename()
            return
        if field == "status":
            w._right_pane = "Information"
            return
        self.mod_id, self.field, self.field_token = mod_id, field, ""
        self.field_saving = False
        m.inspect_mod_field(mod_id, field)
        w._tab = "Edit mod field"

    def draw_field(self):
        w, m = self.workspace, self.workspace.manager
        heading("Edit " + self.field.title(), large=True)
        imgui.text_wrapped("Priority applies to this local profile; metadata fields are shared across local profiles. These edits survive repeated MO2 imports. Original source files stay intact.")
        if self.field == "nexus":
            imgui.text_wrapped("Changing the mod ID clears the old file ID to avoid attributing an installed file to a different mod.")
        if self.field == "category":
            imgui.text_wrapped("Enter one category path per line, at most 32. The first is primary; an empty value removes memberships.")
        plan = m.field_review
        if not plan:
            if self.field_saving and not m.busy and m.job_state == "completed":
                w._dialog = ""
            else:
                imgui.text_disabled("Reviewing local field..." if m.busy else "Reopen editing to review the current target.")
            return
        if plan.expected != self.field_token:
            self.field_token, self.value = plan.expected, plan.value
        imgui.begin_child("field_edit_body", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            imgui.set_next_item_width(-1)
            imgui.begin_disabled(m.busy or m.runtime_active)
            if self.field in {"category", "notes"}:
                _, self.value = imgui.input_text_multiline("##mod_field_value", self.value, (0, max(80., imgui.get_content_region_avail().y - 10)))
            else:
                _, self.value = imgui.input_text("##mod_field_value", self.value)
            imgui.end_disabled()
        finally:
            imgui.end_child()
        if action_button("Save local field", primary=True, enabled=not m.busy and not m.runtime_active):
            w._action(lambda: m.edit_mod_field(self.value))
            self.field_saving = m.busy
        imgui.same_line()
        if action_button("Cancel field edit", enabled=not m.busy):
            m.field_review, w._dialog = None, ""

    def rename(self):
        w, m = self.workspace, self.workspace.manager
        row = m.rows_by_id.get(m.selected_id)
        if not row or row.kind not in {"external", "owned"}:
            raise ManagerError("Choose an owned or referenced mod display title.")
        self.mod_id, self.title = row.id, row.name
        m.inspect_mod_name(row.id)
        w._tab = "Rename mod title"

    def remove(self):
        w, m = self.workspace, self.workspace.manager
        row = m.rows_by_id.get(m.selected_id)
        if not row or row.kind != "owned":
            raise ManagerError("Delete applies to owned mods only. Referenced source folders remain intact; disable the reference locally instead.")
        self.mod_id, self.remove_ack, self.remove_token = row.id, False, ""
        self.removing = False
        m.review_owned(row.id)
        w._tab = "Remove owned mod"

    def remove_references(self, selected=None):
        w, m = self.workspace, self.workspace.manager
        selected = tuple(selected) if selected is not None else (m.selected_id,)
        self.reference_ack, self.reference_token, self.removing = False, "", False
        m.review_reference_removal(selected)
        w._tab = "Remove references"

    def draw_reference_removal(self):
        w, m = self.workspace, self.workspace.manager
        heading("Remove local profile entries", large=True)
        plan = m.reference_review
        if not plan:
            if self.removing and not m.busy and m.job_state == "completed":
                w._dialog = ""
            else:
                imgui.text_wrapped("Reviewing selected references/separators..." if m.busy else "No valid review is available. Return to Library and review again.")
            return
        if self.reference_token != plan.expected:
            self.reference_token, self.reference_ack = plan.expected, False
        imgui.text_wrapped(f"{len(plan.mod_ids)} entries in {plan.profile_name}")
        imgui.text_wrapped("This removes their membership in this local profile. Source folders and other profiles stay intact. Repeated imports preserve the removal. Undo restores these entries while no later edit has changed the profile.")
        imgui.begin_child("reference_remove_details", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            clipper = imgui.ListClipper()
            clipper.begin(len(plan.mod_ids))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    imgui.text_unformatted(plan.names[index] + " | " + plan.kinds[index])
        finally:
            imgui.end_child()
        _, self.reference_ack = imgui.checkbox("Remove these reviewed profile entries; retain source content", self.reference_ack)
        if action_button("Remove reviewed references", primary=True, enabled=self.reference_ack and not m.busy and not m.runtime_active):
            w._action(m.remove_references)
            self.removing = m.busy
        imgui.same_line()
        if action_button("Cancel reference removal", enabled=not m.busy):
            m.reference_review, w._dialog = None, ""

    def draw_rename(self):
        w, m = self.workspace, self.workspace.manager
        heading("Rename shared display title", large=True)
        imgui.text_wrapped("This changes the mod's display title across local profiles. Identity, source directory, payloads and original MO2 metadata remain intact.")
        plan = m.name_review
        if not plan or plan.mod_id != self.mod_id:
            row = m.rows_by_id.get(self.mod_id)
            if not m.busy and m.job_state == "completed" and row and row.name == self.title.strip():
                w._dialog = ""
                return
            imgui.text_disabled("Reviewing the frozen local mod target..." if m.busy else "No reviewed title is available. Return to Library and reopen editing.")
            return
        imgui.text_wrapped("Current: " + plan.name)
        imgui.set_next_item_width(-1)
        imgui.begin_disabled(m.busy or m.runtime_active)
        _, self.title = imgui.input_text("##mod_display_title", self.title)
        imgui.end_disabled()
        if action_button("Save display title", primary=True, enabled=not m.busy and not m.runtime_active and bool(self.title.strip())):
            w._action(lambda: m.rename_mod_name(self.title))
        imgui.same_line()
        if action_button("Cancel title edit", enabled=not m.busy):
            m.name_review, w._dialog = None, ""

    def draw_remove(self):
        w, m = self.workspace, self.workspace.manager
        heading("Remove owned mod with recovery", large=True)
        review = m.lifecycle_review
        if not review or review.mod_id != self.mod_id or m.restore_preview is not None:
            if self.removing and not m.busy and m.job_state == "completed":
                w._dialog = ""
                return
            imgui.text_disabled("Reviewing the frozen owned mod target..." if m.busy else "No owned review is available. Return to Library and review again.")
            return
        if self.remove_token != review.token:
            self.remove_token, self.remove_ack = review.token, False
        imgui.text_wrapped(review.name)
        imgui.text_wrapped("Remove this owned mod from every affected local profile and retain its complete folder in Recovery. Source MO2 folders and game directories stay intact.")
        imgui.begin_child("owned_remove_details", (0, max(imgui.get_text_line_height_with_spacing(), imgui.get_content_region_avail().y - imgui.get_frame_height_with_spacing() * 3)))
        try:
            w._show_owned_review(review)
        finally:
            imgui.end_child()
        _, self.remove_ack = imgui.checkbox("Keep complete recovery content and remove this owned mod", self.remove_ack)
        if action_button("Remove reviewed owned mod", primary=True, enabled=self.remove_ack and not m.busy and not m.runtime_active):
            w._action(m.uninstall)
            self.removing = m.busy
        imgui.same_line()
        if action_button("Cancel mod removal", enabled=not m.busy):
            m.lifecycle_review, w._dialog = None, ""
