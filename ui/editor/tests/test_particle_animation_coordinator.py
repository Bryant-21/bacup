from unittest.mock import MagicMock

from creation_lib.renderer.scene_renderer import SceneNode
from ui.editor.animation_coordinator import AnimationCoordinator
from ui.editor.nif_session import NifRegistry, NifSession
from ui.editor.panels.animation_editor import AnimationEditorPanel
from ui.editor.particles.runtime import PARTICLE_PREVIEW_SEQUENCE
from ui.toolkit.workspaces.nif_workspace import NifWorkspace


def _session(nif_id, anim_sequences, has_particles=True):
    anim_mgr = MagicMock()
    anim_mgr.has_sequence.side_effect = lambda name: name in anim_sequences
    anim_mgr.get_sequences.return_value = list(anim_sequences)
    particle_runtime = MagicMock() if has_particles else None
    if particle_runtime is not None:
        particle_runtime.has_particles = True
    return NifSession(
        nif_id=nif_id,
        nif=MagicMock(),
        file_path=f"{nif_id}.nif",
        scene_root=SceneNode(name="root", block_id=-1, nif_id=nif_id),
        anim_manager=anim_mgr,
        particle_runtime=particle_runtime,
    )


def test_play_sequence_drives_animation_and_particles():
    reg = NifRegistry()
    reg.add_session(_session("main", ["Fire"]))
    coord = AnimationCoordinator(reg)

    coord.play("Fire")

    session = reg.get_session("main")
    session.anim_manager.play.assert_called_once_with("Fire")
    session.particle_runtime.play.assert_called_once_with("Fire")


def test_particle_preview_sequence_does_not_call_animation_manager_play():
    reg = NifRegistry()
    reg.add_session(_session("main", []))
    coord = AnimationCoordinator(reg)

    coord.play(PARTICLE_PREVIEW_SEQUENCE)

    session = reg.get_session("main")
    session.anim_manager.play.assert_not_called()
    session.particle_runtime.play.assert_called_once_with(PARTICLE_PREVIEW_SEQUENCE)
    assert coord.current_sequence_name == PARTICLE_PREVIEW_SEQUENCE


def test_select_particle_preview_reaches_animation_manager_synthetic_branch():
    reg = NifRegistry()
    reg.add_session(_session("main", ["Fire"]))
    coord = AnimationCoordinator(reg)

    coord.select(PARTICLE_PREVIEW_SEQUENCE)

    session = reg.get_session("main")
    session.anim_manager.select_sequence.assert_called_once_with(PARTICLE_PREVIEW_SEQUENCE)
    assert coord.current_sequence_name == PARTICLE_PREVIEW_SEQUENCE


def test_pause_stop_set_time_update_fan_out_to_particles():
    reg = NifRegistry()
    reg.add_session(_session("main", ["Fire"]))
    coord = AnimationCoordinator(reg)

    coord.pause()
    coord.stop()
    coord.set_time(0.75)
    coord.update(0.016)

    runtime = reg.get_session("main").particle_runtime
    runtime.pause.assert_called_once()
    runtime.stop.assert_called_once()
    runtime.set_time.assert_called_once_with(0.75)
    runtime.update.assert_called_once_with(0.016)


def test_workspace_style_update_can_use_animation_coordinator():
    reg = NifRegistry()
    reg.add_session(_session("main", ["Fire"]))
    coord = AnimationCoordinator(reg)

    coord.play("Fire")
    coord.update(0.033)

    session = reg.get_session("main")
    session.anim_manager.update.assert_called_once_with(0.033, session.scene_root)
    session.particle_runtime.update.assert_called_once_with(0.033)


def test_workspace_frame_update_ignores_paused_particle_runtime():
    workspace = NifWorkspace.__new__(NifWorkspace)
    app = MagicMock()
    app.nif_root = None
    runtime = MagicMock()
    runtime.is_playing = True
    runtime.is_paused = True
    session = MagicMock()
    session.anim_manager.is_playing = False
    session.anim_manager._dirty = False
    session.particle_runtime = runtime
    app.registry.all_sessions.return_value = [session]
    workspace._app = app

    status = workspace._update_animation_frame(0.033)

    app.anim_coordinator.update.assert_called_once_with(0.033)
    assert status.is_playing is False


def test_workspace_dirty_update_uses_renderer_scene_root(monkeypatch):
    workspace = NifWorkspace.__new__(NifWorkspace)
    app = MagicMock()
    active_root = object()
    rendered_root = object()
    app.nif_root = active_root
    app.renderer.scene_root = rendered_root
    app.renderer._collision_dirty = False
    session = MagicMock()
    session.anim_manager.is_playing = False
    session.anim_manager._dirty = True
    session.particle_runtime = None
    app.registry.all_sessions.return_value = [session]
    workspace._app = app
    update_world_transforms = MagicMock()
    monkeypatch.setattr(
        "creation_lib.renderer.nif_loader._update_world_transforms",
        update_world_transforms,
    )

    status = workspace._update_animation_frame(0.033)

    app.anim_coordinator.update.assert_called_once_with(0.033)
    assert status.is_playing is False
    assert update_world_transforms.call_args.args[0] is rendered_root
    assert app.renderer._collision_dirty is True


