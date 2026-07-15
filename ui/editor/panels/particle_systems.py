from __future__ import annotations

from imgui_bundle import imgui

from ui.editor.particles.authoring import (
    add_modifier_to_particle_system_session,
    remove_modifier_from_particle_system_session,
)
from ui.editor.particles.builder_model import ModifierKind
from ui.editor.particles.catalog import MODIFIER_CATALOG
from ui.editor.particles.model import (
    ParticleSupportLevel,
    ParticleSystemModel,
    owner_system_for_block,
)
from ui.editor.panels.particle_builder import ParticleBuilderPopup


_BUILDER_ACTION_LABELS = {
    "add": "+ Effect",
    "copy_selected": "Copy Selected",
}


class ParticleSystemsPanel:
    def __init__(self, app):
        self.app = app
        self._visible = True
        self.window_name = "Particle Systems"
        self.selected_system_block_id: int | None = None
        self.show_static_overlays = True
        self.show_runtime_particles = True
        self.builder_popup = _builder_popup(app)
        self.selected_modifier_block_id: int | None = None
        self._selected_modifier_kind_index = 0

    def on_selection_changed(self, nif_id, block_id):
        if block_id is None:
            self.selected_system_block_id = None
            return

        session = self._session(nif_id)
        model = owner_system_for_block(getattr(session, "particle_models", []), block_id)
        self.selected_system_block_id = (
            model.system_block_id if model is not None else None
        )

    def draw(self):
        if not self._visible:
            return

        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return

        session = self._active_session()
        models = list(getattr(session, "particle_models", []) or [])
        if expanded:
            self._draw_builder_actions()
            imgui.separator()
        if not models:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "No particle systems found.",
            )
            imgui.end()
            return

        if expanded:
            self._draw_system_list(models)

        imgui.end()

    def open_new_builder(self) -> None:
        self.builder_popup.open_new(attach_to_block_id=0)

    def open_builder_for_selected(self) -> bool:
        model = self._selected_model()
        if model is None:
            if hasattr(self.app, "status_text"):
                self.app.status_text = "Select a particle system to copy into the builder."
            return False
        self.builder_popup.open_for_model(model, attach_to_block_id=0)
        return True

    def add_modifier_to_selected_system(self, modifier_kind: ModifierKind) -> bool:
        model = self._selected_model()
        if model is None:
            if hasattr(self.app, "status_text"):
                self.app.status_text = "Select a particle system before adding a modifier."
            return False
        result = add_modifier_to_particle_system_session(self.app, model, modifier_kind)
        if result.issues:
            if hasattr(self.app, "status_text"):
                self.app.status_text = result.issues[0].message
            return False
        return True

    def remove_modifier_from_selected_system(self, modifier_block_id: int) -> bool:
        model = self._selected_model()
        if model is None:
            if hasattr(self.app, "status_text"):
                self.app.status_text = "Select a particle system before removing a modifier."
            return False
        result = remove_modifier_from_particle_system_session(
            self.app,
            model,
            modifier_block_id,
        )
        if result.issues:
            if hasattr(self.app, "status_text"):
                self.app.status_text = result.issues[0].message
            return False
        if self.selected_modifier_block_id == modifier_block_id:
            self.selected_modifier_block_id = None
        return True

    def builder_action_labels(self) -> dict[str, str]:
        return dict(_BUILDER_ACTION_LABELS)

    def _draw_builder_actions(self) -> None:
        labels = self.builder_action_labels()
        if imgui.button(labels["add"]):
            self.open_new_builder()
        imgui.same_line()
        model = self._selected_model()
        if model is None:
            imgui.begin_disabled()
        if imgui.button(labels["copy_selected"]):
            self.open_builder_for_selected()
        if model is None:
            imgui.end_disabled()

    def _draw_system_list(self, models: list[ParticleSystemModel]) -> None:
        imgui.text_disabled("Effect / Source / Modifiers")
        for index, model in enumerate(models):
            flags = imgui.TreeNodeFlags_.open_on_arrow
            if index == 0:
                flags |= imgui.TreeNodeFlags_.default_open
            if model.system_block_id == self.selected_system_block_id:
                flags |= imgui.TreeNodeFlags_.selected

            opened = imgui.tree_node_ex(
                f"{model.name} [{model.system_block_id}]##particle_effect_{model.system_block_id}",
                flags,
            )
            if imgui.is_item_clicked(0):
                self.selected_system_block_id = model.system_block_id
                self._select_block(model.nif_id, model.system_block_id)
            imgui.same_line()
            if imgui.small_button(f"Copy##particle_effect_copy_{model.system_block_id}"):
                self.selected_system_block_id = model.system_block_id
                self.open_builder_for_selected()
            if opened:
                self._draw_system_details(model)
                imgui.tree_pop()

    def _draw_selected_system(self) -> None:
        model = self._selected_model()
        if model is None:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "Select a particle system.",
            )
            return

        self._draw_system_details(model)

    def _draw_system_details(self, model: ParticleSystemModel) -> None:
        for line in self.diagnostics_lines(model):
            imgui.text(line)

        imgui.separator()
        _, self.show_static_overlays = imgui.checkbox(
            "Show static overlays", self.show_static_overlays
        )
        _, self.show_runtime_particles = imgui.checkbox(
            "Show runtime particles", self.show_runtime_particles
        )

        imgui.separator()
        imgui.text("Modifiers")
        self._draw_add_modifier_controls(model)
        if model.modifier_block_ids:
            self._draw_modifier_rows(model)
        else:
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0), "None")

        self._draw_modifier_editor(model)

        if model.support_level != ParticleSupportLevel.SUPPORTED or model.warnings:
            imgui.separator()
            imgui.text_colored(imgui.ImVec4(0.9, 0.7, 0.2, 1.0), "Warnings")
            for warning in model.warnings:
                imgui.text_wrapped(f"Block {warning.block_id}: {warning.message}")
            if not model.warnings:
                imgui.text_wrapped(f"Preview support is {model.support_level.value}.")

    def _draw_add_modifier_controls(self, model: ParticleSystemModel) -> None:
        modifier_values = tuple(ModifierKind)
        modifier_labels = [
            MODIFIER_CATALOG[kind].friendly_name for kind in modifier_values
        ]
        selected_index = min(
            self._selected_modifier_kind_index,
            len(modifier_values) - 1,
        )
        _changed, selected_index = imgui.combo(
            f"Add##particle_add_modifier_{model.system_block_id}",
            selected_index,
            modifier_labels,
        )
        self._selected_modifier_kind_index = selected_index
        imgui.same_line()
        if imgui.small_button(
            f"+ Modifier##particle_add_modifier_btn_{model.system_block_id}"
        ):
            self.add_modifier_to_selected_system(modifier_values[selected_index])

    def _draw_modifier_rows(self, model: ParticleSystemModel) -> None:
        for index, (block_id, type_name) in enumerate(
            zip(model.modifier_block_ids, model.modifier_types)
        ):
            depth = _modifier_depth(model, index)
            if depth:
                imgui.indent(depth * 16.0)
            selected = block_id == self.selected_modifier_block_id
            clicked, _ = imgui.selectable(
                f"{_modifier_label(type_name)} [{block_id}]",
                selected,
            )
            if clicked:
                self.select_modifier_for_edit(block_id)
            imgui.same_line()
            if imgui.small_button(f"Tree##particle_mod_jump_{block_id}"):
                self.jump_to_modifier_in_tree(model.nif_id, block_id)
            imgui.same_line()
            if imgui.small_button(f"X##particle_mod_remove_{block_id}"):
                self.remove_modifier_from_selected_system(block_id)
            if depth:
                imgui.unindent(depth * 16.0)

    def select_modifier_for_edit(self, block_id: int) -> None:
        self.selected_modifier_block_id = int(block_id)

    def jump_to_modifier_in_tree(self, nif_id: str, block_id: int) -> None:
        self._select_block(nif_id, block_id)

    def _draw_modifier_editor(self, model: ParticleSystemModel) -> None:
        block_id = self.selected_modifier_block_id
        if block_id is None:
            return
        if block_id not in model.modifier_block_ids:
            return

        block = self._modifier_block(model.nif_id, block_id)
        if block is None:
            return

        imgui.separator()
        imgui.text(f"Edit Modifier: {_modifier_label(block.type_name)} [{block_id}]")
        if not self._draw_modifier_fields_with_properties(model.nif_id, block):
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "Open the Properties panel to edit this modifier.",
            )

    def _draw_modifier_fields_with_properties(self, nif_id: str, block) -> bool:
        properties = getattr(self.app, "properties", None)
        draw_block_fields = getattr(properties, "draw_block_fields", None)
        if not callable(draw_block_fields):
            return False
        session = self._session(nif_id)
        nif = getattr(session, "nif", None) if session is not None else None
        if nif is None:
            nif = getattr(self.app, "nif_file", None)
        if nif is None:
            return False
        draw_block_fields(nif_id, nif, block)
        return True

    def _modifier_block(self, nif_id: str, block_id: int):
        session = self._session(nif_id)
        nif = getattr(session, "nif", None) if session is not None else getattr(self.app, "nif_file", None)
        return nif.get_block(block_id) if nif is not None else None

    def diagnostics_lines(self, model: ParticleSystemModel) -> list[str]:
        lines = [
            f"Name: {model.name}",
            f"System block: {model.system_block_id}",
            f"Data block: {_block_label(model.data_block_id)}",
        ]
        if model.emitter_type:
            lines.append(
                f"Emitter: {model.emitter_type} [{_block_label(model.emitter_block_id)}]"
            )
        else:
            lines.append("Emitter: None")
        lines.extend(
            [
                f"Max particles: {_count_label(model.max_particles)}",
                f"Modifiers: {len(model.modifier_block_ids)}",
                f"Support: {model.support_level.value}",
            ]
        )
        return lines

    def _selected_model(self) -> ParticleSystemModel | None:
        session = self._active_session()
        for model in getattr(session, "particle_models", []) or []:
            if model.system_block_id == self.selected_system_block_id:
                return model
        return None

    def _active_session(self):
        registry = getattr(self.app, "registry", None)
        if registry is None:
            return None
        try:
            return registry.active_session
        except (AttributeError, KeyError):
            return None

    def _session(self, nif_id):
        registry = getattr(self.app, "registry", None)
        if registry is None or nif_id is None:
            return self._active_session()
        sessions = getattr(registry, "sessions", {})
        return sessions.get(nif_id) or self._active_session()

    def _select_block(self, nif_id: str, block_id: int) -> None:
        selection_mgr = getattr(self.app, "selection_mgr", None)
        if selection_mgr is not None and hasattr(selection_mgr, "select_by_id"):
            selection_mgr.select_by_id(nif_id, block_id)
        elif selection_mgr is not None and hasattr(selection_mgr, "select_by_block_id"):
            selection_mgr.select_by_block_id(block_id)

        selected_nif_id = getattr(selection_mgr, "selected_nif_id", None)
        selected_block_id = getattr(selection_mgr, "selected_block_id", None)
        if selected_nif_id != nif_id or selected_block_id != block_id:
            self._select_non_scene_block(nif_id, block_id)

    def _select_non_scene_block(self, nif_id: str, block_id: int) -> None:
        registry = getattr(self.app, "registry", None)
        sessions = getattr(registry, "sessions", {})
        if registry is not None and nif_id in sessions:
            registry.active_id = nif_id

        selection_mgr = getattr(self.app, "selection_mgr", None)
        if selection_mgr is not None:
            if hasattr(selection_mgr, "_selected_nif_id"):
                selection_mgr._selected_nif_id = nif_id
            if hasattr(selection_mgr, "_selected_block_id"):
                selection_mgr._selected_block_id = block_id
            if hasattr(selection_mgr, "_selected_block_id_override"):
                selection_mgr._selected_block_id_override = block_id
            if hasattr(selection_mgr, "_notify"):
                selection_mgr._notify(nif_id, block_id)

        scene_tree = getattr(self.app, "scene_tree", None)
        if scene_tree is not None:
            if hasattr(scene_tree, "_selected_nif_id"):
                scene_tree._selected_nif_id = nif_id
            if hasattr(scene_tree, "_selected_block_id"):
                scene_tree._selected_block_id = block_id
            if hasattr(scene_tree, "_expand_to_block"):
                scene_tree._expand_to_block(block_id)
            if hasattr(scene_tree, "_scroll_to_selected"):
                scene_tree._scroll_to_selected = True

        properties = getattr(self.app, "properties", None)
        if properties is not None and hasattr(properties, "_on_select"):
            properties._on_select(nif_id, block_id)


