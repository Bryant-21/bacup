"""Loading mask drawn while a runner is active."""
from __future__ import annotations

from imgui_bundle import imgui


def draw_runner_overlay(
    title: str,
    message: str,
    progress_fraction: float | None,
) -> None:
    """Draw a centered loading panel with optional progress."""
    win_pos = imgui.get_window_pos()
    win_size = imgui.get_window_size()
    draw_list = imgui.get_foreground_draw_list()

    bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.0, 0.0, 0.0, 0.58))
    draw_list.add_rect_filled(
        imgui.ImVec2(win_pos.x, win_pos.y),
        imgui.ImVec2(win_pos.x + win_size.x, win_pos.y + win_size.y),
        bg_col,
    )

    fraction = (
        max(0.0, min(progress_fraction, 1.0))
        if progress_fraction is not None
        else None
    )
    spinner = ["|", "/", "-", "\\"][int(imgui.get_time() * 8) % 4]
    title_text = f"{spinner}  {title}"
    msg_text = message or "Working..."
    text_width = max(
        imgui.calc_text_size(title_text).x,
        imgui.calc_text_size(msg_text).x,
        360.0,
    )

    pad = 16.0
    row_gap = 8.0
    line_height = imgui.get_text_line_height()
    bar_height = 10.0
    panel_width = min(max(text_width + pad * 2, 420.0), max(260.0, win_size.x - 64.0))
    panel_height = pad * 2 + line_height * 2 + row_gap
    if fraction is not None:
        panel_height += bar_height + row_gap * 2
    panel_min = imgui.ImVec2(
        win_pos.x + (win_size.x - panel_width) * 0.5,
        win_pos.y + (win_size.y - panel_height) * 0.5,
    )
    panel_max = imgui.ImVec2(panel_min.x + panel_width, panel_min.y + panel_height)

    panel_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.14, 0.14, 0.16, 1.0))
    border_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.36, 0.36, 0.42, 1.0))
    text_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.92, 0.92, 0.94, 1.0))
    muted_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.68, 0.68, 0.72, 1.0))
    bar_bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.24, 0.24, 0.28, 1.0))
    bar_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.30, 0.62, 0.88, 1.0))

    draw_list.add_rect_filled(panel_min, panel_max, panel_col, 6.0)
    draw_list.add_rect(panel_min, panel_max, border_col, 6.0)

    x = panel_min.x + pad
    y = panel_min.y + pad
    draw_list.add_text(imgui.ImVec2(x, y), text_col, title_text)
    y += line_height + row_gap
    draw_list.add_text(imgui.ImVec2(x, y), muted_col, msg_text)
    if fraction is not None:
        pct = int(round(fraction * 100))
        y += line_height + row_gap
        bar_min = imgui.ImVec2(x, y)
        bar_max = imgui.ImVec2(panel_max.x - pad, y + bar_height)
        fill_max = imgui.ImVec2(
            bar_min.x + (bar_max.x - bar_min.x) * fraction,
            bar_max.y,
        )
        draw_list.add_rect_filled(bar_min, bar_max, bar_bg_col, 3.0)
        draw_list.add_rect_filled(bar_min, fill_max, bar_col, 3.0)
        y += bar_height + row_gap
        draw_list.add_text(imgui.ImVec2(x, y), muted_col, f"{pct}%")
