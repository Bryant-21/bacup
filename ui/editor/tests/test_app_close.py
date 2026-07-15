"""Tests for NifEditorApp.close_nif()."""
import pytest
from unittest.mock import MagicMock, patch
from ui.editor.nif_session import NifSession, NifRegistry
from creation_lib.renderer.scene_renderer import SceneNode


def _make_session(nif_id="main"):
    nif = MagicMock()
    nif.blocks = []
    anim_mgr = MagicMock()
    anim_mgr._node_cache = {}
    anim_mgr._rest_transforms = {}
    scene_root = SceneNode(name="root", block_id=-1, nif_id=nif_id)
    return NifSession(
        nif_id=nif_id, nif=nif, file_path="test.nif",
        scene_root=scene_root, anim_manager=anim_mgr,
    )


def _make_app_stub():
    """Minimal stub of NifEditorApp with just the attributes close_nif() touches."""
    app = MagicMock()
    app.registry = NifRegistry()
    app.registry.add_session(_make_session("main"))

    app.renderer = MagicMock()
    app.renderer.scene_root = MagicMock()

    app.selection_mgr = MagicMock()
    app.undo_manager = MagicMock()
    app.nif_watcher = MagicMock()
    app._nif_reload_pending = "test.nif"
    app._loading = True
    app._loading_future = MagicMock()
    app.status_text = "some status"
    return app


class _Releasable:
    def __init__(self):
        self.released = False

    def release(self):
        self.released = True


def _make_renderer_with_collision_overlay():
    from creation_lib.renderer.scene_renderer import SceneRenderer

    renderer = SceneRenderer.__new__(SceneRenderer)
    renderer.scene_root = MagicMock()
    renderer._alt_vao_cache = {}
    renderer._fullscreen_vaos = {}
    renderer._shadow_dirty = False
    renderer._collision_dirty = False
    renderer._collision_vbo = _Releasable()
    renderer._collision_color_vbo = _Releasable()
    renderer._collision_vao = _Releasable()
    renderer._collision_num_verts = 6
    renderer.particle_renderer = None
    return renderer


class TestCloseNif:
    def test_clears_registry(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        NifEditorApp.close_nif(app)
        assert len(app.registry.sessions) == 0

    def test_clears_renderer_scene_root(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        NifEditorApp.close_nif(app)
        assert app.renderer.scene_root is None

    def test_clears_selection_and_undo(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        NifEditorApp.close_nif(app)
        app.selection_mgr.clear.assert_called_once()
        app.undo_manager.clear.assert_called_once()

    def test_stops_watcher_and_resets_loading(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        NifEditorApp.close_nif(app)
        app.nif_watcher.stop_watching.assert_called_once()
        assert app._loading is False
        assert app._loading_future is None

    def test_clears_status_text(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        NifEditorApp.close_nif(app)
        assert app.status_text == ""

    def test_clears_pending_reload_prompt(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        NifEditorApp.close_nif(app)
        assert app._nif_reload_pending is None

    def test_no_error_when_no_renderer(self):
        from ui.editor.app import NifEditorApp
        app = _make_app_stub()
        app.renderer = None
        NifEditorApp.close_nif(app)  # must not raise
        assert len(app.registry.sessions) == 0

    def test_clears_collision_overlay_buffers(self):
        from ui.editor.app import NifEditorApp

        app = _make_app_stub()
        app.renderer = _make_renderer_with_collision_overlay()
        old_vbo = app.renderer._collision_vbo
        old_color_vbo = app.renderer._collision_color_vbo
        old_vao = app.renderer._collision_vao

        NifEditorApp.close_nif(app)

        assert old_vbo.released is True
        assert old_color_vbo.released is True
        assert old_vao.released is True
        assert app.renderer._collision_vbo is None
        assert app.renderer._collision_color_vbo is None
        assert app.renderer._collision_vao is None
        assert app.renderer._collision_num_verts == 0


def test_detach_unwatches_child_session():
    from ui.editor.app import NifEditorApp

    app = MagicMock()
    app.registry = NifRegistry()
    app.registry.add_session(_make_session("main"))
    child = _make_session("child_0")
    child.file_path = "child.nif"
    child.parent_nif_id = "main"
    app.registry.add_session(child)
    app.renderer = None
    app.undo_manager = MagicMock()
    app.nif_watcher = MagicMock()
    app.connect_points = MagicMock()

    NifEditorApp.detach_nif(app, "child_0", _force=True)

    app.nif_watcher.unwatch_session.assert_called_once_with(
        "child.nif", registry=app.registry
    )
    assert "child_0" not in app.registry.sessions
