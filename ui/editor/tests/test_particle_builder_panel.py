from types import SimpleNamespace
from unittest.mock import patch

from ui.editor.panels.particle_systems import ParticleSystemsPanel
from ui.editor.particles.authoring import AuthoringResult, DraftValidationIssue
from ui.editor.particles.builder_model import ModifierKind
from ui.editor.particles.model import ParticleSupportLevel, ParticleSystemModel


def _model(system_block_id=20, name="Mist"):
    return ParticleSystemModel(
        nif_id="main",
        system_block_id=system_block_id,
        name=name,
        data_block_id=system_block_id + 1,
        shader_property_block_id=system_block_id + 2,
        alpha_property_block_id=system_block_id + 3,
        controller_block_id=-1,
        emitter_block_id=system_block_id + 4,
        emitter_type="NiPSysBoxEmitter",
        modifier_block_ids=(system_block_id + 4, system_block_id + 5),
        modifier_types=("NiPSysBoxEmitter", "NiPSysGravityModifier"),
        source_texture=r"textures\effects\mist.dds",
        emitter_speed=0.75,
        emitter_lifetime=1.5,
        emitter_radius=0.4,
        max_particles=128,
        support_level=ParticleSupportLevel.SUPPORTED,
    )


class _Registry:
    def __init__(self, session):
        self.active_id = "main"
        self.sessions = {"main": session}

    @property
    def active_session(self):
        return self.sessions[self.active_id]


def _app(models=None, particle_runtime=None):
    session = SimpleNamespace(
        nif_id="main",
        particle_models=models if models is not None else [_model()],
        particle_runtime=particle_runtime,
    )
    return SimpleNamespace(registry=_Registry(session), status_text="")


def test_panel_opens_new_builder_with_realtime_preview_runtime():
    app = _app(particle_runtime=object())
    panel = ParticleSystemsPanel(app)

    panel.open_new_builder()

    popup = panel.builder_popup
    assert popup.is_open is True
    assert popup.draft.systems[0].display_name == "Smoke Puff"
    assert app.registry.active_session.particle_runtime is popup.preview_runtime
    assert popup.preview_runtime.is_playing is True


def test_panel_opens_selected_particle_system_in_builder():
    app = _app(models=[_model(20, "Mist"), _model(40, "Steam")])
    panel = ParticleSystemsPanel(app)
    panel.selected_system_block_id = 40

    opened = panel.open_builder_for_selected()

    assert opened is True
    assert panel.builder_popup.draft.effect_name == "Steam"
    assert panel.builder_popup.draft.systems[0].raw_overrides["source"]["system_block_id"] == 40


def test_builder_stacks_presets_and_modifiers_with_friendly_names():
    panel = ParticleSystemsPanel(_app())
    popup = panel.builder_popup
    popup.open_new()

    popup.add_preset_system("spark_burst")
    popup.add_modifier(ModifierKind.WIND)

    assert [system.display_name for system in popup.draft.systems] == [
        "Smoke Puff",
        "Spark Burst",
    ]
    active = popup.active_system
    assert active.display_name == "Spark Burst"
    assert active.modifiers[-1].display_name == "Wind"


def test_builder_raw_override_editor_updates_active_system():
    popup = ParticleSystemsPanel(_app()).builder_popup
    popup.open_new()

    ok = popup.set_raw_overrides_from_json('{"emitter": {"Speed Variation": 1.25}}')

    assert ok is True
    assert popup.active_system.raw_overrides["emitter"]["Speed Variation"] == 1.25


def test_builder_raw_override_text_change_updates_preview_immediately():
    app = _app()
    popup = ParticleSystemsPanel(app).builder_popup
    popup.open_new()
    original_preview = popup.preview_runtime

    ok = popup.update_raw_overrides_text('{"emitter": {"Speed Variation": 2.5}}')

    assert ok is True
    assert popup.active_system.raw_overrides["emitter"]["Speed Variation"] == 2.5
    assert popup.preview_runtime is not original_preview
    assert app.registry.active_session.particle_runtime is popup.preview_runtime


def test_builder_apply_restores_live_preview_before_writing_and_closes():
    original_runtime = object()
    app = _app(particle_runtime=original_runtime)
    popup = ParticleSystemsPanel(app).builder_popup
    popup.open_new()

    def _apply(_app, _draft, **_kwargs):
        assert _app.registry.active_session.particle_runtime is original_runtime
        return AuthoringResult((99,), ())

    with patch("ui.editor.panels.particle_builder.apply_draft_to_session", side_effect=_apply):
        result = popup.apply()

    assert result.system_block_ids == (99,)
    assert popup.is_open is False
    assert popup.preview_runtime is None


def test_builder_apply_keeps_popup_open_and_previewing_after_authoring_issue():
    original_runtime = object()
    app = _app(particle_runtime=original_runtime)
    popup = ParticleSystemsPanel(app).builder_popup
    popup.open_new()

    issue = DraftValidationIssue("Cannot write particle effect.")
    with patch(
        "ui.editor.panels.particle_builder.apply_draft_to_session",
        return_value=AuthoringResult((), (issue,)),
    ):
        result = popup.apply()

    assert result.issues == (issue,)
    assert popup.is_open is True
    assert app.registry.active_session.particle_runtime is popup.preview_runtime
    assert popup.log_message == "Cannot write particle effect."


def test_builder_draw_close_box_restores_original_runtime():
    original_runtime = object()
    app = _app(particle_runtime=original_runtime)
    popup = ParticleSystemsPanel(app).builder_popup
    popup.open_new()
    assert app.registry.active_session.particle_runtime is popup.preview_runtime

    with patch("ui.editor.panels.particle_builder.imgui") as imgui:
        imgui.WindowFlags_.always_auto_resize.value = 1
        imgui.begin_popup_modal.return_value = (True, False)
        imgui.input_text.return_value = (False, popup.draft.effect_name)
        imgui.checkbox.return_value = (False, popup.draft.loop_preview)
        imgui.slider_float.return_value = (False, popup.draft.preview_time_scale)
        imgui.begin_tab_bar.return_value = False
        imgui.button.return_value = False
        popup.draw()

    assert popup.is_open is False
    assert app.registry.active_session.particle_runtime is original_runtime


def test_builder_close_does_not_overwrite_runtime_rebuilt_while_open():
    original_runtime = object()
    rebuilt_runtime = object()
    app = _app(particle_runtime=original_runtime)
    popup = ParticleSystemsPanel(app).builder_popup
    popup.open_new()
    app.registry.active_session.particle_runtime = rebuilt_runtime

    popup.close()

    assert app.registry.active_session.particle_runtime is rebuilt_runtime
