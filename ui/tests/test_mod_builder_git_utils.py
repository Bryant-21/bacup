import sys
from pathlib import Path
from unittest.mock import patch, MagicMock


def _install_imgui_stub():
    from unittest.mock import MagicMock

    stub = MagicMock()
    for mod in [
        "imgui_bundle",
        "imgui_bundle.imgui",
        "imgui_bundle.hello_imgui",
        "imgui_bundle.immapp",
    ]:
        sys.modules.setdefault(mod, stub)


def _make_app():
    _install_imgui_stub()
    with patch("ui.builder.mod_builder_app.ModBuilderApp._refresh_mods", lambda self: None):
        from ui.builder.mod_builder_app import ModBuilderApp
        app = ModBuilderApp()
    return app


def test_selected_mod_has_git_repo():
    _install_imgui_stub()
    from ui.builder import mod_builder_app as mba

    mod_name = "B21_TestMod"
    app = _make_app()
    app._mod_list = [mod_name]
    app._selected_mod_idx = 0

    with patch.object(mba, "MODS_DIR", "X:\\fake-mods"), patch("ui.builder.mod_builder_app.os.path.isdir") as isdir:
        isdir.side_effect = lambda path: str(path).replace("/", "\\").endswith(f"fake-mods\\{mod_name}\\.git")
        assert app._selected_mod_has_git_repo() is True


def test_git_buttons_call_lib_functions():
    """Verify git buttons use _run_fn with creation_lib.git_ops functions."""
    _install_imgui_stub()

    app = _make_app()
    app._mod_list = ["B21_TestMod"]
    app._selected_mod_idx = 0

    fn_calls = []

    def _capture(target_fn, on_done=None, description=""):
        fn_calls.append(description)

    with patch.object(app, "_run_fn", side_effect=_capture):
        app._on_utils_git_commit()
        app._on_utils_git_pull()
        app._on_utils_git_checkout()

    assert fn_calls == [
        "Git commit B21_TestMod",
        "Git pull B21_TestMod",
        "Git checkout B21_TestMod",
    ]
