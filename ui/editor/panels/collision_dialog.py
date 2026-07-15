"""Generate Collision modal dialog.

Drop-in modal invoked from the scene tree context menu (right-click on an
NiNode subtype) and from the toolbar Tools menu. Wraps
`creation_lib.nif.operations.collision.generate_collision` with a small form:
shape type, layer, mass/friction/restitution/radius sliders, and a
replace-existing checkbox.
"""

from __future__ import annotations

import logging

from imgui_bundle import imgui

_log = logging.getLogger("nif_editor.collision_dialog")


_SHAPE_LABELS = [
    "Convex Hull",
    "Auto Convex Hull",
    "Box",
    "Capsule",
    "Cylinder",
    "Sphere",
    "Auto Best-Fit",
    "Compound (List)",
    "MOPP",
    "Compressed Mesh",
]
_SHAPE_KEYS = [
    "convex_hull",
    "convex_fit",
    "box",
    "capsule",
    "cylinder",
    "sphere",
    "auto",
    "list",
    "mopp",
    "compressed_mesh",
]

_LAYER_LABELS = [
    "STATIC",
    "ANIMSTATIC",
    "CLUTTER",
    "WEAPON",
    "PROJECTILE",
    "TERRAIN",
]
_FALLBACK_MATERIAL_OPTIONS = [
    {"label": "Generic", "value": 186875565},
]


