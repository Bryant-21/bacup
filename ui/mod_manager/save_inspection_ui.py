"""Bounded save/plugin comparison view with existing native widgets."""
from pathlib import Path

from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button, heading, scaled


def draw(workspace):
    m = workspace.manager
    path = getattr(workspace, "_save_inspect_path", "")
    data = m.save_inspections.get(path)
    heading("Saved plugins")
    imgui.text_wrapped(Path(path).name)
    imgui.begin_child("save_plugin_details", (0, -imgui.get_frame_height_with_spacing() * 4))
    try:
        if not data:
            imgui.text_wrapped("Inspect this save's plugin tables first. Unsupported or damaged metadata produces an explicit error above.")
        else:
            imgui.text_wrapped(f"{data.basic['player']} | Level {data.basic['level']} | {data.basic['location']}")
            imgui.text_disabled(f"Game {data.game_version} | Form {data.form_version} | {data.width} x {data.height} screenshot")
            imgui.text_wrapped(data.basic["coverage"])
            for notice in data.notices:
                imgui.text_wrapped(notice)
            image = m.save_images.get(path)
            if image is not None:
                from imgui_bundle import immvision
                changed = image is not getattr(workspace, "_save_image_seen", None)
                workspace._save_image_seen = image
                height, width = image.shape[:2]
                ratio = min(scaled(384) / width, scaled(220) / height, imgui.get_content_region_avail().x / width, 1)
                immvision.push_color_order_rgb()
                try:
                    immvision.image_display("##save_screenshot", image, (max(1, int(width * ratio)), max(1, int(height * ratio))), refresh_image=changed)
                finally:
                    immvision.pop_color_order()
            comparison = m.save_comparisons.get(path)
            if comparison:
                imgui.separator()
                imgui.text_wrapped("Compared with: " + comparison.target)
                imgui.text_wrapped("Plugin names and indices match." if comparison.same_plugins else "Plugin metadata differs. Review the differences below.")
                for label, values in (("Missing", comparison.missing), ("Disabled", comparison.disabled), ("Added", comparison.added),
                                      ("Changed full/light type", comparison.type_changed), ("Unknown headers", comparison.unknown)):
                    if values:
                        imgui.text_wrapped(label + ": " + ", ".join(values))
                for name, old, new in comparison.moved:
                    imgui.text_unformatted(f"{name}: {old} -> {new}")
                for issue in comparison.issues:
                    imgui.text_wrapped(issue)
            imgui.separator()
            imgui.text_disabled(f"{len(data.regular)} full / {len(data.light)} light plugins")
            entries = [(f"{index:02X}", name) for index, name in enumerate(data.regular)] + [(f"FE:{index:03X}", name) for index, name in enumerate(data.light)]
            clipper = imgui.ListClipper()
            clipper.begin(len(entries))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    address, name = entries[index]
                    imgui.push_id(index)
                    imgui.selectable(address + " | " + name + "###saved_plugin", False)
                    imgui.pop_id()
    finally:
        imgui.end_child()
    choices = [("Current local profile", "")]
    if m.save_index:
        choices.extend((save.name, save.path) for save in m.save_index.files if save.path != path)
    selected = getattr(workspace, "_save_compare_target", "")
    keys = [value for _, value in choices]
    position = keys.index(selected) if selected in keys else 0
    imgui.set_next_item_width(-1)
    changed, position = imgui.combo("##save_compare_target", position, [label for label, _ in choices])
    if changed:
        workspace._save_compare_target = keys[position]
    target = keys[position]
    if action_button("Compare plugins", primary=True, enabled=bool(data) and not m.busy and not m.runtime_active):
        workspace._action(lambda: m.compare_save_to(path, target) if target else m.compare_save_profile(path))
    if imgui.get_content_region_avail().x >= scaled(120):
        imgui.same_line()
    if action_button("Inspect again", enabled=bool(path) and not m.busy and not m.runtime_active):
        workspace._action(lambda: m.inspect_save_plugins(path))
    imgui.same_line()
    if action_button("Close", enabled=not m.busy):
        workspace._tab = ""
    if action_button("Preview screenshot", enabled=bool(data and data.image_offset) and not m.busy and not m.runtime_active):
        workspace._action(lambda: m.preview_save_screenshot(path))
    if path in m.save_images:
        imgui.same_line()
        if action_button("Hide screenshot"):
            m.save_images.pop(path, None)
