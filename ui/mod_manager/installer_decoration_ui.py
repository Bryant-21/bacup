"""Render decoded header artwork/title within the existing modern wizard."""
from imgui_bundle import imgui, immvision
from creation_lib.ui.widgets.modern import heading, scaled


def draw(prepared):
    decoration = prepared.fomod.decoration
    pixels = prepared.header_image
    right_of_image = decoration.position == "RightOfImage" and pixels is not None
    available = imgui.get_content_region_avail().x
    if pixels is not None:
        height, width = pixels.shape[:2]
        desired = scaled(max(64, min(240, decoration.height if decoration.height > 0 else 160)))
        desired = min(desired, max(scaled(48), imgui.get_content_region_avail().y * .38))
        maximum_width = available * .42 if right_of_image else available
        ratio = min(maximum_width / width, desired / height)
        immvision.push_color_order_rgb()
        try:
            changed = getattr(draw, "_pixels", None) is not pixels
            draw._pixels = pixels
            immvision.image_display("##installer_header", pixels, (max(1, int(width * ratio)), max(1, int(height * ratio))), refresh_image=changed)
        finally:
            immvision.pop_color_order()
        imgui.set_item_tooltip(decoration.image + " | Author-supplied first-frame artwork")
        if decoration.fade:
            left, right = imgui.get_item_rect_min(), imgui.get_item_rect_max()
            solid = imgui.get_color_u32(imgui.Col_.window_bg)
            transparent = solid & 0x00ffffff
            imgui.get_window_draw_list().add_rect_filled_multi_color((left.x, max(left.y, right.y - scaled(20))),
                                                                  (right.x, right.y), transparent, transparent, solid, solid)
    if right_of_image:
        imgui.same_line()
    colour = decoration.colour
    # A dark/transparent author colour would vanish against the app's dark
    # theme. Fall back to its normal heading while retaining authored metadata.
    readable = colour is not None and max(colour[:3]) >= .35 and colour[3] >= .5
    if decoration.position == "Right":
        text_width = imgui.calc_text_size(prepared.fomod.name).x
        imgui.set_cursor_pos_x(imgui.get_cursor_pos_x() + max(0, available - text_width))
    if readable:
        imgui.push_style_color(imgui.Col_.text, colour)
        try:
            imgui.text_wrapped(prepared.fomod.name)
        finally:
            imgui.pop_style_color()
    else:
        heading(prepared.fomod.name)
    if colour is not None and not readable:
        imgui.set_item_tooltip("The package's title colour was adjusted for readability in this theme.")
