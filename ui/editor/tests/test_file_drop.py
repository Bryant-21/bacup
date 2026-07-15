from types import SimpleNamespace
from unittest.mock import MagicMock


def _make_app_for_drop():
    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    app._viewport_pos = SimpleNamespace(x=10.0, y=20.0)
    app._viewport_size = SimpleNamespace(x=200.0, y=100.0)
    app.load_nif = MagicMock()
    app.status_text = ""
    return app


def test_viewport_drop_opens_first_nif(tmp_path):
    from ui.editor.app import NifEditorApp

    app = _make_app_for_drop()
    nif_path = tmp_path / "weapon.nif"

    handled = NifEditorApp.handle_file_drop(app, [str(nif_path)], x=30.0, y=40.0)

    assert handled is True
    app.load_nif.assert_called_once_with(str(nif_path.resolve(strict=False)))


def test_viewport_drop_opens_bto_and_btr(tmp_path):
    from ui.editor.app import NifEditorApp

    for filename in ("tree.BTO", "landscape.btr"):
        app = _make_app_for_drop()
        path = tmp_path / filename

        handled = NifEditorApp.handle_file_drop(app, [str(path)], x=30.0, y=40.0)

        assert handled is True
        app.load_nif.assert_called_once_with(str(path.resolve(strict=False)))


def test_viewport_drop_ignores_non_nif(tmp_path):
    from ui.editor.app import NifEditorApp

    app = _make_app_for_drop()
    txt_path = tmp_path / "notes.txt"

    handled = NifEditorApp.handle_file_drop(app, [str(txt_path)], x=30.0, y=40.0)

    assert handled is False
    app.load_nif.assert_not_called()
    assert app.status_text == "Drop a .nif, .bto, or .btr file to open it"


def test_viewport_drop_ignores_files_outside_viewport(tmp_path):
    from ui.editor.app import NifEditorApp

    app = _make_app_for_drop()
    nif_path = tmp_path / "weapon.nif"

    handled = NifEditorApp.handle_file_drop(app, [str(nif_path)], x=500.0, y=40.0)

    assert handled is False
    app.load_nif.assert_not_called()
