from __future__ import annotations

import json
from typing import Any, Mapping

from imgui_bundle import imgui

from ui.editor.particles.authoring import (
    AuthoringResult,
    apply_draft_to_session,
    draft_from_particle_model,
)
from ui.editor.particles.builder_model import (
    BlendPreset,
    EmissionShape,
    ModifierKind,
    ParticleEffectDraft,
    ParticleSystemDraft,
)
from ui.editor.particles.catalog import (
    BLEND_PRESETS,
    MODIFIER_CATALOG,
    build_preset,
    get_emission_shape_entry,
    get_modifier_catalog_entry,
    preset_keys,
)
from ui.editor.particles.model import ParticleSystemModel
from ui.editor.particles.preview import build_preview_runtime_for_draft


class ParticleBuilderPopup:
    POPUP_TITLE = "Particle Effect Builder"

    def __init__(self, app):
        self.app = app
        self.is_open = False
        self._pending_open = False
        self.draft = _default_draft()
        self.attach_to_block_id: int | None = 0
        self.preview_runtime = None
        self._has_original_runtime = False
        self._original_runtime = None
        self._preview_session = None
        self._selected_preset_index = 0
        self._selected_modifier_index = 0
        self._selected_modifier_kind_index = 0
        self._raw_overrides_text = "{}"
        self.log_message = ""
        self.log_is_error = False

    @property
    def active_system(self) -> ParticleSystemDraft:
        index = self.draft.active_system_index or 0
        return self.draft.systems[index]

    def open_new(self, attach_to_block_id: int | None = 0) -> None:
        self.open(_default_draft(), attach_to_block_id=attach_to_block_id)

    def open_for_model(
        self,
        model: ParticleSystemModel,
        attach_to_block_id: int | None = 0,
    ) -> None:
        self.open(draft_from_particle_model(model), attach_to_block_id=attach_to_block_id)

    def open(
        self,
        draft: ParticleEffectDraft,
        attach_to_block_id: int | None = 0,
    ) -> None:
        self._restore_preview_runtime()
        self.draft = draft if draft.systems else _default_draft().with_updates(effect_name=draft.effect_name)
        self.attach_to_block_id = attach_to_block_id
        self.log_message = ""
        self.log_is_error = False
        self.is_open = True
        self._pending_open = True
        self._sync_raw_overrides_text()
        self._install_preview_runtime()

    def close(self) -> None:
        self._restore_preview_runtime()
        self.preview_runtime = None
        self.is_open = False
        self._pending_open = False

    def add_preset_system(self, preset_key: str) -> None:
        self.draft = self.draft.add_system(build_preset(preset_key))
        self._sync_raw_overrides_text()
        self._install_preview_runtime()

    def add_modifier(self, kind: ModifierKind) -> None:
        system_index = self.draft.active_system_index or 0
        entry = get_modifier_catalog_entry(kind)
        system = self.active_system.add_modifier(entry.create_draft())
        self.draft = self.draft.update_system(system_index, system)
        self._selected_modifier_index = max(0, len(system.modifiers) - 1)
        self._selected_modifier_kind_index = tuple(ModifierKind).index(kind)
        self._install_preview_runtime()

    def set_raw_overrides_from_json(self, text: str) -> bool:
        self._raw_overrides_text = text or "{}"
        return self._apply_raw_overrides_text(self._raw_overrides_text)

    def update_raw_overrides_text(self, text: str) -> bool:
        self._raw_overrides_text = text
        return self._apply_raw_overrides_text(text)

    def _apply_raw_overrides_text(self, text: str) -> bool:
        try:
            value = json.loads(text or "{}")
        except json.JSONDecodeError as exc:
            self.log_message = f"Raw overrides JSON is invalid: {exc.msg}"
            self.log_is_error = True
            return False
        if not isinstance(value, dict):
            self.log_message = "Raw overrides must be a JSON object."
            self.log_is_error = True
            return False

        system_index = self.draft.active_system_index or 0
        self.draft = self.draft.update_system(system_index, raw_overrides=value)
        self.log_message = ""
        self.log_is_error = False
        self._install_preview_runtime()
        return True

    def apply(self) -> AuthoringResult:
        self._restore_preview_runtime()
        result = apply_draft_to_session(
            self.app,
            self.draft,
            attach_to_block_id=self.attach_to_block_id,
        )
        if result.issues:
            self.log_message = result.issues[0].message
            self.log_is_error = True
            self._install_preview_runtime()
            return result
        self.close()
        return result

    def draw(self) -> None:
        if self._pending_open:
            imgui.open_popup(self.POPUP_TITLE)
            self._pending_open = False

        if not self.is_open:
            return

        flags = imgui.WindowFlags_.always_auto_resize.value
        opened, visible = imgui.begin_popup_modal(self.POPUP_TITLE, True, flags)
        if not opened:
            self.close()
            return
        if not visible:
            self.close()
            imgui.close_current_popup()
            imgui.end_popup()
            return

        self._draw_header()
        imgui.separator()
        if imgui.begin_tab_bar("##particle_builder_tabs"):
            if imgui.begin_tab_item("Presets")[0]:
                self._draw_presets_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Stack")[0]:
                self._draw_stack_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Advanced")[0]:
                self._draw_advanced_tab()
                imgui.end_tab_item()
            imgui.end_tab_bar()

        self._draw_log()
        imgui.separator()
        if imgui.button("Apply", imgui.ImVec2(120, 0)):
            result = self.apply()
            if not result.issues:
                imgui.close_current_popup()
        imgui.same_line()
        if imgui.button("Close", imgui.ImVec2(120, 0)):
            self.close()
            imgui.close_current_popup()

        imgui.end_popup()

    def _draw_header(self) -> None:
        changed, effect_name = imgui.input_text("Name##particle_builder_effect", self.draft.effect_name)
        if changed:
            self.draft = self.draft.with_updates(effect_name=effect_name)

        _, loop_preview = imgui.checkbox("Loop Preview", self.draft.loop_preview)
        if loop_preview != self.draft.loop_preview:
            self.draft = self.draft.with_updates(loop_preview=loop_preview)
            self._install_preview_runtime()

        changed_time, time_scale = imgui.slider_float(
            "Preview Time", self.draft.preview_time_scale, 0.1, 4.0, "%.2f"
        )
        if changed_time:
            self.draft = self.draft.with_updates(preview_time_scale=time_scale)
            self._install_preview_runtime()

    def _draw_presets_tab(self) -> None:
        keys = preset_keys()
        labels = [_friendly_preset_name(key) for key in keys]
        changed, selected = imgui.combo("Preset##particle_builder", self._selected_preset_index, labels)
        if changed:
            self._selected_preset_index = selected
        if imgui.button("+ Add Preset", imgui.ImVec2(140, 0)):
            self.add_preset_system(keys[self._selected_preset_index])
        imgui.same_line()
        if imgui.button("Replace Stack", imgui.ImVec2(140, 0)):
            system = build_preset(keys[self._selected_preset_index])
            self.draft = self.draft.with_updates(
                effect_name=system.display_name,
                systems=(system,),
                active_system_index=0,
            )
            self._sync_raw_overrides_text()
            self._install_preview_runtime()

    def _draw_stack_tab(self) -> None:
        self._draw_system_picker()
        imgui.separator()
        self._draw_active_system_fields()
        imgui.separator()
        self._draw_modifier_picker()

    def _draw_system_picker(self) -> None:
        imgui.text("Systems")
        for index, system in enumerate(self.draft.systems):
            label = f"{system.display_name} ({_friendly_emission(system.emission_shape)})##system_{index}"
            clicked, _ = imgui.selectable(label, index == self.draft.active_system_index)
            if clicked:
                self.draft = self.draft.select_system(index)
                self._sync_raw_overrides_text()

        if imgui.button("+ Add System", imgui.ImVec2(140, 0)):
            self.add_preset_system(preset_keys()[self._selected_preset_index])
        if len(self.draft.systems) > 1:
            imgui.same_line()
            if imgui.button("Remove System", imgui.ImVec2(140, 0)):
                self.draft = self.draft.remove_system(self.draft.active_system_index or 0)
                self._sync_raw_overrides_text()
                self._install_preview_runtime()

    def _draw_active_system_fields(self) -> None:
        system_index = self.draft.active_system_index or 0
        system = self.active_system

        changed_name, name = imgui.input_text("System Name##particle_builder", system.display_name)
        if changed_name:
            system = system.with_updates(display_name=name)

        shape_values = tuple(EmissionShape)
        shape_labels = [_friendly_emission(shape) for shape in shape_values]
        shape_index = shape_values.index(system.emission_shape)
        changed_shape, shape_index = imgui.combo("Emitter##particle_builder", shape_index, shape_labels)
        if changed_shape:
            system = system.with_updates(emission_shape=shape_values[shape_index])

        changed_texture, texture = imgui.input_text("Texture##particle_builder", system.texture_path or "")
        if changed_texture:
            system = system.with_updates(texture_path=texture or None)

        blend_values = tuple(BlendPreset)
        blend_labels = [BLEND_PRESETS[blend].friendly_name for blend in blend_values]
        blend_index = blend_values.index(system.blend)
        changed_blend, blend_index = imgui.combo("Blend##particle_builder", blend_index, blend_labels)
        if changed_blend:
            system = system.with_updates(blend=blend_values[blend_index])

        system = self._draw_numeric_system_fields(system)
        changed_color, color = imgui.color_edit4("Color##particle_builder", imgui.ImVec4(*system.color_rgba))
        if changed_color:
            color_rgba = _color_tuple(color)
            system = system.with_updates(color_rgba=color_rgba, alpha=color_rgba[3])

        if system != self.active_system:
            self.draft = self.draft.update_system(system_index, system)
            self._sync_raw_overrides_text()
            self._install_preview_runtime()

    def _draw_numeric_system_fields(self, system: ParticleSystemDraft) -> ParticleSystemDraft:
        changed_rate, emission_rate = imgui.slider_float("Rate##particle_builder", system.emission_rate, 0.0, 512.0, "%.1f")
        changed_lifetime, lifetime = imgui.slider_float("Lifetime##particle_builder", system.lifetime, 0.01, 30.0, "%.2f")
        changed_speed, speed = imgui.slider_float("Speed##particle_builder", system.speed, 0.0, 20.0, "%.2f")
        changed_spread, spread = imgui.slider_float("Spread##particle_builder", system.spread_degrees, 0.0, 180.0, "%.1f")
        changed_size, size = imgui.slider_float("Size##particle_builder", system.particle_size, 0.01, 10.0, "%.2f")
        changed_rows, rows = imgui.input_int("Atlas Rows##particle_builder", int(system.atlas_rows))
        changed_columns, columns = imgui.input_int("Atlas Columns##particle_builder", int(system.atlas_columns))
        max_index = max(0, max(1, rows) * max(1, columns) - 1)
        changed_index, subtexture_index = imgui.slider_int(
            "Atlas Cell##particle_builder",
            min(int(system.subtexture_index), max_index),
            0,
            max_index,
        )
        if any((
            changed_rate,
            changed_lifetime,
            changed_speed,
            changed_spread,
            changed_size,
            changed_rows,
            changed_columns,
            changed_index,
        )):
            return system.with_updates(
                emission_rate=emission_rate,
                lifetime=lifetime,
                speed=speed,
                spread_degrees=spread,
                particle_size=size,
                atlas_rows=max(1, int(rows)),
                atlas_columns=max(1, int(columns)),
                subtexture_index=max(0, int(subtexture_index)),
            )
        return system

    def _draw_modifier_picker(self) -> None:
        system_index = self.draft.active_system_index or 0
        system = self.active_system
        imgui.text("Modifiers")
        for index, modifier in enumerate(system.modifiers):
            changed_enabled, enabled = imgui.checkbox(f"##mod_enabled_{index}", modifier.enabled)
            imgui.same_line()
            clicked, _ = imgui.selectable(
                f"{modifier.display_name}##modifier_{index}",
                index == self._selected_modifier_index,
            )
            if changed_enabled:
                system = system.update_modifier(index, enabled=enabled)
            if clicked:
                self._selected_modifier_index = index

        modifier_values = tuple(ModifierKind)
        modifier_labels = [MODIFIER_CATALOG[kind].friendly_name for kind in modifier_values]
        changed, selected = imgui.combo(
            "Add##particle_builder_modifier",
            self._selected_modifier_kind_index,
            modifier_labels,
        )
        if changed:
            self._selected_modifier_kind_index = selected
        if imgui.button("+ Add Modifier", imgui.ImVec2(140, 0)):
            self.add_modifier(modifier_values[self._selected_modifier_kind_index])
            return
        if system.modifiers and imgui.button("Remove Modifier", imgui.ImVec2(140, 0)):
            system = system.remove_modifier(min(self._selected_modifier_index, len(system.modifiers) - 1))
            self._selected_modifier_index = max(0, min(self._selected_modifier_index, len(system.modifiers) - 1))

        if system != self.active_system:
            self.draft = self.draft.update_system(system_index, system)
            self._install_preview_runtime()

    def _draw_advanced_tab(self) -> None:
        imgui.text("Raw Block Overrides")
        changed, text = imgui.input_text_multiline(
            "##particle_builder_raw",
            self._raw_overrides_text,
            imgui.ImVec2(520, 180),
        )
        if changed:
            self.update_raw_overrides_text(text)
        if imgui.button("Update Raw Overrides", imgui.ImVec2(170, 0)):
            self.set_raw_overrides_from_json(self._raw_overrides_text)

    def _draw_log(self) -> None:
        if not self.log_message:
            return
        color = (
            imgui.ImVec4(0.95, 0.4, 0.4, 1.0)
            if self.log_is_error
            else imgui.ImVec4(0.4, 0.9, 0.4, 1.0)
        )
        imgui.separator()
        imgui.push_style_color(imgui.Col_.text.value, color)
        imgui.text_wrapped(self.log_message)
        imgui.pop_style_color()

    def _install_preview_runtime(self) -> None:
        session = _active_session(self.app)
        if session is None:
            return
        if not self._has_original_runtime:
            self._original_runtime = getattr(session, "particle_runtime", None)
            self._preview_session = session
            self._has_original_runtime = True
        elif session is self._preview_session and getattr(session, "particle_runtime", None) is not self.preview_runtime:
            self._original_runtime = getattr(session, "particle_runtime", None)
        self.preview_runtime = build_preview_runtime_for_draft(self.draft)
        preview_override = _preview_time_override(self.draft.preview_time_scale)
        for model in self.preview_runtime.models:
            self.preview_runtime.set_overrides(model.system_block_id, preview_override)
        if self.draft.systems:
            self.preview_runtime.play()
        session.particle_runtime = self.preview_runtime

    def _restore_preview_runtime(self) -> None:
        if not self._has_original_runtime:
            return
        session = self._preview_session or _active_session(self.app)
        if session is not None and getattr(session, "particle_runtime", None) is self.preview_runtime:
            session.particle_runtime = self._original_runtime
        self._has_original_runtime = False
        self._original_runtime = None
        self._preview_session = None

    def _sync_raw_overrides_text(self) -> None:
        if not self.draft.systems:
            self._raw_overrides_text = "{}"
            return
        self._raw_overrides_text = json.dumps(
            _jsonable(self.active_system.raw_overrides),
            indent=2,
            sort_keys=True,
        )


def _default_draft() -> ParticleEffectDraft:
    system = build_preset("smoke_puff")
    return ParticleEffectDraft(effect_name=system.display_name, systems=(system,))


def _preview_time_override(time_scale: float):
    from ui.editor.particles.runtime import ParticlePreviewOverrides

    return ParticlePreviewOverrides(time_scale=time_scale)


def _active_session(app):
    registry = getattr(app, "registry", None)
    if registry is None:
        return None
    try:
        return registry.active_session
    except (AttributeError, KeyError):
        return None


def _friendly_preset_name(preset_key: str) -> str:
    return build_preset(preset_key).display_name


def _friendly_emission(shape: EmissionShape) -> str:
    return get_emission_shape_entry(shape).friendly_name


def _color_tuple(color) -> tuple[float, float, float, float]:
    if isinstance(color, (list, tuple)):
        return (float(color[0]), float(color[1]), float(color[2]), float(color[3]))
    return (float(color.x), float(color.y), float(color.z), float(color.w))


def _jsonable(value: Any) -> Any:
    if isinstance(value, Mapping):
        return {str(key): _jsonable(item) for key, item in value.items()}
    if isinstance(value, tuple | list):
        return [_jsonable(item) for item in value]
    return value
