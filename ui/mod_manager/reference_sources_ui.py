"""Fixed-footer review for a shared source pointer, without source file operations."""
from imgui_bundle import imgui
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.modern import heading, action_button

from creation_lib.mod_manager.paths import ManagerError


class SourceUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.mod_id, self.path, self.token = "", "", ""
        self.accept_identity, self.saving = False, False

    def open(self):
        w, m = self.workspace, self.workspace.manager
        row = m.rows_by_id.get(m.selected_id)
        if not row or row.kind != "external":
            raise ManagerError("Choose a referenced source mod first.")
        self.mod_id, self.path, self.token = row.id, row.source, ""
        self.accept_identity, self.saving = False, False
        m.relink_review = None
        w._tab = "Relink source"

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        heading("Relink referenced source folder", large=True)
        imgui.text_wrapped("Choose the same mod at its new location. This updates its shared local source pointer across profiles. Source folders and original MO2 configuration stay intact. A guarded metadata Undo is available afterwards.")
        if self.saving and not m.busy and m.job_state == "completed" and not m.relink_review:
            w._dialog = ""
            return
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.set_next_item_width(-1)
        changed, self.path = imgui.input_text("##reference_source_path", self.path)
        if action_button("Browse reference folder"):
            chosen = pick_folder("Choose the ordinary source folder for this referenced mod", self.path)
            if chosen:
                self.path, changed = chosen, True
        if changed:
            m.relink_review = None
            self.accept_identity, self.token = False, ""
        imgui.same_line()
        if action_button("Review referenced source", enabled=bool(self.path.strip())):
            self.accept_identity, self.token = False, ""
            w._action(lambda: m.review_reference_source(self.mod_id, self.path))
        imgui.end_disabled()
        plan = m.relink_review
        imgui.begin_child("reference_source_review", (0, -imgui.get_frame_height_with_spacing() * 4))
        try:
            if plan:
                if self.token != plan.fingerprint:
                    self.token, self.accept_identity = plan.fingerprint, False
                imgui.text_wrapped(plan.name)
                imgui.text_wrapped("Previous source: " + (plan.old_source or "Unavailable"))
                imgui.text_wrapped("Reviewed source: " + plan.source)
                imgui.text_wrapped(f"{plan.files:,} files, {plan.bytes:,} bytes; shared by {plan.profiles} local profile entries")
                imgui.text_wrapped("Inventory uses names, file identities, sizes and modification times; this is not a filesystem snapshot or a payload-content equivalence check. Confirm this is the same mod before retaining its title/version/Nexus metadata.")
            else:
                imgui.text_wrapped("Review this ordinary folder before changing the source. Nested links and manager-owned libraries are rejected.")
        finally:
            imgui.end_child()
        imgui.begin_disabled(m.busy or m.runtime_active)
        _, self.accept_identity = imgui.checkbox("This folder is the same referenced mod", self.accept_identity)
        if action_button("Apply reviewed source relink", primary=True, enabled=bool(plan) and self.accept_identity):
            w._action(lambda: m.relink_source(accept_identity=self.accept_identity))
            self.saving = m.busy
        imgui.same_line()
        if action_button("Cancel source relink"):
            m.relink_review, w._dialog = None, ""
        imgui.end_disabled()
