"""Tests for NifEditorApp.attach_nif_auto() — async entry point behavior."""

import pytest
from unittest.mock import MagicMock, patch
from concurrent.futures import Future


def _make_app(main_session=None, sessions=None):
    with (
        patch("ui.editor.app.NifFileWatcher"),
        patch("ui.editor.app.ConnectPointDisplay"),
        patch("ui.editor.app.LightDisplay"),
    ):
        from ui.editor.app import NifEditorApp

        app = NifEditorApp.__new__(NifEditorApp)
        app._attaching = False
        app._attach_future = None
        app._attach_filename = ""
        app.status_text = ""
        app.ba2_manager = None
        app.registry = MagicMock()
        app._load_executor = MagicMock()
        app._load_executor.submit.return_value = MagicMock(spec=Future)
        if main_session is not None:
            app.registry.get_session.return_value = main_session
        else:
            app.registry.get_session.side_effect = KeyError("main")
        app.registry.all_sessions.return_value = list(sessions or [])
        app.registry.next_child_id.return_value = "child_0"
        return app


class TestAttachNifAutoGuard:
    def test_raises_when_no_main_nif(self):
        from ui.editor.app import NifEditorApp

        app = _make_app(main_session=None)
        with pytest.raises(ValueError, match="No main NIF loaded"):
            NifEditorApp.attach_nif_auto(app, "child.nif")

    def test_does_not_load_file_before_guard(self):
        """File load moved to background — guard must fire before any NifFile.load."""
        from ui.editor.app import NifEditorApp

        app = _make_app(main_session=None)
        with patch("ui.editor.app.NifFile") as mock_nif_cls:
            with pytest.raises(ValueError):
                NifEditorApp.attach_nif_auto(app, "child.nif")
            mock_nif_cls.load.assert_not_called()


class TestAttachNifAutoSubmits:
    def test_submits_prepare_attach_data_to_executor(self):
        from ui.editor.app import NifEditorApp
        from creation_lib.renderer.nif_loader import prepare_attach_data

        main_session = MagicMock()
        main_session.nif.blocks = []
        app = _make_app(main_session=main_session)

        with (
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=([], [], [])),
        ):
            NifEditorApp.attach_nif_auto(app, "child.nif")

        assert app._load_executor.submit.called
        submit_fn = app._load_executor.submit.call_args[0][0]
        assert submit_fn is prepare_attach_data

    def test_sets_attaching_true(self):
        from ui.editor.app import NifEditorApp

        main_session = MagicMock()
        main_session.nif.blocks = []
        app = _make_app(main_session=main_session)

        with (
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=([], [], [])),
        ):
            NifEditorApp.attach_nif_auto(app, "child.nif")

        assert app._attaching is True
        assert app._attach_future is not None
        assert app._attach_filename == "child.nif"

    def test_sets_status_text_to_attaching(self):
        from ui.editor.app import NifEditorApp

        main_session = MagicMock()
        main_session.nif.blocks = []
        app = _make_app(main_session=main_session)

        with (
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=([], [], [])),
        ):
            NifEditorApp.attach_nif_auto(app, "child.nif")

        assert "Attaching" in app.status_text
        assert "child.nif" in app.status_text
