from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from creation_lib.renderer.scene_renderer import SceneNode
from ui.editor.nif_session import NifRegistry


def _make_app_stub():
    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    app.registry = NifRegistry()
    app.renderer = MagicMock()
    app.renderer.programs = {"fo4": MagicMock(), "default": MagicMock()}
    app.ctx = MagicMock()
    app._toolkit_settings = SimpleNamespace(get_active_game=lambda: "fo4")
    app._build_texture_dirs = MagicMock(return_value=([], [], []))
    app._create_ba2_manager = MagicMock(return_value=None)
    app._create_particle_runtime = MagicMock(return_value=([], None))
    app.nif_watcher = MagicMock()
    app.selection_mgr = MagicMock()
    app.undo_manager = MagicMock()
    app.light_display = MagicMock()
    app._refresh_asset_watchers = MagicMock()
    app._nif_reload_pending = "old.nif"
    app.status_text = ""
    return app


def test_new_blank_nif_creates_unsaved_ninode_root_session():
    from ui.editor.app import NifEditorApp

    app = _make_app_stub()
    scene_root = SceneNode(name="nif_root", block_id=-1, nif_id="main")

    with patch(
        "creation_lib.renderer.nif_loader.rebuild_scene_from_nif",
        return_value=scene_root,
    ):
        session = NifEditorApp.new_blank_nif(app, "fo4")

    assert session is app.registry.active_session
    assert session.file_path == "untitled.nif"
    assert session.dirty is True
    assert session.nif.get_block(0).type_name == "NiNode"
    assert session.nif.get_block(0).get_field("Children") == []
    assert app.renderer.scene_root is scene_root
    app.nif_watcher.stop_watching.assert_called_once()
    app.undo_manager.clear.assert_called_once()
    assert app._nif_reload_pending is None


def test_force_root_ninode_preserves_saveable_header(tmp_path):
    from creation_lib.nif.nif_file import NifFile
    from ui.editor.app import NifEditorApp

    nif = NifFile.new("fo4")

    NifEditorApp._force_root_ninode(nif)
    out_path = tmp_path / "blank.nif"
    nif.save(str(out_path))
    reloaded = NifFile.load(str(out_path))

    assert reloaded.get_block(0).type_name == "NiNode"
    assert reloaded.get_block(0).get_field("Children") == []
    assert reloaded.get_hierarchy()["roots"][0]["type"] == "NiNode"
