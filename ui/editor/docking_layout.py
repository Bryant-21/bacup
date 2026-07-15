"""hello_imgui docking layout — NIF editor windows for toolkit integration."""
from imgui_bundle import hello_imgui


def get_nif_dockable_windows() -> list[hello_imgui.DockableWindow]:
    """Return NIF editor dockable windows with ##nif namespace.

    Used by the toolkit host. Does NOT include shared panels (AI Chat, Log).
    Panels are initially created with gui_function=_noop; the workspace
    binds actual panel draw methods in _bind_dockable_windows() after
    panel instances exist.
    """
    _noop = lambda: None  # noqa: E731

    def _win(label, dock):
        w = hello_imgui.DockableWindow(label_=label, dock_space_name_=dock)
        w.call_begin_end = False
        w.gui_function = _noop
        return w

    return [
        # Left dock — stacked
        _win("Scene Tree##nif", "LeftDock"),
        _win("File Browser##nif", "LeftDock"),
        _win("Properties##nif", "LeftDockBottom"),
        _win("Texture Set Editor##nif", "LeftDockBottom"),
        # Right dock (tabbed, after shared AI Chat)
        _win("Animation Editor##nif", "RightDock"),
        _win("Particle Systems##nif", "RightDock"),
        # Bottom dock (tabbed, after shared Log)
        _win("Skeleton Tools##nif", "BottomDock"),
        _win("Validation##nif", "BottomDock"),
        _win("Batch Operations##nif", "BottomDock"),
        # Center viewport
        _win("Viewport##nif", "MainDockSpace"),
    ]