class CollisionDialog:
    """Modal popup for generating collision on a target node."""

    POPUP_TITLE = "Generate Collision"

    def __init__(self, app):
        self.app = app
        self._open = False
        self._pending_open = False
        self._target_node_id: int = 0
        self._source_block_ids: list[int] | None = None
        self._shape_type: int = 1     # Auto Convex Hull
        self._layer: int = 0           # STATIC
        self._material: int = 0
        self._material_options: list[dict[str, object]] = []
        self._mass: float = 0.0
        self._friction: float = 0.5
        self._restitution: float = 0.4
        self._radius: float = 0.05
        self._replace: bool = True
        self._include_child_nodes: bool = True
        self._log_msg: str = ""
        self._log_is_error: bool = False

    # -- lifecycle -----------------------------------------------------

    def open(self, node_block_id: int, source_block_ids: list[int] | None = None) -> None:
        """Open the modal, targeting the given NiNode block id."""
        self._target_node_id = int(node_block_id)
        self._source_block_ids = [int(sid) for sid in source_block_ids] if source_block_ids else None
        self._log_msg = ""
        self._log_is_error = False
        self._material_options = []
        self._material = 0
        # Reset to sensible defaults each time the dialog is opened.
        self._mass = 0.0
        self._friction = 0.5
        self._restitution = 0.4
        self._radius = 0.05
        self._replace = True
        self._include_child_nodes = True
        # Defer open_popup() to draw() — callers (context menu, toolbar) are
        # inside a different ImGui ID scope, so calling open_popup from here
        # registers it at the wrong stack level and begin_popup_modal silently
        # fails to match it.
        self._pending_open = True

    # -- rendering -----------------------------------------------------

    def _target_label(self) -> str:
        return self._block_label(self._target_node_id)

    def _source_label(self) -> str | None:
        if not self._source_block_ids:
            return None
        return ", ".join(self._block_label(sid) for sid in self._source_block_ids)

    def _block_label(self, block_id: int) -> str:
        nif = getattr(self.app, "nif_file", None)
        if nif is None:
            return f"Block #{block_id}"
        block = nif.get_block(block_id)
        if block is None:
            return f"Block #{block_id} (missing)"
        name = block.get_field("Name") or ""
        if isinstance(name, list):
            name = "".join(str(c) for c in name)
        if name:
            return f"[{block_id}] {block.type_name} \"{name}\""
        return f"[{block_id}] {block.type_name}"

    def draw(self) -> None:
        """Draw the modal when open. Safe to call every frame."""
        if self._pending_open:
            imgui.open_popup(self.POPUP_TITLE)
            self._pending_open = False
            self._open = True

        if not self._open:
            return

        flags = imgui.WindowFlags_.always_auto_resize.value
        opened, _visible = imgui.begin_popup_modal(self.POPUP_TITLE, True, flags)
        if not opened:
            # User clicked outside or the popup is no longer in the stack.
            self._open = False
            return

        imgui.text("Target:")
        imgui.same_line()
        imgui.text_colored(imgui.ImVec4(0.8, 0.9, 1.0, 1.0), self._target_label())
        source_label = self._source_label()
        if source_label:
            imgui.text("Source:")
            imgui.same_line()
            imgui.text_colored(imgui.ImVec4(0.8, 0.9, 1.0, 1.0), source_label)
        imgui.separator()

        imgui.set_next_item_width(220)
        _, self._shape_type = imgui.combo(
            "Shape##coll_dlg", self._shape_type, _SHAPE_LABELS
        )
        imgui.set_next_item_width(220)
        _, self._layer = imgui.combo(
            "Layer##coll_dlg", self._layer, _LAYER_LABELS
        )
        material_options = self._get_material_options()
        material_labels = [str(option["label"]) for option in material_options]
        if self._material >= len(material_labels):
            self._material = 0
        imgui.set_next_item_width(220)
        _, self._material = imgui.combo(
            "Material##coll_dlg", self._material, material_labels
        )

        imgui.set_next_item_width(220)
        _, self._mass = imgui.slider_float(
            "Mass##coll_dlg", self._mass, 0.0, 1000.0, "%.2f"
        )
        imgui.set_next_item_width(220)
        _, self._friction = imgui.slider_float(
            "Friction##coll_dlg", self._friction, 0.0, 2.0, "%.3f"
        )
        imgui.set_next_item_width(220)
        _, self._restitution = imgui.slider_float(
            "Restitution##coll_dlg", self._restitution, 0.0, 1.0, "%.3f"
        )
        imgui.set_next_item_width(220)
        _, self._radius = imgui.slider_float(
            "Radius##coll_dlg", self._radius, 0.001, 5.0, "%.3f"
        )

        _, self._replace = imgui.checkbox(
            "Replace existing collision", self._replace
        )
        if self._source_block_ids is None:
            _, self._include_child_nodes = imgui.checkbox(
                "Use child NiNode meshes", self._include_child_nodes
            )

        if self._log_msg:
            imgui.separator()
            color = (
                imgui.ImVec4(0.95, 0.4, 0.4, 1.0)
                if self._log_is_error
                else imgui.ImVec4(0.4, 0.9, 0.4, 1.0)
            )
            imgui.push_style_color(imgui.Col_.text.value, color)
            imgui.text_wrapped(self._log_msg)
            imgui.pop_style_color()

        imgui.separator()
        if imgui.button("Generate", imgui.ImVec2(120, 0)):
            self._apply()
        imgui.same_line()
        if imgui.button("Close", imgui.ImVec2(120, 0)):
            self._open = False
            imgui.close_current_popup()

        imgui.end_popup()

    # -- action --------------------------------------------------------

    def _game_profile(self):
        try:
            session = self.app.registry.active_session
            return getattr(session, "game_profile", None)
        except (AttributeError, KeyError):
            return None

    def _get_material_options(self) -> list[dict[str, object]]:
        if self._material_options:
            return self._material_options
        try:
            from creation_lib.nif.operations.collision_materials import (
                default_collision_material,
                get_collision_material_options,
            )

            options = get_collision_material_options(self._game_profile())
            default_value = default_collision_material(self._game_profile())
        except Exception:
            options = list(_FALLBACK_MATERIAL_OPTIONS)
            default_value = int(options[0]["value"])
        if not options:
            options = list(_FALLBACK_MATERIAL_OPTIONS)
        self._material_options = options
        for index, option in enumerate(options):
            if int(option["value"]) == int(default_value):
                self._material = index
                break
        return self._material_options

    def _apply(self) -> None:
        nif = getattr(self.app, "nif_file", None)
        if nif is None:
            self._log_msg = "No NIF loaded"
            self._log_is_error = True
            return

        from creation_lib.nif.operations.collision import generate_collision

        shape_type = _SHAPE_KEYS[self._shape_type]
        layer = _LAYER_LABELS[self._layer]
        material_options = self._get_material_options()
        material = material_options[self._material]["value"]
        profile = self._game_profile()
        if profile is None:
            self._log_msg = "Error: no game profile resolved for collision generation"
            self._log_is_error = True
            return

        try:
            result = generate_collision(
                nif,
                node_block_id=self._target_node_id,
                shape_type=shape_type,
                layer=layer,
                material=material,
                mass=self._mass,
                friction=self._friction,
                restitution=self._restitution,
                radius=self._radius,
                replace=self._replace,
                source_block_ids=self._source_block_ids,
                include_child_nodes=self._include_child_nodes,
                profile=profile,
            )
        except Exception as exc:
            _log.exception("generate_collision raised")
            self._log_msg = f"Error: {exc}"
            self._log_is_error = True
            return

        if result.success:
            self._log_msg = result.description or "Collision generated."
            self._log_is_error = False
            if hasattr(self.app, "_nif_dirty"):
                self.app._nif_dirty = True
            try:
                if hasattr(self.app, "rebuild_scene_from_nif"):
                    self.app.rebuild_scene_from_nif()
            except Exception:
                _log.exception("rebuild_scene_from_nif failed after generate_collision")
            renderer = getattr(self.app, "renderer", None)
            if renderer is not None:
                try:
                    renderer._show_collision = True
                    renderer._collision_dirty = True
                except Exception:
                    pass
        else:
            msg = f"Error: {result.description}" if result.description else "Generate collision failed"
            warnings = getattr(result, "warnings", None) or []
            if warnings:
                msg += "\n" + "\n".join(f"Warning: {w}" for w in warnings)
            self._log_msg = msg
            self._log_is_error = True
