"""Property editor panel — schema-aware imgui widgets for NIF block fields.

Reads the selected block's fields and generates appropriate imgui widgets
using schema metadata (enums as dropdowns, bitflags as checkboxes,
bitfields as grouped sub-editors, refs with Go/Clear buttons).
All edits are routed through the undo manager.
"""

import copy
import logging
import math
import traceback

from imgui_bundle import imgui

from creation_lib.nif.nif_bsx_flags import BSX_FLAG_DEFS
from .collision_info import (
    is_collision_block,
    summarize_collision_block,
    _COLL_OBJ_FLAGS,
)
from .properties_header import HEADER_BLOCK_ID, draw_header_props

_log = logging.getLogger("nif_editor.properties")

_BHK_COLL_OBJ_TYPES = frozenset({
    "bhkCollisionObject",
    "bhkNPCollisionObject",
    "bhkBlendCollisionObject",
    "bhkSPCollisionObject",
})

NIF_FLOAT_SENTINEL_THRESHOLD = 3.4e38
DEFAULT_FLOAT_FIELD_SLIDER_BOUNDS = (-100.0, 100.0)


def is_nif_float_sentinel(value: float) -> bool:
    return math.isinf(float(value)) or abs(float(value)) >= NIF_FLOAT_SENTINEL_THRESHOLD


def float_field_slider_bounds(name: str, value: float) -> tuple[float, float] | None:
    if is_nif_float_sentinel(value):
        return None
    if name == "Grayscale to Palette Scale":
        return 0.0, 1.0
    return DEFAULT_FLOAT_FIELD_SLIDER_BOUNDS


def enum_option_names_and_values(enum_def) -> tuple[list[str], list[int]]:
    return [opt.name for opt in enum_def.options], [opt.value for opt in enum_def.options]


