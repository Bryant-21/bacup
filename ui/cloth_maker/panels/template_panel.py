"""Template panel — cloth template library browser and applicator.

Lists available cloth templates via the native cloth_template_list
pyfunction, lets the user pick a parent bone, and applies the
template (nif_core_native.cloth_template_apply) to generate cloth setup data.
"""
from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ui.cloth_maker.cloth_maker_app import ClothMakerApp

_log = logging.getLogger("cloth_maker.template_panel")


class TemplatePanel:
    """Template library browser with apply functionality."""

    def __init__(self, app: ClothMakerApp):
        self.app = app

        # Template registry state
        self._templates: list[dict] = []
        self._selected_idx: int = -1
        self._registry_loaded: bool = False

        # Parent bone selection
        self._bone_names: list[str] = []
        self._selected_bone_idx: int = 0

        # Apply state
        self._applying: bool = False
        self._apply_error: str = ""
        self._apply_success: str = ""

    def _load_registry(self) -> None:
        """Load available templates from the template registry."""
        if self._registry_loaded:
            return

        try:
            import json
            from creation_lib._native import havok_native as _hn
            from creation_lib._native import nif_core_native as _nif
            summaries = json.loads(_hn.cloth_template_list())
            self._templates = [
                {
                    "name": s["name"],
                    "description": s.get("description", ""),
                    "category": s.get("category", "General"),
                    "particle_count": s.get("num_particles", "?"),
                    "id": s["name"].lower().replace(" ", "_"),
                }
                for s in summaries
            ]
            self._registry_loaded = True
            _log.info("Loaded %d cloth templates", len(self._templates))
        except Exception as e:
            _log.warning("Failed to load template registry: %s", e)
            self._templates = []
            self._registry_loaded = True

    def _load_bone_names(self) -> None:
        """Extract bone names from the loaded NIF for parent bone selection."""
        if self._bone_names:
            return

        if not self.app.scene.loaded:
            return

        try:
            # Try to get bone names from the skinned mesh
            if self.app.nif_file:
                from creation_lib.nif import NifFile
                bones = []
                for node in self.app.nif_file.get_nodes():
                    name = getattr(node, "name", "")
                    if name and name not in bones:
                        bones.append(name)
                self._bone_names = sorted(bones) if bones else ["COM"]
            else:
                self._bone_names = ["COM"]
        except Exception:
            self._bone_names = ["COM"]

    def draw(self) -> None:
        visible, _ = imgui.begin("Templates##cloth_maker")
        if not visible:
            imgui.end()
            return

        self._load_registry()

        if not self._templates:
            self._draw_empty_state()
            imgui.end()
            return

        self._draw_template_list()
        imgui.spacing()
        self._draw_template_details()
        imgui.spacing()
        self._draw_bone_picker()
        imgui.spacing()
        self._draw_apply_section()

        imgui.end()

    def _draw_empty_state(self) -> None:
        imgui.text_colored(
            imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
            "No cloth templates available.",
        )
        imgui.spacing()
        imgui.text_disabled(
            "Templates will appear here once the template\n"
            "library is installed in py_creation_lib/python/creation_lib/havok_cloth/templates/."
        )

    def _draw_template_list(self) -> None:
        imgui.separator_text("Template Library")

        # Group by category
        categories: dict[str, list[int]] = {}
        for i, t in enumerate(self._templates):
            cat = t.get("category", "General")
            categories.setdefault(cat, []).append(i)

        for cat_name in sorted(categories.keys()):
            if imgui.tree_node(f"{cat_name} ({len(categories[cat_name])})"):
                for idx in categories[cat_name]:
                    t = self._templates[idx]
                    is_selected = (idx == self._selected_idx)
                    if imgui.selectable(
                        f"{t['name']}##{idx}",
                        is_selected,
                    )[0]:
                        self._selected_idx = idx
                        self._apply_error = ""
                        self._apply_success = ""
                imgui.tree_pop()

    def _draw_template_details(self) -> None:
        if self._selected_idx < 0 or self._selected_idx >= len(self._templates):
            imgui.text_disabled("Select a template above")
            return

        t = self._templates[self._selected_idx]
        imgui.separator_text("Details")
        imgui.text(f"Name: {t['name']}")
        imgui.text_wrapped(f"Description: {t['description']}")
        imgui.text(f"Category: {t.get('category', 'General')}")
        pc = t.get("particle_count", "?")
        imgui.text(f"Particles: {pc}")

    def _draw_bone_picker(self) -> None:
        imgui.separator_text("Parent Bone")

        if not self.app.scene.loaded:
            imgui.text_disabled("Import a NIF first to select parent bone")
            return

        self._load_bone_names()

        if not self._bone_names:
            imgui.text_disabled("No bones found in NIF")
            return

        changed, self._selected_bone_idx = imgui.combo(
            "Parent Bone##template",
            self._selected_bone_idx,
            self._bone_names,
        )

    def _draw_apply_section(self) -> None:
        imgui.separator()
        imgui.spacing()

        can_apply = (
            self._selected_idx >= 0
            and self.app.scene.loaded
            and not self._applying
        )

        if not can_apply:
            imgui.begin_disabled()
        if imgui.button("Apply Template", imgui.ImVec2(-1, 30)):
            self._apply_template()
        if not can_apply:
            imgui.end_disabled()

        if self._applying:
            imgui.text("Applying template...")

        if self._apply_error:
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
                f"Error: {self._apply_error}",
            )

        if self._apply_success:
            imgui.text_colored(
                imgui.ImVec4(0.3, 1.0, 0.3, 1.0),
                self._apply_success,
            )

        imgui.spacing()
        imgui.text_disabled("Applies template to the loaded NIF mesh.")
        imgui.text_disabled("Export NIF to save the result.")

    def _apply_template(self) -> None:
        """Apply the selected template to the current scene."""
        if self._selected_idx < 0 or self._selected_idx >= len(self._templates):
            return

        t = self._templates[self._selected_idx]
        parent_bone = (
            self._bone_names[self._selected_bone_idx]
            if self._bone_names and self._selected_bone_idx < len(self._bone_names)
            else "COM"
        )

        self._applying = True
        self._apply_error = ""
        self._apply_success = ""

        self.app.push_undo("Apply template")

        try:
            import json
            from pathlib import Path
            from creation_lib._native import havok_native as _hn

            nif_path = self.app.scene.nif_path
            source_nif_bytes = Path(nif_path).read_bytes()
            args = {"parent_bone": parent_bone}
            nif_bytes = bytes(_nif.cloth_template_apply(t["name"], source_nif_bytes, json.dumps(args)))

            # Write back to the same path and reload scene
            Path(nif_path).write_bytes(nif_bytes)
            self.app.scene.load_from_nif(nif_path)

            # Invalidate param panel so it reloads from updated graph
            if self.app.param_panel:
                self.app.param_panel._dirty = True

            # Invalidate preview panel solver
            if self.app.preview_panel and self.app.preview_panel.solver is not None:
                self.app.preview_panel.solver = None

            tmpl_info = json.loads(_hn.cloth_template_get(t["name"]))
            particle_count = tmpl_info.get("num_particles", "?")
            self._apply_success = (
                f"Applied '{t['name']}' to bone '{parent_bone}' "
                f"({particle_count} particles)"
            )
            self.app.status_text = self._apply_success
            _log.info("Applied template '%s' with parent bone '%s'",
                      t["name"], parent_bone)

        except Exception as e:
            self._apply_error = str(e)
            _log.error("Failed to apply template: %s", e, exc_info=True)
        finally:
            self._applying = False
