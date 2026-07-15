"""Tests for async attach: PreparedAttachData, prepare_attach_data, _poll_attaching."""

import pytest
from unittest.mock import MagicMock, patch
from concurrent.futures import Future


class TestPreparedAttachData:
    def test_instantiation(self):
        from creation_lib.renderer.nif_loader import PreparedAttachData, PreparedNifData

        prepared = MagicMock(spec=PreparedNifData)
        data = PreparedAttachData(
            prepared=prepared,
            matched_cp="P-Barrel",
            parent_nif_id="main",
        )
        assert data.prepared is prepared
        assert data.matched_cp == "P-Barrel"
        assert data.parent_nif_id == "main"


def _make_mock_nif(child_cp_names=None):
    """Return a mock NifFile with BSConnectPoint::Children blocks."""
    mock_nif = MagicMock()
    if child_cp_names:
        block = MagicMock()
        block.type_name = "BSConnectPoint::Children"
        block.get_field.side_effect = lambda k: (
            child_cp_names if k == "Point Name" else None
        )
        mock_nif.blocks = [block]
    else:
        mock_nif.blocks = []
    return mock_nif


class TestPrepareAttachData:
    def _call(self, child_cp_names, parent_cp_names, occupied_cps=None):
        from creation_lib.renderer.nif_loader import prepare_attach_data

        mock_nif = _make_mock_nif(child_cp_names)
        mock_prepared = MagicMock()
        mock_prepared.nif = mock_nif
        mock_prepared.nif_id = "child_0"

        with patch("ui.editor.nif_loader.prepare_nif_data", return_value=mock_prepared):
            return prepare_attach_data(
                filepath="child.nif",
                texture_dirs=[],
                ba2_mgr=None,
                nif_id="child_0",
                parent_cp_names=set(parent_cp_names),
                occupied_cps=set(occupied_cps or []),
            )

    def test_raises_when_no_child_cps(self):
        with pytest.raises(ValueError, match="no child connect points"):
            self._call(child_cp_names=[], parent_cp_names=["P-Barrel"])

    def test_raises_when_no_match(self):
        with pytest.raises(ValueError, match="no matching parent CPs"):
            self._call(child_cp_names=["C-Barrel"], parent_cp_names=["P-Scope"])

    def test_raises_when_cp_occupied(self):
        with pytest.raises(ValueError, match="already attached"):
            self._call(
                child_cp_names=["C-Barrel"],
                parent_cp_names=["P-Barrel"],
                occupied_cps=["P-Barrel"],
            )

    def test_returns_prepared_attach_data_on_success(self):
        from creation_lib.renderer.nif_loader import PreparedAttachData

        result = self._call(
            child_cp_names=["C-Barrel"],
            parent_cp_names=["P-Barrel"],
        )
        assert isinstance(result, PreparedAttachData)
        assert result.matched_cp == "P-Barrel"
        assert result.parent_nif_id == "main"

    def test_c_prefix_converted_to_p_for_matching(self):
        result = self._call(
            child_cp_names=["C-Scope"],
            parent_cp_names=["P-Scope"],
        )
        assert result.matched_cp == "P-Scope"

    def test_no_prefix_gets_p_prefix(self):
        """CP names without C- prefix get P- prefix added."""
        result = self._call(
            child_cp_names=["Barrel"],
            parent_cp_names=["P-Barrel"],
        )
        assert result.matched_cp == "P-Barrel"


