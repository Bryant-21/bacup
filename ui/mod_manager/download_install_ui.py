"""Explicit download destinations, reusing the normal reviewed installer."""
from pathlib import Path

from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button, heading


class DownloadInstaller:
    def __init__(self, workspace):
        self.workspace, self.selected = workspace, ""

    def open(self, record, *, imported=False):
        w = self.workspace
        self.selected = ""
        w.manager.review_download(record["source" if imported else "path"], imported=imported)
        w._tab = "Download destination"

    def draw(self):
        w, m = self.workspace, self.workspace.manager
        heading("Install downloaded archive")
        review = m.download_review
        if not review:
            imgui.text_wrapped("Reading the archive reference and available local destinations." if m.busy else "Download review is unavailable. Refresh Downloads and open it again.")
            if action_button("Close download review", enabled=not m.busy):
                w._dialog = ""
            return
        imgui.begin_disabled(m.busy or m.runtime_active)
        try:
            imgui.begin_child("download_destinations", (0, -imgui.get_frame_height_with_spacing() * 3))
            try:
                imgui.text_wrapped(Path(review.path).name)
                metadata = review.metadata()
                profile = next((p for p in m.profiles if p.id == review.profile_id), None)
                target = next((t for t in m.targets if t.id == review.target_id), None)
                imgui.text_wrapped(f"Profile: {profile.name if profile else review.profile_id} | FO4 target: {target.name if target else review.target_id or 'None'}\nArchive identity: {metadata.get('game') or 'Unknown'} / mod {metadata.get('mod_id') or 'Unknown'} / file {metadata.get('file_id') or 'Unknown'}")
                imgui.separator()
                clipper = imgui.ListClipper()
                height = imgui.get_text_line_height_with_spacing() * 2
                clipper.begin(len(review.candidates), height)
                while clipper.step():
                    for index in range(clipper.display_start, clipper.display_end):
                        item = review.candidates[index]
                        imgui.push_id(item.id)
                        text_position = imgui.get_cursor_screen_pos()
                        if imgui.selectable("##download_destination", self.selected == item.id)[0]:
                            self.selected = item.id
                        imgui.get_window_draw_list().add_text(text_position, imgui.get_color_u32(imgui.Col_.text), item.name)
                        imgui.set_item_tooltip(item.source + "\nVersion: " + (item.version or "Unknown") + f" | file {item.file_id or 'Unknown'}\n" + item.reason)
                        imgui.text_disabled(item.reason)
                        imgui.pop_id()
                imgui.separator()
                imgui.text_wrapped("Imported Nexus metadata is author/source supplied and unverified." if review.imported else "Owned archive checksum verified.")
                imgui.text_wrapped("Original MO2 archive stays in place. Installation copies only the reviewed payload into this manager." if review.imported else "Completed owned archive is retained. Review installer choices and destinations before applying.")
                imgui.text_wrapped("Reinstall replaces owned content in all profiles with recovery. Additive updates create an owned layer above the selected reference for this profile/target. Neither identity matching nor a published replacement proves payload compatibility.")
            finally:
                imgui.end_child()
            selected = review.by_id.get(self.selected)
            if action_button("Review reinstall", primary=True, enabled=bool(selected and selected.eligible)):
                self.prepare(selected.id)
            imgui.same_line()
            if action_button("Review additive update", primary=True, enabled=bool(selected and selected.overlay_eligible)):
                w._prepare(review.path, download_review=review, overlay_id=selected.id)
            if action_button("Install as a new mod"):
                self.prepare("")
            imgui.same_line()
            if action_button("Cancel download install"):
                m.download_review = None
                self.selected, w._dialog = "", ""
        finally:
            imgui.end_disabled()

    def prepare(self, mod_id):
        w, review = self.workspace, self.workspace.manager.download_review
        w._prepare(review.path, mod_id, download_review=review)
