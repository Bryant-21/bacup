"""Standalone launcher for the Material Editor.

Usage: uv run python -m ui.materials [path_to_bgsm_or_bgem]
"""

import sys

from imgui_bundle import hello_imgui, imgui, immapp

from creation_lib.ui.theme.window_chrome import create_runner_params, run_app
from .app import MaterialEditorApp


def main():
    app = MaterialEditorApp()

    # Deferred open from command line
    if len(sys.argv) > 1:
        app._pending_open = sys.argv[1]

    def _on_gui():
        app.draw()
        app.draw_user_guide_window()
        app.process_shortcuts()

    def _on_menus():
        app.draw_standalone_menu()

    params = create_runner_params(
        title="Material Editor",
        width=900,
        height=700,
        gui_fn=_on_gui,
    )
    # Enable menu bar for standalone mode
    params.imgui_window_params.show_menu_bar = True
    params.callbacks.show_menus = _on_menus
    toolbar_opts = hello_imgui.EdgeToolbarOptions()
    toolbar_opts.size_em = 2.5
    params.callbacks.add_edge_toolbar(
        hello_imgui.EdgeToolbarType.top,
        app.draw_toolbar,
        toolbar_opts,
    )
    # Use full-screen window (not dock space) for standalone simplicity
    params.imgui_window_params.default_imgui_window_type = (
        hello_imgui.DefaultImGuiWindowType.provide_full_screen_window
    )

    run_app(params)


if __name__ == "__main__":
    main()
