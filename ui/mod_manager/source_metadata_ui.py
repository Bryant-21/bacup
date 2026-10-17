"""Selective source metadata review with an always-visible apply/cancel footer."""
from imgui_bundle import imgui
from creation_lib.ui.widgets import pick_file
from creation_lib.ui.widgets.modern import heading, action_button

from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.source_metadata import LABELS


def _display(field, value):
    if field == "nexus":
        return f"{value[0] or 'No game'} / mod {value[1]} / file {value[2]}"
    if field == "categories":
        return ", ".join(value) or "Uncategorized"
    return str(value) or "Empty"


class SourceMetadataUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.mod_id, self.category_file, self.token = "", "", ""
        self.selected, self.saving = set(), False

    def open(self, *, category_file=""):
        w, m = self.workspace, self.workspace.manager
        row = m.rows_by_id.get(m.selected_id)
        if not row or row.kind != "external":
            raise ManagerError("Choose a referenced mod first.")
        self.mod_id, self.category_file, self.token = row.id, category_file, ""
        self.selected, self.saving = set(), False
        m.source_metadata_review = None
        w._tab = "Refresh source metadata"
        m.review_source_metadata(self.mod_id, category_file)

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        heading("Refresh referenced source metadata", large=True)
        imgui.text_wrapped("Choose fields from meta.ini to refresh shared local metadata. Unchecked fields retain local customizations. Guarded Undo is available afterwards.")
        if self.saving and not m.busy and m.job_state == "completed" and not m.source_metadata_review:
            w._dialog = ""
            return
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.set_next_item_width(-1)
        changed, self.category_file = imgui.input_text_with_hint("##source_category_mapping", "Optional MO2 categories.dat mapping", self.category_file)
        if action_button("Choose category mapping"):
            chosen = pick_file("Choose the source MO2 category mapping", [("MO2 categories", "*.dat")])
            if chosen:
                self.category_file, changed = chosen, True
        if changed:
            m.source_metadata_review = None
            self.selected, self.token = set(), ""
        imgui.same_line()
        if action_button("Review source metadata"):
            self.selected, self.token = set(), ""
            w._action(lambda: m.review_source_metadata(self.mod_id, self.category_file))
        imgui.end_disabled()
        plan = m.source_metadata_review
        imgui.begin_child("source_metadata_review", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            if plan:
                token = plan.expected + repr(plan.inputs)
                if self.token != token:
                    self.token, self.selected = token, set()
                imgui.text_wrapped(f"{plan.name} | {plan.profiles} local profile entries")
                imgui.text_wrapped("Source: " + plan.source)
                imgui.begin_disabled(m.busy or m.runtime_active)
                for field in plan.fields:
                    imgui.push_id(field.name)
                    imgui.begin_disabled(bool(field.issue))
                    changed, chosen = imgui.checkbox(LABELS[field.name] + "###source_field", field.name in self.selected)
                    imgui.end_disabled()
                    if changed and not field.issue:
                        if chosen:
                            self.selected.add(field.name)
                        else:
                            self.selected.discard(field.name)
                    imgui.text_wrapped("Local: " + _display(field.name, field.current))
                    if field.issue:
                        imgui.text_wrapped("Unavailable: " + field.issue)
                    else:
                        imgui.text_wrapped("Source: " + _display(field.name, field.value))
                    if field.warning:
                        imgui.text_wrapped(field.warning)
                    imgui.separator()
                    imgui.pop_id()
                imgui.end_disabled()
            else:
                imgui.text_wrapped("Review the source metadata before choosing fields. Category IDs need an explicit category mapping. Unsupported encoded values remain unavailable.")
        finally:
            imgui.end_child()
        imgui.begin_disabled(m.busy or m.runtime_active)
        imgui.text_disabled(f"{len(self.selected)} fields selected")
        if action_button("Apply selected source fields", primary=True, enabled=bool(plan) and bool(self.selected)):
            w._action(lambda: m.refresh_source_metadata(tuple(self.selected)))
            self.saving = m.busy
        imgui.same_line()
        if action_button("Cancel metadata refresh"):
            m.source_metadata_review, w._dialog = None, ""
        imgui.end_disabled()