class PropertiesPanel:
    """imgui panel that shows editable properties of the selected NIF block."""

    def __init__(self, app):
        self.app = app
        self._visible = True
        self.window_name = "Properties"
        self._selected_block_id = None
        self._selected_nif_id = None
        # Cache schema lookups per block type
        self._cached_type = None
        self._cached_fdefs = {}
        # Drag/slider undo coalescing: only push undo on release
        self._drag_field: str | None = None  # field key being dragged
        self._drag_old_value = None  # value before drag started

        if hasattr(app, "selection_mgr"):
            app.selection_mgr.on_selection_changed(self._on_select)

    def _on_select(self, nif_id, block_id):
        self._selected_nif_id = nif_id
        self._selected_block_id = block_id

    def _get_game_profile(self):
        """Get the active session's game profile, or None."""
        try:
            session = self.app.registry.active_session
            return getattr(session, "game_profile", None)
        except (AttributeError, KeyError):
            return None

    def _get_fdefs(self, block):
        """Get FieldDef map for a block, cached by type name."""
        if block.type_name != self._cached_type:
            self._cached_type = block.type_name
            nif = self.app.nif_file
            if nif:
                from creation_lib.nif.schema import build_field_def_map
                self._cached_fdefs = build_field_def_map(nif.schema, block.type_name)
        return self._cached_fdefs

    def draw(self):
        """Draw the properties panel."""
        if not self._visible:
            return

        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return

        # Use the NIF that owns the selected block, not just the active session
        nif_id = getattr(self, "_selected_nif_id", None)

        # Header sentinel — render NIF header instead of a block
        if self._selected_block_id == HEADER_BLOCK_ID:
            target_id = nif_id if nif_id in self.app.registry.sessions else (
                getattr(self.app.registry, "active_id", None)
            )
            if target_id is not None:
                draw_header_props(self.app, target_id)
            imgui.end()
            return

        if nif_id and nif_id in self.app.registry.sessions:
            nif = self.app.registry.get_session(nif_id).nif
        else:
            nif = self.app.nif_file
        if not nif or self._selected_block_id is None:
            imgui.text_colored(
                imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
                f"No selection (nif_id={nif_id}, bid={self._selected_block_id}, has_nif={nif is not None})",
            )
            imgui.end()
            return

        block = nif.get_block(self._selected_block_id)
        if not block:
            imgui.text_colored(imgui.ImVec4(0.8, 0.3, 0.3, 1.0), "Block not found")
            imgui.end()
            return

        # Header
        imgui.text_colored(
            imgui.ImVec4(0.9, 0.8, 0.5, 1.0),
            f"[{block.block_id}] {block.type_name}",
        )
        imgui.separator()

        # Collision summary — show decoded layer/flags/shape info before raw fields.
        # Also surfaced on nodes that have a Collision Object attached, so users can
        # see the shape/layer without navigating into the bhk subtree.
        detail_lines: list[str] = []
        detail_header: str | None = None
        if is_collision_block(block.type_name):
            _, detail_lines = summarize_collision_block(nif, block)
        else:
            coll_ref = block.get_field("Collision Object")
            if isinstance(coll_ref, int) and coll_ref >= 0:
                coll_obj = nif.get_block(coll_ref)
                if coll_obj is not None:
                    _, detail_lines = summarize_collision_block(nif, coll_obj)
                    if detail_lines:
                        detail_header = "Attached Collision:"
        if detail_lines:
            imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.9, 0.7, 0.4, 1.0))
            if detail_header:
                imgui.text(detail_header)
            for line in detail_lines:
                imgui.text(line)
            imgui.pop_style_color()
            imgui.separator()

        # Scrolling field list
        imgui.begin_child("props_scroll", imgui.ImVec2(0, 0))

        try:
            self.draw_block_fields(
                nif_id or getattr(self.app.registry, "active_id", None),
                nif,
                block,
            )
        except Exception:
            imgui.text_colored(
                imgui.ImVec4(0.8, 0.3, 0.3, 1.0), "Error rendering properties"
            )
            _log.error("properties draw error:\n%s", traceback.format_exc())

        imgui.end_child()
        imgui.end()

    def draw_block_fields(
        self,
        nif_id,
        nif,
        block,
        skip_field_names: set[str] | frozenset[str] | tuple[str, ...] = (),
    ) -> None:
        """Draw schema-aware editable fields for a NIF block."""
        if nif is None or block is None:
            return

        if nif_id is not None:
            self._selected_nif_id = nif_id
            registry = getattr(self.app, "registry", None)
            sessions = getattr(registry, "sessions", {})
            if registry is not None and nif_id in sessions:
                registry.active_id = nif_id
        self._selected_block_id = block.block_id

        fdefs = self._fdefs_for_nif(nif, block)
        schema = nif.schema
        skip = set(skip_field_names)

        has_transform = (
            block.get_field("Translation") is not None
            and block.get_field("Rotation") is not None
        )
        transform_drawn = False

        for field_name, field_value in block.fields:
            if field_name in skip:
                continue
            if has_transform and field_name in ("Translation", "Rotation", "Scale"):
                if not transform_drawn:
                    transform_drawn = True
                    self._draw_transform_group(block)
                continue

            fdef = fdefs.get(field_name)
            try:
                self._draw_field(block, field_name, field_value, fdef, schema)
            except Exception:
                imgui.text_colored(
                    imgui.ImVec4(0.8, 0.3, 0.3, 1.0),
                    f"{field_name}: <render error>",
                )
                _log.debug(
                    "field render error: %s\n%s", field_name, traceback.format_exc()
                )

    def _fdefs_for_nif(self, nif, block) -> dict[str, object]:
        from creation_lib.nif.schema import build_field_def_map
        return build_field_def_map(nif.schema, block.type_name)

    def _draw_field(self, block, name: str, value, fdef, schema):
        """Draw an appropriate imgui widget for a field value."""
        if block.type_name == "BSXFlags" and name == "Integer Data":
            self._draw_bsx_flags(block, name, value)
            return

        if block.type_name in _BHK_COLL_OBJ_TYPES and name == "Flags":
            self._draw_bhk_coll_flags(block, name, value)
            return

        # None values
        if value is None:
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), f"{name}: None")
            return

        # Bytes / bytearray — hex viewer (16-byte rows: offset, hex, ASCII)
        if isinstance(value, (bytes, bytearray)):
            if imgui.tree_node(f"{name} [{len(value)} bytes]"):
                imgui.begin_child(
                    f"hex_{name}", imgui.ImVec2(0, 200), imgui.ChildFlags_.borders.value
                )
                for offset in range(0, len(value), 16):
                    row = value[offset : offset + 16]
                    hex_str = " ".join(f"{b:02X}" for b in row)
                    ascii_str = "".join(chr(b) if 32 <= b < 127 else "." for b in row)
                    imgui.text(f"{offset:08X}  {hex_str:<48s}  {ascii_str}")
                imgui.end_child()
                imgui.tree_pop()
            return

        # Large arrays — virtual-scrolled via begin_child region
        if isinstance(value, list) and len(value) > 20:
            # Special case: Vertex Data gets a columnar table view
            if name == "Vertex Data" and len(value) > 0 and isinstance(value[0], dict):
                self._draw_vertex_table(block, name, value)
                return
            if imgui.tree_node(f"{name} [{len(value)} items]"):
                imgui.begin_child(
                    f"array_{name}",
                    imgui.ImVec2(0, 300),
                    imgui.ChildFlags_.borders.value,
                )
                # Manual virtual scroll: only draw visible rows
                item_height = imgui.get_text_line_height_with_spacing()
                scroll_y = imgui.get_scroll_y()
                visible_height = 300.0
                first = max(0, int(scroll_y / item_height) - 1)
                last = min(
                    len(value), int((scroll_y + visible_height) / item_height) + 2
                )
                # Spacer for items above visible range
                if first > 0:
                    imgui.dummy(imgui.ImVec2(0, first * item_height))
                for i in range(first, last):
                    self._draw_field(block, f"{name}[{i}]", value[i], None, schema)
                # Spacer for items below visible range
                remaining = len(value) - last
                if remaining > 0:
                    imgui.dummy(imgui.ImVec2(0, remaining * item_height))
                imgui.end_child()
                imgui.tree_pop()
            return

        # Schema-aware type dispatch (check before isinstance fallbacks)
        if fdef and schema:
            ftype = fdef.type

            # Enum dropdown
            if ftype in schema.enums:
                self._draw_enum(block, name, value, schema.enums[ftype])
                return

            # Bitflag checkboxes
            if ftype in schema.bitflags:
                self._draw_bitflags(block, name, value, schema.bitflags[ftype])
                return

            # Bitfield sub-editors
            if ftype in schema.bitfields:
                self._draw_bitfield(block, name, value, schema.bitfields[ftype], schema)
                return

            # Ref/Ptr — schema-based detection
            if ftype in ("Ref", "Ptr"):
                self._draw_ref(block, name, value)
                return

            # Color types — schema-based detection
            if ftype in ("Color3", "ByteColor3"):
                color = _ensure_color3(value)
                self._draw_color(block, name, color)
                return
            if ftype in ("Color4", "ByteColor4", "ByteColor4BGRA"):
                color = _ensure_color4(value)
                self._draw_color(block, name, color)
                return

        # StringIndex fields — dropdown from header string table
        if fdef and schema and fdef.type == "StringIndex" and isinstance(value, int):
            self._draw_string_index(block, name, value)
            return

        # NIF dataclass types (Color3, Color4, Vector3, etc.) — handle before dict check
        if _is_color(value):
            color = (
                _ensure_color4(value) if hasattr(value, "a") else _ensure_color3(value)
            )
            self._draw_color(block, name, color)
            return

        # Dictionary (struct) — expand as sub-fields
        if isinstance(value, dict):
            if _is_quaternion(value):
                self._draw_quaternion(block, name, value)
            elif _is_vector3(value):
                self._draw_vector3(block, name, value)
            elif _is_color(value):
                self._draw_color(block, name, value)
            elif _is_matrix33(value):
                self._draw_matrix33(block, name, value)
            else:
                self._draw_struct(block, name, value, fdef, schema)
            return

        # Ref/Ptr fallback (heuristic for fields without schema)
        if isinstance(value, int) and _looks_like_ref(name):
            self._draw_ref(block, name, value)
            return

        # Simple types
        if isinstance(value, bool):
            changed, new_val = imgui.checkbox(name, value)
            if changed:
                self._set_field(block, name, value, new_val)
            return

        if isinstance(value, int):
            # imgui.input_int is 32-bit; show large values as read-only text
            if value > 0x7FFFFFFF or value < -0x80000000:
                imgui.text(f"{name}: {value} (0x{value:X})")
            else:
                changed, new_val = imgui.input_int(name, value)
                if changed:
                    self._set_field(block, name, value, new_val)
            return

        if isinstance(value, float):
            # FLT_MAX / -FLT_MAX are NIF sentinel values (unbounded time range).
            # slider_float can't handle them — show as read-only text.
            bounds = float_field_slider_bounds(name, value)
            if bounds is None:
                sentinel = "FLT_MAX" if value > 0 else "-FLT_MAX"
                imgui.text(f"{name}: {sentinel}  ({value:.6g})")
                return
            # Special slider for Grayscale to Palette Scale (0-1 range, live viewport)
            # Only show for FO4 (greyscale-to-palette is FO4-only)
            if name == "Grayscale to Palette Scale":
                profile = self._get_game_profile()
                if profile and profile.id != "fo4":
                    imgui.text_colored(
                        imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
                        f"{name}: {value:.3f}  (FO4 only)",
                    )
                    return
                changed, new_val = imgui.slider_float(name, value, *bounds, format="%.3f")
                if changed:
                    self._set_field_drag(block, name, value, new_val)
                    self._push_palette_scale(block, new_val)
                self._check_drag_release(block, name, new_val if changed else value)
                return
            changed, new_val = imgui.slider_float(name, value, *bounds, "%.4f")
            if changed:
                self._set_field_drag(block, name, value, new_val)
            self._check_drag_release(block, name, new_val if changed else value)
            return

        if isinstance(value, str):
            # Check if this is a file path field
            if _is_path_field(block, name):
                self._draw_path_field(block, name, value)
            else:
                changed, new_val = imgui.input_text(name, value)
                if changed:
                    self._set_field(block, name, value, new_val)
            return

        # Small lists (e.g., short arrays)
        if isinstance(value, list) and len(value) <= 20:
            if imgui.tree_node(f"{name} [{len(value)}]"):
                for i, item in enumerate(value):
                    self._draw_field(block, f"{name}[{i}]", item, None, schema)
                imgui.tree_pop()
            return

        # Fallback: display as text (truncated)
        text = str(value)
        if len(text) > 120:
            text = text[:120] + "..."
        imgui.text(f"{name}: {text}")

    # -------------------------------------------------------------------
    # Schema-aware widgets
    # -------------------------------------------------------------------

    def _draw_enum(self, block, name: str, value, enum_def):
        """Draw an enum field as a combo dropdown."""
        # Build option list
        options = enum_def.options
        option_names = [opt.name for opt in options]
        option_values = [opt.value for opt in options]

        # Find current index
        int_val = int(value) if isinstance(value, (int, float)) else 0
        current_idx = -1
        for i, ov in enumerate(option_values):
            if ov == int_val:
                current_idx = i
                break

        # Display label: show name if found, else raw value
        if current_idx >= 0:
            preview = option_names[current_idx]
        else:
            preview = f"Unknown ({int_val})"

        changed, new_idx = imgui.combo(name, current_idx, option_names)
        if changed and 0 <= new_idx < len(option_values):
            self._set_field(block, name, value, option_values[new_idx])

    def _draw_bitflags(self, block, name: str, value, bitflag_def):
        """Draw bitflags as a collapsible tree of checkboxes."""
        int_val = int(value) if isinstance(value, (int, float)) else 0

        label = f"{name}: 0x{int_val:X}"
        if imgui.tree_node(label):
            new_val = int_val
            for opt in bitflag_def.options:
                bit_mask = 1 << opt.value
                is_set = bool(int_val & bit_mask)
                changed, checked = imgui.checkbox(
                    f"{opt.name} (bit {opt.value})##{name}_{opt.name}",
                    is_set,
                )
                if changed:
                    if checked:
                        new_val |= bit_mask
                    else:
                        new_val &= ~bit_mask

            if new_val != int_val:
                self._set_field(block, name, value, new_val)

            imgui.tree_pop()

    def _draw_bsx_flags(self, block, name: str, value):
        """Draw BSXFlags.Integer Data with named checkboxes instead of a raw int."""
        int_val = int(value) if isinstance(value, (int, float)) else 0

        label = f"{name}: {int_val} (0x{int_val:X})"
        if imgui.tree_node(label):
            new_val = int_val
            for flag in BSX_FLAG_DEFS:
                is_set = bool(int_val & flag.mask)
                changed, checked = imgui.checkbox(
                    f"Bit {flag.bit}: {flag.label} ({flag.mask})##bsx_{flag.bit}",
                    is_set,
                )
                if flag.description:
                    imgui.same_line()
                    imgui.text_disabled(f"- {flag.description}")
                if changed:
                    if checked:
                        new_val |= flag.mask
                    else:
                        new_val &= ~flag.mask

            if new_val != int_val:
                self._set_field(block, name, value, new_val)

            imgui.tree_pop()

    def _draw_bhk_coll_flags(self, block, name: str, value):
        """Draw bhkCollisionObject.Flags as named checkboxes."""
        int_val = int(value) if isinstance(value, (int, float)) else 0
        named = [n for bit, n in _COLL_OBJ_FLAGS.items() if int_val & bit]
        label = f"{name}: {', '.join(named) if named else f'0x{int_val:02X}'}"
        if imgui.tree_node(label):
            new_val = int_val
            for bit, flag_name in _COLL_OBJ_FLAGS.items():
                is_set = bool(int_val & bit)
                changed, checked = imgui.checkbox(
                    f"{flag_name} (0x{bit:03X})##bhk_flag_{bit}", is_set
                )
                if changed:
                    if checked:
                        new_val |= bit
                    else:
                        new_val &= ~bit
            if new_val != int_val:
                self._set_field(block, name, value, new_val)
            imgui.tree_pop()

    def _draw_bitfield(self, block, name: str, value, bitfield_def, schema):
        """Draw bitfield members as individual sub-editors."""
        int_val = int(value) if isinstance(value, (int, float)) else 0

        label = f"{name}: 0x{int_val:X}"
        if imgui.tree_node(label):
            new_val = int_val
            for member in bitfield_def.members:
                extracted = (int_val & member.mask) >> member.pos

                # If the member type is an enum, render as combo
                if member.type in schema.enums:
                    enum_def = schema.enums[member.type]
                    option_names = [opt.name for opt in enum_def.options]
                    option_values = [opt.value for opt in enum_def.options]
                    current_idx = -1
                    for i, ov in enumerate(option_values):
                        if ov == extracted:
                            current_idx = i
                            break

                    changed, new_idx = imgui.combo(
                        f"{member.name}##{name}_{member.name}",
                        current_idx,
                        option_names,
                    )
                    if changed and 0 <= new_idx < len(option_values):
                        new_val = (new_val & ~member.mask) | (
                            option_values[new_idx] << member.pos
                        )
                # If the member type is bitflags, render as checkboxes
                elif member.type in schema.bitflags:
                    bf_def = schema.bitflags[member.type]
                    if imgui.tree_node(
                        f"{member.name}: 0x{extracted:X}##{name}_{member.name}"
                    ):
                        new_member_val = extracted
                        for opt in bf_def.options:
                            bit_mask = 1 << opt.value
                            is_set = bool(extracted & bit_mask)
                            chg, checked = imgui.checkbox(
                                f"{opt.name}##{name}_{member.name}_{opt.name}",
                                is_set,
                            )
                            if chg:
                                if checked:
                                    new_member_val |= bit_mask
                                else:
                                    new_member_val &= ~bit_mask
                        if new_member_val != extracted:
                            new_val = (new_val & ~member.mask) | (
                                new_member_val << member.pos
                            )
                        imgui.tree_pop()
                else:
                    max_member_val = (1 << member.width) - 1
                    changed, new_member_val = imgui.input_int(
                        f"{member.name}##{name}_{member.name}",
                        extracted,
                    )
                    if changed:
                        clamped = max(0, min(max_member_val, new_member_val))
                        new_val = (new_val & ~member.mask) | (clamped << member.pos)

            if new_val != int_val:
                self._set_field(block, name, value, new_val)

            imgui.tree_pop()

    # -------------------------------------------------------------------
    # Value widgets (vectors, colors, matrices, refs, structs)
    # -------------------------------------------------------------------

    def _draw_vector3(self, block, name: str, value: dict):
        """Draw XYZ vector input."""
        x = float(value.get("x", 0))
        y = float(value.get("y", 0))
        z = float(value.get("z", 0))
        changed, vals = imgui.slider_float3(name, [x, y, z], -1000.0, 1000.0, "%.3f")
        if changed:
            nx, ny, nz = vals
            new_value = {"x": nx, "y": ny, "z": nz}
            if "w" in value:
                new_value["w"] = value["w"]
            self._set_field_drag(block, name, value, new_value)
        self._check_drag_release(
            block,
            name,
            {
                "x": vals[0],
                "y": vals[1],
                "z": vals[2],
                **({} if "w" not in value else {"w": value["w"]}),
            }
            if changed
            else value,
        )

    def _draw_color(self, block, name: str, value: dict):
        """Draw color picker."""
        r = float(value.get("r", 1))
        g = float(value.get("g", 1))
        b = float(value.get("b", 1))
        a = float(value.get("a", 1)) if "a" in value else None

        if a is not None:
            col = imgui.ImVec4(r, g, b, a)
            changed, col = imgui.color_edit4(name, col)
            # col may be ImVec4 or list depending on imgui binding
            cr, cg, cb, ca = (
                (col[0], col[1], col[2], col[3])
                if isinstance(col, (list, tuple))
                else (col.x, col.y, col.z, col.w)
            )
            if changed:
                new_value = {"r": cr, "g": cg, "b": cb, "a": ca}
                self._set_field_drag(block, name, value, new_value)
            self._check_drag_release(
                block, name, {"r": cr, "g": cg, "b": cb, "a": ca} if changed else value
            )
        else:
            col = imgui.ImVec4(r, g, b, 1.0)
            changed, col = imgui.color_edit3(name, col)
            cr, cg, cb = (
                (col[0], col[1], col[2])
                if isinstance(col, (list, tuple))
                else (col.x, col.y, col.z)
            )
            if changed:
                new_value = {"r": cr, "g": cg, "b": cb}
                self._set_field_drag(block, name, value, new_value)
            self._check_drag_release(
                block, name, {"r": cr, "g": cg, "b": cb} if changed else value
            )

    def _draw_matrix33(self, block, name: str, value: dict):
        """Draw a 3x3 rotation matrix — editable as Euler angles or raw matrix."""
        if imgui.tree_node(f"{name} (Matrix33)"):
            # Extract matrix values — nif.xml Matrix33 names use m[col][row],
            # so transpose to get standard [row][col] for display and math.
            m = [
                [
                    float(value.get("m11", 1)),
                    float(value.get("m21", 0)),
                    float(value.get("m31", 0)),
                ],
                [
                    float(value.get("m12", 0)),
                    float(value.get("m22", 1)),
                    float(value.get("m32", 0)),
                ],
                [
                    float(value.get("m13", 0)),
                    float(value.get("m23", 0)),
                    float(value.get("m33", 1)),
                ],
            ]

            # Show as Euler angles (editable)
            pitch, yaw, roll = _matrix33_to_euler(m)
            imgui.text("Euler (degrees):")
            changed_p, new_pitch = imgui.slider_float(
                f"Pitch##{name}", pitch, -180.0, 180.0, "%.1f"
            )
            released_p = imgui.is_item_deactivated_after_edit()
            changed_y, new_yaw = imgui.slider_float(
                f"Yaw##{name}", yaw, -180.0, 180.0, "%.1f"
            )
            released_y = imgui.is_item_deactivated_after_edit()
            changed_r, new_roll = imgui.slider_float(
                f"Roll##{name}", roll, -180.0, 180.0, "%.1f"
            )
            released_r = imgui.is_item_deactivated_after_edit()

            if changed_p or changed_y or changed_r:
                new_m = _euler_to_matrix33(new_pitch, new_yaw, new_roll)
                new_value = {
                    "m11": new_m[0][0],
                    "m21": new_m[0][1],
                    "m31": new_m[0][2],
                    "m12": new_m[1][0],
                    "m22": new_m[1][1],
                    "m32": new_m[1][2],
                    "m13": new_m[2][0],
                    "m23": new_m[2][1],
                    "m33": new_m[2][2],
                }
                self._set_field_drag(block, name, value, new_value)
            if released_p or released_y or released_r:
                self._finalize_drag(block, name)

            # Also show raw matrix (editable per-row)
            if imgui.tree_node(f"Raw Matrix##{name}"):
                changed1, r1 = imgui.slider_float3(
                    f"Row 1##{name}", m[0], -1.0, 1.0, "%.4f"
                )
                released1 = imgui.is_item_deactivated_after_edit()
                changed2, r2 = imgui.slider_float3(
                    f"Row 2##{name}", m[1], -1.0, 1.0, "%.4f"
                )
                released2 = imgui.is_item_deactivated_after_edit()
                changed3, r3 = imgui.slider_float3(
                    f"Row 3##{name}", m[2], -1.0, 1.0, "%.4f"
                )
                released3 = imgui.is_item_deactivated_after_edit()
                if changed1 or changed2 or changed3:
                    n11, n12, n13 = r1
                    n21, n22, n23 = r2
                    n31, n32, n33 = r3
                    new_value = {
                        "m11": n11,
                        "m21": n12,
                        "m31": n13,
                        "m12": n21,
                        "m22": n22,
                        "m32": n23,
                        "m13": n31,
                        "m23": n32,
                        "m33": n33,
                    }
                    self._set_field_drag(block, name, value, new_value)
                if released1 or released2 or released3:
                    self._finalize_drag(block, name)
                imgui.tree_pop()

            imgui.tree_pop()

    def _draw_struct(self, block, name: str, value: dict, fdef, schema):
        """Draw a generic struct as expandable tree."""
        if imgui.tree_node(f"{name} ({len(value)} fields)"):
            for k, v in value.items():
                self._draw_field(block, f"{name}.{k}", v, None, schema)
            imgui.tree_pop()

    def _draw_vertex_table(self, block, name: str, value: list):
        """Draw vertex data as a columnar table with virtual scrolling."""
        # Detect which columns are present from the first vertex
        sample = value[0]
        col_defs = []
        for key in (
            "Vertex",
            "Normal",
            "UV",
            "Vertex Colors",
            "Tangent",
            "Bitangent X",
            "Bitangent Y",
            "Bitangent Z",
            "Bone Weights",
            "Bone Indices",
        ):
            if key in sample:
                col_defs.append(key)

        if imgui.tree_node(f"{name} [{len(value)} vertices]"):
            num_cols = len(col_defs) + 1  # +1 for index column
            flags = (
                imgui.TableFlags_.borders.value
                | imgui.TableFlags_.scroll_y.value
                | imgui.TableFlags_.resizable.value
                | imgui.TableFlags_.row_bg.value
            )
            opened = imgui.begin_table(
                f"vtx_{name}", num_cols, flags, imgui.ImVec2(0.0, 300.0)
            )
            if opened:
                # Header
                imgui.table_setup_column(
                    "#", imgui.TableColumnFlags_.width_fixed.value, 40.0
                )
                for col_name in col_defs:
                    imgui.table_setup_column(col_name)
                imgui.table_setup_scroll_freeze(1, 1)
                imgui.table_headers_row()

                # Rows — render all; imgui table scroll handles clipping
                for i, vtx in enumerate(value):
                    imgui.table_next_row()
                    imgui.table_next_column()
                    imgui.text(str(i))
                    for col_name in col_defs:
                        imgui.table_next_column()
                        v = vtx.get(col_name)
                        if v is None:
                            imgui.text_colored(imgui.ImVec4(0.4, 0.4, 0.4, 1.0), "-")
                        elif isinstance(v, dict):
                            parts = [f"{fv:.3f}" for fv in v.values()]
                            imgui.text(" ".join(parts))
                        elif isinstance(v, list):
                            parts = [
                                f"{item:.2f}" if isinstance(item, float) else str(item)
                                for item in v
                            ]
                            imgui.text(" ".join(parts))
                        else:
                            imgui.text(str(v))
                imgui.end_table()
            imgui.tree_pop()

    def _draw_quaternion(self, block, name: str, value: dict):
        """Draw a quaternion with both WXYZ sliders and Euler angle view."""
        w = float(value.get("w", 1))
        x = float(value.get("x", 0))
        y = float(value.get("y", 0))
        z = float(value.get("z", 0))

        if imgui.tree_node(f"{name} (Quaternion)"):
            # WXYZ drag sliders
            changed_w, nw = imgui.slider_float(f"W##{name}", w, -1.0, 1.0, "%.4f")
            rel_w = imgui.is_item_deactivated_after_edit()
            changed_x, nx = imgui.slider_float(f"X##{name}", x, -1.0, 1.0, "%.4f")
            rel_x = imgui.is_item_deactivated_after_edit()
            changed_y, ny = imgui.slider_float(f"Y##{name}", y, -1.0, 1.0, "%.4f")
            rel_y = imgui.is_item_deactivated_after_edit()
            changed_z, nz = imgui.slider_float(f"Z##{name}", z, -1.0, 1.0, "%.4f")
            rel_z = imgui.is_item_deactivated_after_edit()

            if changed_w or changed_x or changed_y or changed_z:
                self._set_field_drag(
                    block, name, value, {"w": nw, "x": nx, "y": ny, "z": nz}
                )
            if rel_w or rel_x or rel_y or rel_z:
                self._finalize_drag(block, name)

            # Show Euler angles (read-only, derived from quaternion)
            pitch, yaw, roll = _quat_to_euler(w, x, y, z)
            imgui.text_colored(
                imgui.ImVec4(0.5, 0.7, 0.5, 1.0),
                f"Euler: P={pitch:.1f} Y={yaw:.1f} R={roll:.1f}",
            )

            # Normalize button
            length = math.sqrt(w * w + x * x + y * y + z * z)
            if length > 0 and abs(length - 1.0) > 0.001:
                imgui.same_line()
                if imgui.small_button(f"Normalize##{name}"):
                    self._set_field(
                        block,
                        name,
                        value,
                        {
                            "w": w / length,
                            "x": x / length,
                            "y": y / length,
                            "z": z / length,
                        },
                    )

            imgui.tree_pop()

    def _draw_string_index(self, block, name: str, value: int):
        """Draw a string index as a dropdown of header string table entries."""
        nif = self.app.nif_file
        if not nif or not hasattr(nif, "header") or not nif.header.strings:
            # Fall back to plain int
            changed, new_val = imgui.input_int(name, value)
            if changed:
                self._set_field(block, name, value, new_val)
            return

        strings = nif.header.strings
        # Build display list
        options = [f"[{i}] {s}" for i, s in enumerate(strings)]

        current_idx = value if 0 <= value < len(strings) else -1
        preview = options[current_idx] if current_idx >= 0 else f"<invalid: {value}>"

        if imgui.begin_combo(f"{name}##str_idx", preview):
            for i, label in enumerate(options):
                selected = i == current_idx
                clicked, _ = imgui.selectable(label, selected)
                if clicked and i != current_idx:
                    self._set_field(block, name, value, i)
                if selected:
                    imgui.set_item_default_focus()
            imgui.end_combo()

    def _draw_ref(self, block, name: str, value):
        """Draw a block reference with Go and Clear buttons."""
        ref_id = int(value) if isinstance(value, (int, float)) else -1

        if ref_id < 0:
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), f"{name}: None")
            # No Go/Clear buttons for null refs. Don't leave a dangling
            # `same_line()` here — that would glue the next field's widget
            # onto this row (e.g. Shader Flags 1 ending up beside a null
            # Controller on BSLightingShaderProperty).
            return

        nif = self.app.nif_file
        target = nif.get_block(ref_id) if nif else None
        if target:
            label = f"{name}: -> [{ref_id}] {target.type_name}"
            imgui.push_style_color(
                imgui.Col_.text.value, imgui.ImVec4(0.4, 0.7, 1.0, 1.0)
            )
            try:
                clicked, _ = imgui.selectable(label, False)
                if clicked:
                    if hasattr(self.app, "selection_mgr"):
                        self.app.selection_mgr.select_by_block_id(ref_id)
            finally:
                imgui.pop_style_color()
        else:
            imgui.text(f"{name}: -> [{ref_id}] (missing)")

        # Go + Clear buttons
        imgui.same_line()
        if imgui.small_button(f"Go##{name}"):
            if hasattr(self.app, "selection_mgr"):
                self.app.selection_mgr.select_by_block_id(ref_id)
        imgui.same_line()
        if imgui.small_button(f"Clear##{name}"):
            self._set_field(block, name, value, -1)

    # -------------------------------------------------------------------
    # Transform group
    # -------------------------------------------------------------------

    def _draw_transform_group(self, block):
        """Group Translation + Rotation + Scale into a collapsible Transform section."""
        if imgui.tree_node_ex("Transform", imgui.TreeNodeFlags_.default_open.value):
            trans = block.get_field("Translation") or {}
            if isinstance(trans, dict):
                self._draw_vector3(block, "Translation", trans)

            rot = block.get_field("Rotation") or {}
            if isinstance(rot, dict) and _is_matrix33(rot):
                self._draw_matrix33(block, "Rotation", rot)

            scale = block.get_field("Scale")
            if scale is not None:
                changed, new_val = imgui.slider_float(
                    "Scale", float(scale), 0.0, 10.0, "%.4f"
                )
                if changed:
                    self._set_field_drag(block, "Scale", scale, new_val)
                self._check_drag_release(block, "Scale", new_val if changed else scale)

            # Reset Transform button
            if imgui.small_button("Reset Transform"):
                from creation_lib.nif.actions import SetFieldAction, CompositeAction

                nif = self.app.nif_file
                cmds = []
                old_trans = block.get_field("Translation")
                old_rot = block.get_field("Rotation")
                old_scale = block.get_field("Scale")
                identity_trans = {"x": 0.0, "y": 0.0, "z": 0.0}
                identity_rot = {
                    "m11": 1.0,
                    "m12": 0.0,
                    "m13": 0.0,
                    "m21": 0.0,
                    "m22": 1.0,
                    "m23": 0.0,
                    "m31": 0.0,
                    "m32": 0.0,
                    "m33": 1.0,
                }
                cmds.append(
                    SetFieldAction(
                        block_id=block.block_id,
                        field_name="Translation",
                        old_value=old_trans,
                        new_value=identity_trans,
                    )
                )
                cmds.append(
                    SetFieldAction(
                        block_id=block.block_id,
                        field_name="Rotation",
                        old_value=old_rot,
                        new_value=identity_rot,
                    )
                )
                cmds.append(
                    SetFieldAction(
                        block_id=block.block_id,
                        field_name="Scale",
                        old_value=old_scale,
                        new_value=1.0,
                    )
                )
                comp = CompositeAction(children=cmds, _description="Reset Transform")
                comp.execute(nif)
                self.app.undo_manager.push(self.app.registry.active_id, comp)
                self._mark_dirty()

            imgui.tree_pop()

    # -------------------------------------------------------------------
    # Path fields (texture paths, behavior paths, etc.)
    # -------------------------------------------------------------------

    def _draw_path_field(self, block, name: str, value: str):
        """Draw a file path field with a browse button."""
        changed, new_val = imgui.input_text(f"{name}##path", value)
        if changed:
            self._set_field(block, name, value, new_val)

        imgui.same_line()
        if imgui.small_button(f"...##{name}"):
            self._browse_file(block, name, value)

    def _browse_file(self, block, name: str, current_value: str):
        """Open a file dialog for path fields."""
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file

            # Determine file filter from context
            lower_name = name.lower()
            if "texture" in lower_name or lower_name.startswith("texture"):
                filetypes = [("DDS Textures", "*.dds"), ("All files", "*.*")]
            elif "behavior" in lower_name or "bsgraph" in lower_name:
                filetypes = [("HKX files", "*.hkx"), ("All files", "*.*")]
            elif "material" in lower_name or "bgsm" in lower_name:
                filetypes = [("Material files", "*.bgsm *.bgem"), ("All files", "*.*")]
            else:
                filetypes = [("All files", "*.*")]

            filepath = pick_file(f"Browse {name}", filetypes)

            if filepath:
                from ui.shared.path_utils import to_game_relative_path

                file_type = (
                    "material"
                    if ("material" in lower_name or "bgsm" in lower_name)
                    else "texture"
                )
                filepath = to_game_relative_path(filepath, file_type)
                self._set_field(block, name, current_value, filepath)

        except Exception:
            pass

    # -------------------------------------------------------------------
    # Undo-aware field setter
    # -------------------------------------------------------------------

    def _set_field(self, block, name: str, old_value, new_value):
        """Set a field with undo support (immediate — use for discrete edits)."""
        if old_value == new_value:
            return

        from creation_lib.nif.actions import SetFieldAction

        nif = self.app.nif_file
        cmd = SetFieldAction(
            block_id=block.block_id,
            field_name=name,
            old_value=old_value,
            new_value=new_value,
        )
        cmd.execute(nif)
        self.app.undo_manager.push(self.app.registry.active_id, cmd)
        self._mark_dirty()
        self.app.rebuild_scene_from_nif()

    def _set_field_drag(self, block, name: str, old_value, new_value):
        """Set a field during a drag/slider interaction — apply live but defer undo.

        Call this when imgui reports `changed` on a drag/slider widget.
        After the widget, call _check_drag_release() to push undo on release.
        """
        import copy as _copy

        # First frame of drag: capture the original value
        drag_key = f"{block.block_id}:{name}"
        if self._drag_field != drag_key:
            self._drag_field = drag_key
            self._drag_old_value = _copy.deepcopy(old_value)

        # Apply to NIF immediately (live preview) but don't push undo
        nif = self.app.nif_file
        block.set_field(name, new_value)
        self._mark_dirty()

    def _check_drag_release(self, block, name: str, current_value):
        """After a drag/slider widget, check if the user released it and push undo."""
        if not imgui.is_item_deactivated_after_edit():
            return
        self._finalize_drag(block, name, current_value)

    def _finalize_drag(self, block, name: str, current_value=None):
        """Push a single undo action for a completed drag. Called on widget release."""
        drag_key = f"{block.block_id}:{name}"
        if self._drag_field != drag_key or self._drag_old_value is None:
            return

        # Read the final value from the NIF block, not the widget's last frame.
        # On drag release, imgui can report a stale `current_value` even though the
        # block has already been updated by _set_field_drag().
        current_value = block.get_field(name)

        from creation_lib.nif.actions import SetFieldAction

        cmd = SetFieldAction(
            block_id=block.block_id,
            field_name=name,
            old_value=self._drag_old_value,
            new_value=current_value,
        )
        # Value is already applied — just register for undo
        self.app.undo_manager.push(self.app.registry.active_id, cmd)
        self._drag_field = None
        self._drag_old_value = None

        # Rebuild scene so material/shader changes are reflected in the viewport
        self.app.rebuild_scene_from_nif()

        # Re-apply the live palette preview after the scene graph is rebuilt.
        # rebuild_scene_from_nif() reconstructs mesh/material objects, so any
        # direct node.mesh.material.palette_scale tweak is lost otherwise.
        if name == "Grayscale to Palette Scale":
            self._push_palette_scale(block, float(current_value))

    def _push_palette_scale(self, shader_block, new_scale: float):
        """Push a palette scale change to GPU materials for live viewport preview.

        Walks the scene tree and updates any mesh whose shader property
        matches this block.
        """
        root = getattr(self.app, "nif_root", None)
        nif = self.app.nif_file
        if not root or not nif:
            return
        shader_bid = shader_block.block_id
        self._update_palette_scale_recursive(root, nif, shader_bid, new_scale)

    def _update_palette_scale_recursive(self, node, nif, shader_bid, scale):
        """Recursively find meshes referencing a shader property and update palette_scale."""
        if node.mesh:
            # Check if this node's BSTriShape references the given shader property
            block = nif.get_block(node.block_id)
            if block:
                from creation_lib.renderer.material_pipeline import _get_ref_id

                ref = block.get_field("Shader Property")
                if ref is not None and _get_ref_id(ref) == shader_bid:
                    node.mesh.material.palette_scale = scale
        for child in node.children:
            self._update_palette_scale_recursive(child, nif, shader_bid, scale)

    def _mark_dirty(self):
        """Mark the NIF as modified (for save tracking)."""
        if hasattr(self.app, "_nif_dirty"):
            self.app._nif_dirty = True


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _is_quaternion(d: dict) -> bool:
    """Detect quaternion: must have exactly w, x, y, z keys."""
    return set(d.keys()) == {"w", "x", "y", "z"}


