from pathlib import Path
from unittest.mock import MagicMock, patch

from creation_lib.renderer.scene_renderer import SceneNode
from ui.editor.nif_session import NifRegistry, NifSession


def _make_session(nif_id: str = "main") -> NifSession:
    return NifSession(
        nif_id=nif_id,
        nif=MagicMock(),
        file_path=f"{nif_id}.nif",
        scene_root=SceneNode(name=nif_id, block_id=-1, nif_id=nif_id),
        anim_manager=MagicMock(),
    )


def test_material_reload_invalidates_cache_and_rebuilds_referencing_session(tmp_path):
    from ui.editor.app import NifEditorApp

    material_path = str(tmp_path / "test.bgsm")
    app = NifEditorApp.__new__(NifEditorApp)
    app.registry = NifRegistry()
    app.registry.add_session(_make_session("main"))
    app.registry.add_session(_make_session("child_0"))
    app.material_watcher = MagicMock()
    app.material_watcher.check_reloads.return_value = [material_path]
    app._material_watch_nif_ids = {
        NifEditorApp._watch_key(material_path): {"child_0"},
    }
    app.rebuild_scene_from_nif = MagicMock()
    app._refresh_asset_watchers = MagicMock()
    app.status_text = ""

    with patch(
        "creation_lib.renderer.material_pipeline.invalidate_material_cache"
    ) as invalidate:
        rebuilt = NifEditorApp._handle_material_reloads(app)

    invalidate.assert_called_once_with(material_path)
    app.rebuild_scene_from_nif.assert_called_once_with("child_0")
    app._refresh_asset_watchers.assert_called_once()
    assert rebuilt == ["child_0"]
    assert app.status_text == f"Reloaded material: {Path(material_path).name}"


def test_material_reload_noops_without_changes():
    from ui.editor.app import NifEditorApp

    app = NifEditorApp.__new__(NifEditorApp)
    app.registry = NifRegistry()
    app.registry.add_session(_make_session("main"))
    app.material_watcher = MagicMock()
    app.material_watcher.check_reloads.return_value = []
    app.rebuild_scene_from_nif = MagicMock()

    rebuilt = NifEditorApp._handle_material_reloads(app)

    assert rebuilt == []
    app.rebuild_scene_from_nif.assert_not_called()
