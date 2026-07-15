"""Tests for NifEditorApp save behavior."""
from unittest.mock import MagicMock

from creation_lib.renderer.scene_renderer import SceneNode
from ui.editor.nif_session import NifRegistry, NifSession


def test_save_session_marks_path_as_saved(tmp_path):
    from ui.editor.app import NifEditorApp

    path = tmp_path / "saved.nif"
    nif = MagicMock()
    session = NifSession(
        nif_id="main",
        nif=nif,
        file_path=str(path),
        scene_root=SceneNode(name="root", block_id=-1, nif_id="main"),
        anim_manager=MagicMock(),
        dirty=True,
    )
    app = MagicMock()
    app.registry = NifRegistry()
    app.registry.add_session(session)
    app.nif_watcher = MagicMock()
    app.status_text = ""

    NifEditorApp._save_session(app, "main")

    nif.save.assert_called_once_with(str(path))
    app.nif_watcher.mark_saved.assert_called_once_with(str(path))
    assert session.dirty is False
