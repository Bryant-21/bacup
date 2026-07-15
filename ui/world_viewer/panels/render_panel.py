from __future__ import annotations

from imgui_bundle import imgui


def draw(app) -> None:
    state = app.state
    changed, output_path = imgui.input_text(
        "Output##world_viewer_render_output",
        state.render_output_path,
    )
    if changed:
        state.render_output_path = output_path

    changed, width = imgui.input_int("Width##world_viewer_render_width", int(state.render_width))
    if changed:
        state.render_width = max(1, width)
    changed, height = imgui.input_int("Height##world_viewer_render_height", int(state.render_height))
    if changed:
        state.render_height = max(1, height)

    if imgui.button("Render Current View##world_viewer_render"):
        app.render_current_view(state.render_output_path)