def _block_label(block_id: int | None) -> str:
    return str(block_id) if block_id is not None and block_id >= 0 else "None"


def _count_label(count: int | None) -> str:
    return str(count) if count is not None else "Unknown"


def _modifier_label(type_name: str) -> str:
    return {
        "NiPSysSphereEmitter": "Sphere Emitter",
        "NiPSysBoxEmitter": "Box Emitter",
        "NiPSysCylinderEmitter": "Cylinder Emitter",
        "NiPSysGravityModifier": "Gravity",
        "NiPSysDragModifier": "Drag",
        "BSWindModifier": "Wind",
        "NiPSysRotationModifier": "Rotation",
        "BSPSysScaleModifier": "Size Over Life",
        "NiPSysGrowFadeModifier": "Grow/Fade",
        "BSPSysSimpleColorModifier": "Color Over Life",
        "NiPSysColorModifier": "Color Modifier",
        "NiPSysColliderManager": "Collision",
        "NiPSysSpawnModifier": "Spawn Rate",
        "NiPSysAgeDeathModifier": "Age/Death",
        "NiPSysPositionModifier": "Position",
        "NiPSysBoundUpdateModifier": "Bounds",
    }.get(type_name, type_name)


def _modifier_depth(model: ParticleSystemModel, index: int) -> int:
    if index < len(model.modifier_depths):
        return max(0, int(model.modifier_depths[index]))
    return 0


def _builder_popup(app) -> ParticleBuilderPopup:
    popup = getattr(app, "particle_builder_popup", None)
    if popup is None:
        popup = ParticleBuilderPopup(app)
        try:
            app.particle_builder_popup = popup
        except Exception:
            pass
    return popup
