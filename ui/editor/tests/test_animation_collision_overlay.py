from types import SimpleNamespace

from creation_lib.renderer.scene_renderer import SceneNode
from ui.editor.panels.animation_editor import AnimationEditorPanel
from ui.toolkit.workspaces.nif_workspace import NifWorkspace


def test_animation_tick_marks_collision_overlay_dirty_when_pose_changes(monkeypatch):
    calls = []
    root = SceneNode(name="root", block_id=-1)
    anim_mgr = SimpleNamespace(
        _dirty=False,
        update=lambda dt, scene_root: setattr(anim_mgr, "_dirty", True),
    )
    renderer = SimpleNamespace(_collision_dirty=False)
    workspace = NifWorkspace.__new__(NifWorkspace)
    workspace._app = SimpleNamespace(
        animation_mgr=anim_mgr,
        nif_root=root,
        renderer=renderer,
    )

    monkeypatch.setattr(
        "creation_lib.renderer.nif_loader._update_world_transforms",
        lambda scene_root, parent_world: calls.append((scene_root, parent_world)),
    )

    workspace._update_animation_frame(0.016)

    assert calls and calls[0][0] is root
    assert renderer._collision_dirty is True


def test_animation_editor_scrub_marks_collision_overlay_dirty(monkeypatch):
    calls = []
    root = SceneNode(name="root", block_id=-1)
    renderer = SimpleNamespace(_collision_dirty=False)
    panel = AnimationEditorPanel.__new__(AnimationEditorPanel)
    panel.app = SimpleNamespace(
        registry=SimpleNamespace(sessions={}),
        nif_root=root,
        renderer=renderer,
    )

    monkeypatch.setattr(
        "creation_lib.renderer.nif_loader._update_world_transforms",
        lambda scene_root, parent_world: calls.append((scene_root, parent_world)),
    )

    panel._flush_animation_pose_to_viewport()

    assert calls and calls[0][0] is root
    assert renderer._collision_dirty is True