class TestPollAttaching:
    def _make_app(self):
        with (
            patch("ui.editor.app.NifFileWatcher"),
            patch("ui.editor.app.ConnectPointDisplay"),
            patch("ui.editor.app.LightDisplay"),
        ):
            from ui.editor.app import NifEditorApp

            app = NifEditorApp.__new__(NifEditorApp)
            app._attaching = True
            app._attach_future = MagicMock(spec=Future)
            app._attach_filename = "scope.nif"
            app.status_text = ""
            app.renderer = MagicMock()
            app.renderer.programs = {"default": MagicMock(), "fo4": MagicMock()}
            app.renderer.scene_root = MagicMock()
            app.registry = MagicMock()
            app.registry.get_session.return_value = MagicMock()
            app.ctx = MagicMock()
            app.connect_points = MagicMock()
            app.selection_mgr = MagicMock()
            return app

    def test_does_nothing_when_not_attaching(self):
        app = self._make_app()
        app._attaching = False
        app._attach_future = MagicMock()
        app._poll_attaching()
        app._attach_future.done.assert_not_called()

    def test_does_nothing_when_future_not_done(self):
        app = self._make_app()
        app._attach_future.done.return_value = False
        app._poll_attaching()
        assert app._attaching is True  # still attaching

    def test_error_clears_attaching_and_sets_status(self):
        app = self._make_app()
        app._attach_future.done.return_value = True
        app._attach_future.result.side_effect = ValueError("no child connect points")
        app._poll_attaching()
        assert app._attaching is False
        assert app._attach_future is None
        assert "Attach error" in app.status_text

    def test_success_clears_attaching_and_sets_status(self):
        from creation_lib.renderer.nif_loader import PreparedAttachData, PreparedNifData

        app = self._make_app()

        mock_prepared_nif = MagicMock(spec=PreparedNifData)
        mock_prepared_nif.nif_id = "child_0"
        mock_prepared_nif.filepath = "scope.nif"
        mock_prepared_nif.game_profile = None

        mock_attach_data = MagicMock(spec=PreparedAttachData)
        mock_attach_data.prepared = mock_prepared_nif
        mock_attach_data.matched_cp = "P-Barrel"
        mock_attach_data.parent_nif_id = "main"

        fake_scene_root = MagicMock()
        fake_scene_root.children = []
        fake_nif = MagicMock()
        fake_nif.blocks = []

        app._attach_future.done.return_value = True
        app._attach_future.result.return_value = mock_attach_data

        parent_session = MagicMock()
        parent_session.scene_root = MagicMock()
        parent_session.scene_root.children = []
        parent_session.game_profile = None
        app.registry.get_session.return_value = parent_session

        with (
            patch(
                "ui.editor.nif_loader.upload_nif_to_gpu",
                return_value=(fake_scene_root, fake_nif),
            ),
            patch("ui.editor.app.AnimationManager"),
            patch("ui.editor.app.NifSession"),
            patch("ui.editor.nif_loader._update_world_transforms"),
            patch.object(app, "_rebuild_selection_bounds"),
            patch.object(app, "_find_child_connect_point", return_value=None),
            patch.object(app, "_get_cp_world_transform", return_value=MagicMock()),
        ):
            app._poll_attaching()

        assert app._attaching is False
        assert app._attach_future is None
        assert "Attached" in app.status_text
        assert "scope.nif" in app.status_text


class TestAttachNifAutoAsync:
    def _make_app(self):
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
            return app

    def test_raises_when_no_main_session(self):
        from ui.editor.app import NifEditorApp

        app = self._make_app()
        app.registry.get_session.side_effect = KeyError("main")
        with pytest.raises(ValueError, match="No main NIF loaded"):
            NifEditorApp.attach_nif_auto(app, "child.nif")

    def test_sets_attaching_true_and_submits_future(self):
        from ui.editor.app import NifEditorApp

        app = self._make_app()
        main_session = MagicMock()
        main_session.nif.blocks = []
        app.registry.get_session.return_value = main_session
        app.registry.all_sessions.return_value = []
        app.registry.next_child_id.return_value = "child_0"

        with (
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=[]),
        ):
            NifEditorApp.attach_nif_auto(app, "child.nif")

        assert app._attaching is True
        assert app._attach_future is not None
        assert app._load_executor.submit.called

    def test_does_not_raise_on_submit(self):
        """attach_nif_auto does not raise; errors surface in _poll_attaching."""
        from ui.editor.app import NifEditorApp

        app = self._make_app()
        main_session = MagicMock()
        main_session.nif.blocks = []
        app.registry.get_session.return_value = main_session
        app.registry.all_sessions.return_value = []
        app.registry.next_child_id.return_value = "child_0"

        with (
            patch.object(app, "_detect_game_profile", return_value=None),
            patch.object(app, "_build_texture_dirs", return_value=[]),
        ):
            # Should not raise
            NifEditorApp.attach_nif_auto(app, "child.nif")
