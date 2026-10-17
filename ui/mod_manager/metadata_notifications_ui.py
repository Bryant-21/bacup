"""Source-change notices route into the existing selective metadata review."""
from imgui_bundle import imgui
from pathlib import Path
from creation_lib.ui.widgets.modern import action_button

from creation_lib.mod_manager.metadata_observer import dismiss
from .toolbar_ui import _next_button


class MetadataNotificationsUI:
    def __init__(self, workspace):
        self.workspace, self.selected = workspace, None

    def open(self):
        self.selected = None
        self.workspace._dialog = "Source metadata notices"
        self.workspace._tab = "Source metadata notices"

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        observer = m.metadata_observer
        index = observer.index
        imgui.text_wrapped("Source metadata changes never overwrite local customizations automatically. Review individual fields or dismiss a notice. Missing/unavailable files remain explicit. Notices cover references in this library, including other profiles.")
        if observer.error:
            imgui.text_wrapped("Observation paused: " + observer.error)
        if index:
            imgui.text_disabled(f"{index.count} notices | {index.visited}/{len(index.targets)} files reached since refresh; up to 200 listed")
        imgui.begin_child("metadata_notices", (0, -imgui.get_frame_height_with_spacing() * 3))
        try:
            if index:
                for notice in index.notices:
                    imgui.push_id(notice.owner_id + ":" + notice.kind + ":" + notice.path)
                    if imgui.selectable(notice.name + " - " + notice.kind + " " + notice.change, self.selected == notice)[0]:
                        self.selected = notice
                    imgui.set_item_tooltip(notice.path + ("\n" + notice.issue if notice.issue else ""))
                    imgui.pop_id()
        finally:
            imgui.end_child()
        if self.selected:
            notice = self.selected
            imgui.text_unformatted("Source: " + Path(notice.path).name)
            imgui.set_item_tooltip(notice.path)
            if notice.issue:
                imgui.text_wrapped(notice.issue)
            candidate = m.rows_by_id.get(notice.owner_id if notice.kind == "mod" else m.selected_id)
            current = bool(index and notice in index.notices)
            same_source = bool(candidate and (notice.kind != "mod" or Path(candidate.source) / "meta.ini" == Path(notice.path)))
            allowed = bool(current and same_source and candidate.kind == "external" and not m.busy and not m.runtime_active and not notice.issue and notice.change != "removed")
            if action_button("Review changed source fields", primary=True, enabled=allowed):
                m.selected_id = candidate.id
                w._action(lambda: w.source_metadata.open(category_file=notice.path if notice.kind == "categories" else ""))
            imgui.set_item_tooltip("Select a profile containing this referenced mod. A category-map notice reviews the selected referenced mod. Fields start unchecked.")
            _next_button("Dismiss this metadata notice")
            if action_button("Dismiss this metadata notice", enabled=current and not m.busy and not m.runtime_active):
                w._action(lambda: dismiss(m.library, notice))
                observer.due = 0
                self.selected = None
        _next_button("Refresh metadata notices")
        if action_button("Refresh metadata notices", enabled=not m.busy and not m.runtime_active):
            observer.due = 0
        _next_button("Close metadata notices")
        if action_button("Close metadata notices"):
            w._dialog = ""
