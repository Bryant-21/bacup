"""Backward-compat shim: re-exports from creation_lib.ui.widgets.user_guide."""
from creation_lib.ui.widgets.user_guide import (  # noqa: F401
    UserGuide,
    UserGuideProvider,
    get_user_guide,
    has_user_guide,
    toggle_user_guide,
    draw_user_guide_menu_item,
    draw_help_menu,
    draw_toolbar_help_button,
    draw_generic_user_guide_window,
    draw_docked_user_guide_window,
)
from creation_lib.ui.widgets import user_guide as _ug  # noqa: F401

# expose module-level names so monkeypatching `user_guide.imgui` still works
from creation_lib.ui.widgets.user_guide import imgui, hello_imgui  # noqa: F401
from imgui_bundle import icons_fontawesome_6 as fa  # noqa: F401
