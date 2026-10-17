"""Small vector status marks: independent of icon fonts and emoji coverage."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import scaled, semantic_color


def draw_badge(draw, badge, x, y, size):
    color = imgui.get_color_u32(semantic_color(badge.role))
    radius, line = size * .32, max(1., scaled(1.4))
    cx, cy = x + size * .5, y + size * .5
    icon = badge.icon
    if "TRIANGLE" in icon:
        draw.add_triangle((cx, cy - radius), (cx - radius, cy + radius), (cx + radius, cy + radius), color, line)
        draw.add_line((cx, cy - radius * .4), (cx, cy + radius * .3), color, line)
    elif "CHECK" in icon:
        draw.add_line((cx - radius, cy), (cx - radius * .2, cy + radius * .7), color, line)
        draw.add_line((cx - radius * .2, cy + radius * .7), (cx + radius, cy - radius * .7), color, line)
    elif "ARROW" in icon:
        direction = -1 if "UP" in icon else 1
        tip = cy + radius * direction
        draw.add_line((cx, cy - radius * direction), (cx, tip), color, line)
        draw.add_line((cx, tip), (cx - radius * .65, tip - radius * .65 * direction), color, line)
        draw.add_line((cx, tip), (cx + radius * .65, tip - radius * .65 * direction), color, line)
    elif "BOLT" in icon:
        draw.add_line((cx + radius * .5, cy - radius), (cx - radius * .5, cy), color, line)
        draw.add_line((cx - radius * .5, cy), (cx + radius * .5, cy), color, line)
        draw.add_line((cx + radius * .5, cy), (cx - radius * .5, cy + radius), color, line)
    elif "NOT_EQUAL" in icon:
        for offset in (-.3, .3):
            draw.add_line((cx - radius, cy + radius * offset), (cx + radius, cy + radius * offset), color, line)
        draw.add_line((cx - radius * .6, cy + radius), (cx + radius * .6, cy - radius), color, line)
    else:
        # Neutral unknown/off/empty states are intentional, with exact text in tooltips.
        draw.add_circle((cx, cy), radius, color, 12, line)
        draw.add_line((cx - radius * .45, cy), (cx + radius * .45, cy), color, line)
        if "SLASH" in icon:
            draw.add_line((cx - radius, cy - radius), (cx + radius, cy + radius), color, line)