def _is_vector3(d: dict) -> bool:
    keys = set(d.keys())
    return keys == {"x", "y", "z"}


def _is_color(d) -> bool:
    if hasattr(d, "r") and hasattr(d, "g") and hasattr(d, "b"):
        return True
    if isinstance(d, dict):
        keys = set(d.keys())
        return keys == {"r", "g", "b"} or keys == {"r", "g", "b", "a"}
    return False


def _is_matrix33(d: dict) -> bool:
    return "m11" in d and "m33" in d


def _looks_like_ref(name: str) -> bool:
    ref_names = {
        "Shader Property",
        "Alpha Property",
        "Collision Object",
        "Skin",
        "Skin Instance",
        "Texture Set",
        "Controller",
        "Interpolator",
        "Data",
        "Extra Data",
        "Target",
        "Root Node",
        "Manager",
    }
    return name in ref_names or "Property" in name or "Ref" in name


def _ensure_color3(value) -> dict:
    """Ensure a value is a valid Color3 dict, converting from dataclass/int/other."""
    if isinstance(value, dict) and "r" in value:
        return value
    if hasattr(value, "r") and hasattr(value, "g") and hasattr(value, "b"):
        return {"r": float(value.r), "g": float(value.g), "b": float(value.b)}
    return {"r": 0.0, "g": 0.0, "b": 0.0}


