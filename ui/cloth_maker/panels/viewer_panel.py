"""Viewer panel — cloth info readout and display toggles.

Right-side panel in the cloth maker workspace. Shows cloth data
summary (particle/constraint/collidable counts) and overlay
display toggles.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.viewer_panel")


class ViewerPanel:
    """Right-side panel: cloth info + display toggles."""

    def __init__(self, app: ClothMakerApp):
        self.app = app

    def draw(self) -> None:
        visible, _ = imgui.begin("Viewer##cloth_maker")
        if not visible:
            imgui.end()
            return

        scene = self.app.scene

        if scene.loaded:
            self._draw_cloth_info(scene)

        # --- Scene Settings (always visible) ---
        self._draw_scene_section()

        # --- NIF Path ---
        if scene.loaded:
            imgui.spacing()
            imgui.separator_text("File")
            imgui.text_wrapped(scene.nif_path)

        imgui.end()

    def _draw_scene_section(self) -> None:
        """Draw scene, mesh, texture, and lighting controls."""
        app = self.app
        renderer = getattr(app, 'renderer', None)
        scene_settings = getattr(app, 'scene_settings', None)

        imgui.spacing()
        imgui.separator_text("Scene")

        # Background color
        if scene_settings is not None:
            col = imgui.ImVec4(*scene_settings.bg_color, 1.0)
            c, col = imgui.color_edit3("Background##cm", col)
            if c:
                scene_settings.bg_color = [col.x, col.y, col.z]
                scene_settings.apply_to(app)

        # Grid toggle
        if scene_settings is not None:
            c, val = imgui.checkbox("Show Grid##cm", scene_settings.grid_visible)
            if c:
                scene_settings.grid_visible = val
                scene_settings.apply_to(app)

        imgui.spacing()
        imgui.separator_text("Mesh")

        # Mesh opacity slider
        if renderer is not None:
            imgui.set_next_item_width(150)
            c, val = imgui.slider_float("Mesh Opacity##cm", renderer.toggles.mesh_alpha, 0.0, 1.0, "%.2f")
            if c:
                renderer.toggles.mesh_alpha = val

        imgui.spacing()
        imgui.separator_text("Textures")

        # Texture toggles
        if renderer is not None:
            t = renderer.toggles
            for label, attr in [
                ("Diffuse##cm",    "diffuse"),
                ("Normal Map##cm", "normal"),
                ("Specular##cm",   "specular"),
                ("Env Map##cm",    "env_map"),
            ]:
                c, val = imgui.checkbox(label, getattr(t, attr))
                if c:
                    setattr(t, attr, val)

        imgui.spacing()
        imgui.separator_text("Lighting")

        # Lighting preset
        from creation_lib.renderer.lighting import LIGHTING_PRESETS, LIGHT_TYPE_KEYS, LIGHT_TYPE_LABELS
        _LIGHTING_KEYS = list(LIGHTING_PRESETS.keys())
        _LIGHTING_LABELS = ["Studio", "Dramatic", "Outdoor"]
        if scene_settings is not None:
            cur_idx = (_LIGHTING_KEYS.index(scene_settings.lighting_preset)
                       if scene_settings.lighting_preset in _LIGHTING_KEYS else 0)
            imgui.set_next_item_width(150)
            c, new_idx = imgui.combo("Preset##cm", cur_idx, _LIGHTING_LABELS)
            if c:
                scene_settings.lighting_preset = _LIGHTING_KEYS[new_idx]
                scene_settings.apply_to(app)

        # Light type
        if scene_settings is not None:
            cur_lt = (LIGHT_TYPE_KEYS.index(scene_settings.light_type)
                      if scene_settings.light_type in LIGHT_TYPE_KEYS else 0)
            imgui.set_next_item_width(150)
            c, new_lt = imgui.combo("Light Type##cm", cur_lt, LIGHT_TYPE_LABELS)
            if c:
                scene_settings.light_type = LIGHT_TYPE_KEYS[new_lt]
                scene_settings.apply_to(app)

        # Lighting sliders
        for label, attr, lo, hi in [
            ("Env Boost##cm",     "_dbg_envBoost",     0.0, 20.0),
            ("Exposure##cm",      "_dbg_exposure",     0.5, 40.0),
            ("Spec Boost##cm",    "_dbg_specBoost",    0.0, 20.0),
            ("Ambient Boost##cm", "_dbg_ambientBoost", 0.0, 10.0),
        ]:
            imgui.set_next_item_width(150)
            c, val = imgui.slider_float(label, getattr(app, attr, 1.0), lo, hi, "%.2f")
            if c:
                setattr(app, attr, val)

        imgui.spacing()

        # Navigation style
        _NAV_KEYS = ["3dsmax", "blender", "default"]
        _NAV_LABELS = [
            "3ds Max  (Alt+MMB=orbit)",
            "Blender  (MMB=orbit)",
            "Default  (Ctrl+LMB=orbit)",
        ]
        if scene_settings is not None:
            cur_nav = (_NAV_KEYS.index(scene_settings.nav_style)
                       if scene_settings.nav_style in _NAV_KEYS else 0)
            imgui.set_next_item_width(150)
            c, new_nav = imgui.combo("Navigation##cm", cur_nav, _NAV_LABELS)
            if c:
                scene_settings.nav_style = _NAV_KEYS[new_nav]
                scene_settings.apply_to(app)

    def _draw_cloth_info(self, scene) -> None:
        """Draw cloth data info and display toggles."""
        cj = scene.cloth_json or {}
        scd = scene.active_sim_cloth

        # --- Cloth Info ---
        imgui.separator_text("Cloth Info")

        sim_cloths = cj.get("sim_cloths", [])
        imgui.text(f"Name: {cj.get('name', '')}")
        imgui.text(f"Sim cloths: {len(sim_cloths)}")

        if scd is not None:
            imgui.spacing()
            particles = scd.get("particles", [])
            imgui.text(f"Particles: {len(particles)}")
            imgui.text(f"  Fixed: {len(scd.get('fixed_particle_indices', []))}")

            # Constraint breakdown
            constraint_sets = scd.get("constraint_sets", [])
            total_links = len(scene.constraint_links)
            imgui.text(f"Constraints: {total_links}")
            for cs in constraint_sets:
                class_name = cs.get("class_name", "")
                cn = class_name.replace("hcl", "").replace("ConstraintSet", "")
                count = cs.get("link_count", len(cs.get("links", [])))
                imgui.text(f"  {cn}: {count}")

            # Collidables
            imgui.text(f"Collidables: {len(scd.get('collidables', []))}")
            imgui.text(f"  Capsules: {len(scene.capsules)}")
            imgui.text(f"  Spheres: {len(scene.spheres)}")

        imgui.spacing()
        imgui.text(f"Operators: {len(cj.get('operators', []))}")
        imgui.text(f"States: {len(cj.get('cloth_states', []))}")

        # --- Display Toggles ---
        imgui.spacing()
        imgui.separator_text("Display")

        _, scene.show_particles = imgui.checkbox(
            "Particles", scene.show_particles,
        )
        _, scene.show_constraints = imgui.checkbox(
            "Constraints", scene.show_constraints,
        )
        _, scene.show_capsules = imgui.checkbox(
            "Capsules", scene.show_capsules,
        )
        _, scene.show_pins = imgui.checkbox(
            "Pin markers", scene.show_pins,
        )
