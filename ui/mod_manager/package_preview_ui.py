"""Preview a selected source without launching editors or installer scripts."""
from imgui_bundle import imgui, immvision
from creation_lib.ui.widgets.modern import action_button, scaled

from creation_lib.mod_manager.paths import ManagerError


class PackagePreviewUI:
    def __init__(self, workspace):
        self.workspace, self.prepared, self.result, self.pixels = workspace, None, None, None

    def draw(self, source):
        m = self.workspace.manager
        prepared = m.prepared
        if self.prepared is not prepared:
            self.prepared, self.result, self.pixels = prepared, None, None
        if action_button("Preview selected package file", enabled=bool(source) and not m.busy):
            from creation_lib.mod_manager.package_preview import read
            def complete(result):
                if m.prepared is not prepared:
                    raise ManagerError("Package changed while previewing. Select its file again.")
                self.result = result
                m.status = "Read an inert package preview; no external application was launched."
            m.submit("Reading bounded package preview", lambda cancel, progress: read(prepared.package, source, cancel), complete)
        result = self.result
        if result is None:
            return
        imgui.text_wrapped(f"{result.source} | {result.bytes:,} bytes | {result.kind}" + (" | truncated" if result.truncated else ""))
        if result.pixels is not None:
            height, width = result.pixels.shape[:2]
            ratio = min(imgui.get_content_region_avail().x / width, scaled(160) / height)
            immvision.push_color_order_rgb()
            try:
                immvision.image_display("##package_member_preview", result.pixels,
                                        (max(1, int(width * ratio)), max(1, int(height * ratio))),
                                        refresh_image=self.pixels is not result.pixels)
                self.pixels = result.pixels
            finally:
                immvision.pop_color_order()
        else:
            imgui.input_text_multiline("##inert_package_preview", result.text, (-1, scaled(160)), imgui.InputTextFlags_.read_only)
        if action_button("Close package preview"):
            self.result, self.pixels = None, None