def _ensure_color4(value) -> dict:
    """Ensure a value is a valid Color4 dict, converting from dataclass/int/other."""
    if isinstance(value, dict) and "r" in value:
        return value
    if hasattr(value, "r") and hasattr(value, "g") and hasattr(value, "b"):
        a = float(value.a) if hasattr(value, "a") else 1.0
        return {"r": float(value.r), "g": float(value.g), "b": float(value.b), "a": a}
    return {"r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0}


def _is_path_field(block, name: str) -> bool:
    """Detect fields that contain file paths."""
    # Known path-containing block+field combos
    type_name = block.type_name
    lower_name = name.lower()

    if type_name == "BSShaderTextureSet" and lower_name.startswith("texture"):
        return True
    if (
        type_name in ("BSLightingShaderProperty", "BSEffectShaderProperty")
        and lower_name == "name"
    ):
        return True
    if type_name == "BSLightingShaderProperty" and name == "RootMaterial":
        return True
    if type_name == "BSBehaviorGraphExtraData" and lower_name in (
        "name",
        "behaviour graph file",
    ):
        return True
    if "file" in lower_name or "path" in lower_name:
        return True
    return False


def _matrix33_to_euler(m: list[list[float]]) -> tuple[float, float, float]:
    """Convert 3x3 rotation matrix to Euler angles (pitch, yaw, roll) in degrees.

    Uses ZYX convention (yaw-pitch-roll).
    """
    # Clamp to avoid domain errors from floating point
    sy = max(-1.0, min(1.0, -m[0][2]))
    pitch = math.asin(sy)

    if abs(sy) < 0.9999:
        yaw = math.atan2(m[0][1], m[0][0])
        roll = math.atan2(m[1][2], m[2][2])
    else:
        yaw = math.atan2(-m[1][0], m[1][1])
        roll = 0.0

    return math.degrees(pitch), math.degrees(yaw), math.degrees(roll)


def _euler_to_matrix33(
    pitch_deg: float, yaw_deg: float, roll_deg: float
) -> list[list[float]]:
    """Convert Euler angles (degrees) to a 3x3 rotation matrix (ZYX convention)."""
    p = math.radians(pitch_deg)
    y = math.radians(yaw_deg)
    r = math.radians(roll_deg)

    cp, sp = math.cos(p), math.sin(p)
    cy, sy = math.cos(y), math.sin(y)
    cr, sr = math.cos(r), math.sin(r)

    return [
        [cy * cp, sy * cp, -sp],
        [cy * sp * sr - sy * cr, sy * sp * sr + cy * cr, cp * sr],
        [cy * sp * cr + sy * sr, sy * sp * cr - cy * sr, cp * cr],
    ]


def _quat_to_euler(
    w: float, x: float, y: float, z: float
) -> tuple[float, float, float]:
    """Convert quaternion to Euler angles (pitch, yaw, roll) in degrees."""
    # Roll (X)
    sinr_cosp = 2.0 * (w * x + y * z)
    cosr_cosp = 1.0 - 2.0 * (x * x + y * y)
    roll = math.atan2(sinr_cosp, cosr_cosp)

    # Pitch (Y)
    sinp = 2.0 * (w * y - z * x)
    sinp = max(-1.0, min(1.0, sinp))
    pitch = math.asin(sinp)

    # Yaw (Z)
    siny_cosp = 2.0 * (w * z + x * y)
    cosy_cosp = 1.0 - 2.0 * (y * y + z * z)
    yaw = math.atan2(siny_cosp, cosy_cosp)

    return math.degrees(pitch), math.degrees(yaw), math.degrees(roll)
