from unittest.mock import MagicMock, patch

from creation_lib.nif.nif_file import NifFile

from ui.editor.panels.particle_systems import ParticleSystemsPanel
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
        max_particles=128,
        support_level=ParticleSupportLevel.SUPPORTED,
    )


class _Session:
    def __init__(self, models=None, nif=None):
        self.nif_id = "main"
        self.particle_models = models or [_model()]
        self.particle_runtime = MagicMock()
        self.nif = nif
        self.dirty = False


class _Registry:
    def __init__(self, session=None):
        self.active_id = "main"
        self.sessions = {"main": session or _Session()}

    @property
    def active_session(self):
        return self.sessions[self.active_id]

    def all_sessions(self):
        return list(self.sessions.values())


class _SelectionMgr:
    def __init__(self):
        self._selected_nif_id = None
        self._selected_block_id = None
        self._selected_block_id_override = None
        self._notify = MagicMock()
        self.select_by_id = MagicMock()

    @property
    def selected_nif_id(self):
        return self._selected_nif_id

    @property
    def selected_block_id(self):
        return self._selected_block_id


def test_find_owner_for_selected_particle_block():
    app = MagicMock()
    app.registry = _Registry()
    panel = ParticleSystemsPanel(app)

    panel.on_selection_changed("main", 25)

    assert panel.selected_system_block_id == 20


def test_select_block_falls_back_for_non_scene_particle_blocks():
    app = MagicMock()
    app.registry = _Registry()
    app.properties = MagicMock()
    app.scene_tree = MagicMock()
    app.selection_mgr = _SelectionMgr()

    panel = ParticleSystemsPanel(app)

    panel._select_block("main", 25)

    app.selection_mgr.select_by_id.assert_called_once_with("main", 25)
    app.selection_mgr._notify.assert_called_once_with("main", 25)
    app.properties._on_select.assert_called_once_with("main", 25)
    assert app.selection_mgr.selected_nif_id == "main"
    assert app.selection_mgr.selected_block_id == 25
    assert app.registry.active_id == "main"


def test_effect_panel_does_not_own_preview_override_state():
    panel = ParticleSystemsPanel(MagicMock())

    assert not hasattr(panel, "set_preview_overrides")
    assert not hasattr(panel, "_preview_overrides")
    assert not hasattr(panel, "_preview_overrides_for")
    assert not hasattr(panel, "_apply_preview_override_change")


def test_diagnostics_lines_include_static_particle_details():
    panel = ParticleSystemsPanel(MagicMock())
    model = _model(system_block_id=20)

    diagnostics = panel.diagnostics_lines(model)

    assert "Emitter: NiPSysBoxEmitter [24]" in diagnostics
    assert "Max particles: 128" in diagnostics
    assert "Modifiers: 2" in diagnostics


def test_builder_action_labels_follow_effect_stack_pattern():
    panel = ParticleSystemsPanel(MagicMock())

    labels = panel.builder_action_labels()

    assert labels["add"] == "+ Effect"
    assert labels["copy_selected"] == "Copy Selected"
    assert "edit_selected" not in labels


def test_modifier_select_stays_in_panel_and_jump_button_selects_tree_block():
    app = MagicMock()
    app.registry = _Registry()
    app.properties = MagicMock()
    app.scene_tree = MagicMock()
    app.selection_mgr = _SelectionMgr()
    panel = ParticleSystemsPanel(app)

    panel.select_modifier_for_edit(25)

    assert panel.selected_modifier_block_id == 25
    app.selection_mgr.select_by_id.assert_not_called()

    panel.jump_to_modifier_in_tree("main", 25)

    app.selection_mgr.select_by_id.assert_called_once_with("main", 25)
    app.selection_mgr._notify.assert_called_once_with("main", 25)
    app.scene_tree._expand_to_block.assert_called_once_with(25)
    assert app.scene_tree._scroll_to_selected is True


def test_modifier_fields_delegate_to_shared_properties_renderer():
    nif = NifFile()
    nif.add_block("BSFadeNode", {"Name": "Root", "Children": [], "Num Children": 0})
    modifier = nif.add_block("BSPSysSimpleColorModifier", {"Order": 2})
    app = MagicMock()
    app.registry = _Registry(_Session(nif=nif))
    app.properties = MagicMock()
    panel = ParticleSystemsPanel(app)

    rendered = panel._draw_modifier_fields_with_properties("main", modifier)

    assert rendered is True
    app.properties.draw_block_fields.assert_called_once_with("main", nif, modifier)


def test_add_modifier_to_selected_system_routes_to_nif_authoring():
    app = MagicMock()
    app.registry = _Registry()
    panel = ParticleSystemsPanel(app)
    panel.selected_system_block_id = 20

    with patch("ui.editor.panels.particle_systems.add_modifier_to_particle_system_session") as add_modifier:
        add_modifier.return_value.issues = ()
        result = panel.add_modifier_to_selected_system(ModifierKind.GRAVITY)

    assert result is True
    add_modifier.assert_called_once_with(app, app.registry.active_session.particle_models[0], ModifierKind.GRAVITY)


def test_remove_modifier_from_selected_system_routes_to_nif_authoring():
    app = MagicMock()
    app.registry = _Registry()
    panel = ParticleSystemsPanel(app)
    panel.selected_system_block_id = 20
    panel.selected_modifier_block_id = 25

    with patch("ui.editor.panels.particle_systems.remove_modifier_from_particle_system_session") as remove_modifier:
        remove_modifier.return_value.issues = ()
        result = panel.remove_modifier_from_selected_system(25)

    assert result is True
    assert panel.selected_modifier_block_id is None
    remove_modifier.assert_called_once_with(app, app.registry.active_session.particle_models[0], 25)
