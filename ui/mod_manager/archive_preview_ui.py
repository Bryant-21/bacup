from imgui_bundle import imgui, immvision
from creation_lib.ui.widgets.modern import action_button, scaled
from creation_lib.mod_manager.archive_index import preview
from creation_lib.mod_manager.paths import ManagerError


class ArchivePreviewUI:
    def __init__(self, workspace):
        self.workspace = workspace
        self.context, self.provider, self.result, self.pixels = None, 0, None, None

    def draw(self, index, member):
        context = id(index), member.path
        if context != self.context:
            self.context, self.provider, self.result, self.pixels = context, 0, None, None
        changed, self.provider = imgui.combo("Preview provider", self.provider,
                                       [f"{a.provider.name} / {a.name}" for a in member.archives])
        if changed:
            self.result, self.pixels = None, None
        archive = member.archives[self.provider]
        provider = self.provider
        m = self.workspace.manager
        if action_button("Preview member", enabled=not m.busy):
            def complete(result):
                if self.context != context or self.provider != provider or m.archive_index is not index:
                    raise ManagerError("Archive selection changed; preview the current selection again.")
                self.result = result
            m.submit("Reading archive member", lambda cancel, progress: preview(archive, member.path, cancel), complete)
        result = self.result
        if result is None:
            imgui.text_wrapped("Select a provider and preview text, images or binary bytes. Files remain unchanged.")
            return
        imgui.text_wrapped(f"{result.source} | {result.bytes:,} bytes | {result.kind}" + (" | truncated" if result.truncated else ""))
        preview_height = max(scaled(40), min(scaled(135), imgui.get_content_region_avail().y - scaled(4)))
        if result.pixels is None:
            imgui.input_text_multiline("##archive_preview", result.text, (-1, preview_height), imgui.InputTextFlags_.read_only)
        else:
            height, width = result.pixels.shape[:2]
            ratio = min(imgui.get_content_region_avail().x / width, preview_height / height)
            immvision.push_color_order_rgb()
            try:
                immvision.image_display("##archive_image", result.pixels,
                                        (max(1, int(width * ratio)), max(1, int(height * ratio))),
                                        refresh_image=self.pixels is not result.pixels)
                self.pixels = result.pixels
            finally:
                immvision.pop_color_order()
