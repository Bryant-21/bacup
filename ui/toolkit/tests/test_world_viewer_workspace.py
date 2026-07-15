from __future__ import annotations

from ui.world_viewer.workspace import WorldViewerWorkspace


class _Scene:
    def __init__(self):
        self.closed = False

    def close(self) -> None:
        self.closed = True

    def stats(self):
        return {"ok": True, "warnings": [], "counts": {}, "data": {}}

    def query_visible(self, *_args):
        return {
            "ok": True,
            "warnings": [],
            "counts": {"visible_instances": 1},
            "timings_ms": {"culling": 0.0},
            "data": {
                "batches": [
                    {
                        "kind": "static",
                        "instance_count": 1,
                        "mesh_buffer": "mesh:static:0",
                        "instance_buffer": "instances:static:0",
                    }
                ]
            },
        }


class _Session:
    def __init__(self, scene=None, fail=False):
        self.scene = scene or _Scene()
        self.fail = fail
        self.closed = False

    def load_worldspace(self, *_args):
        if self.fail:
            raise RuntimeError("load failed")
        return self.scene

    def close(self) -> None:
        self.closed = True


def test_world_viewer_workspace_defaults() -> None:
    workspace = WorldViewerWorkspace()

    assert workspace.id == "world_viewer"
    assert workspace.name == "World Viewer"
    assert workspace.get_settings_defaults()["game"] == "fo4"
    assert workspace.get_settings_defaults()["worldspace"] == ""


def test_world_viewer_uses_toolkit_settings_paths() -> None:
    calls: list[str] = []

    class Settings:
        def get_game_paths(self, game: str):
            calls.append(game)
            return {"game_root": "X:/Games/Fallout4", "data": "X:/Games/Fallout4/Data"}

    workspace = WorldViewerWorkspace(toolkit_settings=Settings())
    workspace.apply_settings({"game": "fo4"})

    assert workspace.resolve_game_paths()["data"] == "X:/Games/Fallout4/Data"
    assert calls == ["fo4"]


def test_world_viewer_preserves_previous_scene_when_load_fails(monkeypatch) -> None:
    workspace = WorldViewerWorkspace()
    workspace.apply_settings({"worldspace": "Commonwealth"})
    old_scene = _Scene()
    old_session = _Session(old_scene)
    failed_session = _Session(fail=True)
    workspace.app._scene = old_scene
    workspace.app._session = old_session

    monkeypatch.setattr(workspace.app, "_create_session", lambda: failed_session)

    assert workspace.app.load_scene() is old_scene
    assert workspace.app._scene is old_scene
    assert workspace.app._session is old_session
    assert old_scene.closed is False
    assert old_session.closed is False
    assert failed_session.closed is True


def test_world_viewer_viewport_summary_queries_loaded_scene() -> None:
    workspace = WorldViewerWorkspace()
    workspace.apply_settings({"worldspace": "Commonwealth"})
    workspace.app._scene = _Scene()

    summary = workspace.app.viewport_summary()

    assert summary["loaded"] is True
    assert summary["counts"]["visible_instances"] == 1
    assert summary["batches"][0]["kind"] == "static"