def test_get_all_sequences_includes_particle_preview_when_any_session_has_particles():
    reg = NifRegistry()
    reg.add_session(_session("main", []))
    coord = AnimationCoordinator(reg)

    sequences = coord.get_all_sequences()

    assert sequences[PARTICLE_PREVIEW_SEQUENCE] == ["main"]


def test_particle_preview_transport_state_uses_active_particle_runtime():
    app = MagicMock()
    runtime = MagicMock()
    runtime.has_particles = True
    runtime.is_playing = True
    runtime.is_paused = False
    session = MagicMock()
    session.particle_runtime = runtime
    app.registry.active_session = session
    app.registry.all_sessions.return_value = [session]
    panel = AnimationEditorPanel(app)
    mgr = MagicMock()
    mgr.is_playing = False
    mgr.is_paused = False
    mgr.current_sequence = None

    is_playing, is_paused = panel._get_transport_state(PARTICLE_PREVIEW_SEQUENCE, mgr)

    assert is_playing is True
    assert is_paused is False


def test_particle_preview_transport_state_reports_paused_runtime():
    app = MagicMock()
    runtime = MagicMock()
    runtime.has_particles = True
    runtime.is_playing = True
    runtime.is_paused = True
    session = MagicMock()
    session.particle_runtime = runtime
    app.registry.active_session = session
    app.registry.all_sessions.return_value = [session]
    panel = AnimationEditorPanel(app)
    mgr = MagicMock()
    mgr.is_playing = False
    mgr.is_paused = False
    mgr.current_sequence = None

    is_playing, is_paused = panel._get_transport_state(PARTICLE_PREVIEW_SEQUENCE, mgr)

    assert is_playing is False
    assert is_paused is True


def test_particle_preview_transport_state_uses_child_particle_runtime():
    app = MagicMock()
    main = MagicMock()
    main.particle_runtime = None
    child_runtime = MagicMock()
    child_runtime.has_particles = True
    child_runtime.is_playing = True
    child_runtime.is_paused = False
    child = MagicMock()
    child.particle_runtime = child_runtime
    app.registry.active_session = main
    app.registry.all_sessions.return_value = [main, child]
    panel = AnimationEditorPanel(app)
    mgr = MagicMock()
    mgr.is_playing = False
    mgr.is_paused = False
    mgr.current_sequence = None

    is_playing, is_paused = panel._get_transport_state(PARTICLE_PREVIEW_SEQUENCE, mgr)

    assert is_playing is True
    assert is_paused is False


def test_non_particle_transport_state_uses_animation_manager():
    app = MagicMock()
    runtime = MagicMock()
    runtime.has_particles = True
    runtime.is_playing = True
    runtime.is_paused = False
    session = MagicMock()
    session.particle_runtime = runtime
    app.registry.active_session = session
    app.registry.all_sessions.return_value = [session]
    panel = AnimationEditorPanel(app)
    mgr = MagicMock()
    mgr.is_playing = False
    mgr.is_paused = True
    mgr.current_sequence = MagicMock()

    is_playing, is_paused = panel._get_transport_state("Fire", mgr)

    assert is_playing is False
    assert is_paused is True


def test_workspace_toolbar_current_sequence_prefers_particle_preview_selection():
    workspace = NifWorkspace.__new__(NifWorkspace)
    app = MagicMock()
    app.anim_coordinator.current_sequence_name = PARTICLE_PREVIEW_SEQUENCE
    workspace._app = app
    mgr = MagicMock()
    mgr.current_sequence.name = "Fire"

    sequence = workspace._get_toolbar_current_sequence(
        ["Fire", PARTICLE_PREVIEW_SEQUENCE],
        mgr,
    )

    assert sequence == PARTICLE_PREVIEW_SEQUENCE


def test_workspace_toolbar_playback_state_treats_paused_particles_as_paused():
    workspace = NifWorkspace.__new__(NifWorkspace)
    runtime = MagicMock()
    runtime.has_particles = True
    runtime.is_playing = True
    runtime.is_paused = True
    session = MagicMock()
    session.particle_runtime = runtime
    app = MagicMock()
    app.registry.all_sessions.return_value = [session]
    workspace._app = app
    mgr = MagicMock()
    mgr.is_playing = False
    mgr.is_paused = False

    is_playing, is_paused = workspace._get_toolbar_playback_state(
        PARTICLE_PREVIEW_SEQUENCE,
        mgr,
    )

    assert is_playing is False
    assert is_paused is True


def test_select_animation_sequence_allows_particle_preview_sequence():
    workspace = NifWorkspace.__new__(NifWorkspace)
    session = MagicMock()
    session.anim_manager.has_sequence.return_value = False
    app = MagicMock()
    app.anim_coordinator = None
    app.registry.all_sessions.return_value = [session]
    workspace._app = app

    workspace._select_animation_sequence(PARTICLE_PREVIEW_SEQUENCE)

    session.anim_manager.select_sequence.assert_called_once_with(PARTICLE_PREVIEW_SEQUENCE)


def test_select_animation_sequence_uses_coordinator_when_available():
    workspace = NifWorkspace.__new__(NifWorkspace)
    app = MagicMock()
    workspace._app = app

    workspace._select_animation_sequence(PARTICLE_PREVIEW_SEQUENCE)

    app.anim_coordinator.select.assert_called_once_with(PARTICLE_PREVIEW_SEQUENCE)
