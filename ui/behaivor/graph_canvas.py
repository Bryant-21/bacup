"""imgui_node_editor rendering layer for the behavior graph editor.

Renders the node graph using imgui_bundle's imgui_node_editor bindings.
Reads from a GraphModel instance and renders all nodes, pins, and links.
Handles interactive link creation, deletion, and node selection.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from collections import deque

from imgui_bundle import imgui, imgui_node_editor as ne

from .node_types import NODE_TYPE_DEFINITIONS

if TYPE_CHECKING:
    from .graph_model import GraphModel

log = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Color themes — 18 total (must match property_editor.py's copy)
# ---------------------------------------------------------------------------

COLOR_THEMES: list[tuple[int, int, int]] = [
    (80, 80, 80),       # 0  - Default grey
    (100, 140, 200),    # 1  - Blue
    (60, 80, 140),      # 2  - Deep Blue
    (180, 80, 80),      # 3  - Red
    (140, 80, 160),     # 4  - Purple
    (100, 60, 100),     # 5  - Eggplant
    (60, 160, 100),     # 6  - Emerald
    (80, 140, 80),      # 7  - Green
    (140, 100, 60),     # 8  - Brown
    (200, 140, 60),     # 9  - Orange
    (200, 200, 80),     # 10 - Yellow
    (80, 160, 180),     # 11 - Cerulean
    (200, 100, 120),    # 12 - Rose
    (160, 120, 180),    # 13 - Lavender
    (100, 180, 160),    # 14 - Teal
    (180, 160, 100),    # 15 - Sand
    (120, 100, 80),     # 16 - Mocha
    (60, 120, 100),     # 17 - Forest
]

COLOR_THEME_NAMES: list[str] = [
    "Default", "Blue", "Deep Blue", "Red", "Purple", "Eggplant",
    "Emerald", "Green", "Brown", "Orange", "Yellow", "Cerulean",
    "Rose", "Lavender", "Teal", "Sand", "Mocha", "Forest",
]

# ---------------------------------------------------------------------------
# Pin ID encoding helpers
# ---------------------------------------------------------------------------
# Output pin: node_id * 100 + port_index         (port_index in 0..49)
# Input pin:  node_id * 100 + 50 + port_index     (port_index in 0..49)
#
# This gives each node up to 50 output ports and 50 input ports, which is
# more than enough for any Havok behavior node.

_OUTPUT_PIN_OFFSET = 0
_INPUT_PIN_OFFSET = 50


def _encode_output_pin(node_id: int, port_index: int) -> int:
    return node_id * 100 + _OUTPUT_PIN_OFFSET + port_index


def _encode_input_pin(node_id: int, port_index: int) -> int:
    return node_id * 100 + _INPUT_PIN_OFFSET + port_index


def _decode_pin(pin_raw: int) -> tuple[int, int, bool]:
    """Decode a raw pin ID -> (node_id, port_index, is_input)."""
    node_id = pin_raw // 100
    remainder = pin_raw % 100
    if remainder >= _INPUT_PIN_OFFSET:
        return node_id, remainder - _INPUT_PIN_OFFSET, True
    return node_id, remainder, False


def _theme_color(color_id: int, alpha: float = 1.0) -> imgui.ImVec4:
    """Convert a COLOR_THEMES index to an ImVec4 (0-1 range)."""
    idx = color_id % len(COLOR_THEMES)
    r, g, b = COLOR_THEMES[idx]
    return imgui.ImVec4(r / 255.0, g / 255.0, b / 255.0, alpha)


def _theme_color_u32(color_id: int, alpha: float = 1.0) -> int:
    """Convert a COLOR_THEMES index to a packed U32 color."""
    return imgui.get_color_u32(_theme_color(color_id, alpha))


# ---------------------------------------------------------------------------
# Node header height constant (approximate, for background rect drawing)
# ---------------------------------------------------------------------------
_HEADER_PAD_Y = 4.0


# ---------------------------------------------------------------------------
# GraphCanvas
# ---------------------------------------------------------------------------

def _estimate_node_height(node_data: dict) -> float:
    """Estimate a node's rendered height based on its port count."""
    type_id = node_data.get("nodeTypeID", -1)
    defn = NODE_TYPE_DEFINITIONS.get(type_id, {})
    n_inputs = len(defn.get("input_ports", []))
    n_outputs = len(defn.get("output_ports", []))
    pin_rows = max(n_inputs, n_outputs, 1)
    has_subtitle = bool(node_data.get("nodeName", ""))
    header_h = 50.0 if not has_subtitle else 70.0
    # ~28px per pin row (icon 12 + text padding + spacing)
    return pin_rows * 28.0 + header_h


def _compute_tree_layout(
    nodes: dict[int, dict],
    connections: list,
    x_spacing: float = 500.0,
    y_gap: float = 60.0,
) -> dict[int, tuple[float, float]]:
    """Compute a hierarchical tree layout for the node graph.

    Uses BFS from root nodes, assigning layers (columns) by depth.
    Vertical spacing adapts to actual node height estimates.
    Returns a dict mapping node_id -> (x, y) position.
    """
    if not nodes:
        return {}

    # Build adjacency: parent -> children
    children_of: dict[int, list[int]] = {}
    has_parent: set[int] = set()
    for conn in connections:
        port_idx, from_id, to_id = conn[0], conn[1], conn[2]
        children_of.setdefault(from_id, []).append(to_id)
        has_parent.add(to_id)

    # Find roots (nodes with no incoming connections)
    all_ids = set(nodes.keys())
    roots = [nid for nid in sorted(all_ids) if nid not in has_parent]
    if not roots:
        roots = [min(all_ids)]

    # BFS to assign layers (depth)
    layer_of: dict[int, int] = {}
    parent_of: dict[int, int] = {}
    queue: deque[int] = deque()
    for r in roots:
        layer_of[r] = 0
        queue.append(r)

    while queue:
        nid = queue.popleft()
        depth = layer_of[nid]
        for child in children_of.get(nid, []):
            if child not in layer_of:
                layer_of[child] = depth + 1
                parent_of[child] = nid
                queue.append(child)

    # Any disconnected nodes go to layer 0
    for nid in all_ids:
        if nid not in layer_of:
            layer_of[nid] = 0

    # Group nodes by layer
    layers: dict[int, list[int]] = {}
    for nid, layer in layer_of.items():
        layers.setdefault(layer, []).append(nid)

    # Sort nodes within each layer by parent position to reduce edge crossings.
    # First pass: position layers left-to-right so parent order is available.
    positions: dict[int, tuple[float, float]] = {}
    for layer_idx in sorted(layers.keys()):
        layer_nodes = layers[layer_idx]

        # Sort by parent's Y position (if available), then by node ID
        if layer_idx > 0:
            def _sort_key(nid: int) -> tuple[float, int]:
                p = parent_of.get(nid)
                py = positions[p][1] if p is not None and p in positions else 0.0
                return (py, nid)
            layer_nodes.sort(key=_sort_key)
        else:
            layer_nodes.sort()

        # Stack nodes vertically with height-aware spacing
        y = 0.0
        for i, nid in enumerate(layer_nodes):
            positions[nid] = (layer_idx * x_spacing, y)
            node_h = _estimate_node_height(nodes[nid])
            y += node_h + y_gap

        # Center the layer around y=0
        if layer_nodes:
            total_h = y - y_gap
            offset = total_h / 2.0
            for nid in layer_nodes:
                x, old_y = positions[nid]
                positions[nid] = (x, old_y - offset)

    return positions


class GraphCanvas:
    """Renders the behavior graph using imgui_node_editor.

    Reads from a ``GraphModel`` each frame and draws nodes, pins, and links.
    Also handles interactive link creation / deletion and selection tracking.
    """

    def __init__(self) -> None:
        self._editor_context: ne.EditorContext | None = None
        self._selected_node_id: int | None = None
        self._pending_create_type: int | None = None
        self._pending_drop_pos: imgui.ImVec2 | None = None
        # Maps link sequential ID -> (port_idx, from_id, to_id) each frame
        self._link_id_map: dict[int, tuple[int, int, int]] = {}
        self._frame_count: int = 0
        self._needs_layout: bool = False
        self._current_model_connections: list = []

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def selected_node_id(self) -> int | None:
        """Currently selected node ID, or None."""
        return self._selected_node_id

    def request_create_node(self, type_id: int) -> None:
        """Request that a node of *type_id* be created on the next frame.

        Called externally (e.g. from a palette or context menu).
        """
        self._pending_create_type = type_id

    def request_layout(self) -> None:
        """Request that the graph be auto-laid out on the next frame.

        Keeps the existing editor context so node positions can be set
        immediately without a create/destroy cycle and stale view state.
        """
        self._needs_layout = True

    def navigate_to_content(self) -> None:
        """Zoom / pan so all nodes are visible."""
        if self._editor_context is not None:
            ne.navigate_to_content(0.0)

    def destroy(self) -> None:
        """Release the editor context.  Call when the window closes."""
        if self._editor_context is not None:
            ne.destroy_editor(self._editor_context)
            self._editor_context = None

    # ------------------------------------------------------------------
    # Main render entry-point
    # ------------------------------------------------------------------

    def render(self, model: GraphModel) -> None:
        """Render the full node graph.  Call once per frame inside the
        canvas dock/window region.
        """
        # Lazy-create editor context
        if self._editor_context is None:
            # Delete corrupted state file (near-zero zoom causes blank canvas + high CPU)
            import os
            _state_file = os.path.join(os.path.dirname(__file__), "..", "NodeEditor.json")
            if os.path.isfile(_state_file):
                try:
                    import json
                    with open(_state_file) as f:
                        state = json.load(f)
                    zoom = state.get("view", {}).get("zoom", 1.0)
                    if zoom < 0.01 or zoom > 100.0:
                        log.warning("Removing corrupted NodeEditor.json (zoom=%.2e)", zoom)
                        os.remove(_state_file)
                except Exception:
                    pass

            config = ne.Config()
            config.navigate_button_index = 2   # middle-mouse to pan
            config.context_menu_button_index = 1  # right-click for context menu
            self._editor_context = ne.create_editor(config)
            self._frame_count = 0

        ne.set_current_editor(self._editor_context)
        ne.begin("BehaviorGraph")

        # Auto-layout on first frame or when requested
        if self._frame_count == 0 or self._needs_layout:
            positions = _compute_tree_layout(model.nodes, model.connections)
            for node_id, (x, y) in positions.items():
                ne.set_node_position(ne.NodeId(node_id), imgui.ImVec2(x, y))
            if self._needs_layout:
                self._frame_count = 0  # reset so navigate_to_content re-fires
            self._needs_layout = False

        # --- Nodes ---
        self._current_model_connections = model.connections
        for node_id, node_data in model.nodes.items():
            self._render_node(node_id, node_data)

        # --- Links ---
        self._render_links(model)

        # --- Interactive creation ---
        self._handle_create(model)

        # --- Interactive deletion ---
        self._handle_delete(model)

        # --- Pending palette creation ---
        self._handle_pending_create(model)

        # --- Selection tracking ---
        self._update_selection()

        # --- Context menus (right-click on link, node, or background) ---
        self._handle_context_menus(model)

        # Navigate to fit after a few frames (let layout settle)
        # Repeat over frames 2-5 so node sizes are fully resolved
        if 2 <= self._frame_count <= 5:
            ne.navigate_to_content(0.0)

        self._frame_count += 1

        ne.end()

        # Accept drag-drop from the node palette
        if imgui.begin_drag_drop_target():
            payload = imgui.accept_drag_drop_payload_py_id("NODE_TYPE")
            if payload is not None:
                self._pending_create_type = payload.data_id
                self._pending_drop_pos = imgui.get_mouse_pos()
            imgui.end_drag_drop_target()

    # ------------------------------------------------------------------
    # Node rendering
    # ------------------------------------------------------------------

    _PIN_ICON_SIZE = 12.0  # px, diameter of pin icon

    @staticmethod
    def _draw_pin_icon(
        is_input: bool,
        connected: bool,
        color_u32: int,
    ) -> None:
        """Draw a pin icon (triangle for flow, circle for data) using ImDrawList."""
        size = GraphCanvas._PIN_ICON_SIZE
        pos = imgui.get_cursor_screen_pos()
        draw_list = imgui.get_window_draw_list()
        center = imgui.ImVec2(pos.x + size * 0.5, pos.y + size * 0.5)
        bg_col = imgui.get_color_u32(imgui.ImVec4(0.125, 0.125, 0.15, 1.0))

        if is_input:
            # Triangle pointing right (flow arrow)
            r = size * 0.45
            p1 = imgui.ImVec2(center.x - r * 0.6, center.y - r)
            p2 = imgui.ImVec2(center.x + r, center.y)
            p3 = imgui.ImVec2(center.x - r * 0.6, center.y + r)
            if connected:
                draw_list.add_triangle_filled(p1, p2, p3, color_u32)
            else:
                draw_list.add_triangle_filled(p1, p2, p3, bg_col)
                draw_list.add_triangle(p1, p2, p3, color_u32, 1.5)
        else:
            # Circle for output
            radius = size * 0.35
            if connected:
                draw_list.add_circle_filled(center, radius, color_u32, 12)
            else:
                draw_list.add_circle_filled(center, radius, bg_col, 12)
                draw_list.add_circle(center, radius, color_u32, 12, 1.5)

        imgui.dummy(imgui.ImVec2(size, size))

    def _render_node(self, node_id: int, node_data: dict) -> None:
        type_id = node_data.get("nodeTypeID", -1)
        defn = NODE_TYPE_DEFINITIONS.get(type_id, {})
        color_id = node_data.get("nodeColorID", 0) % len(COLOR_THEMES)

        r, g, b = COLOR_THEMES[color_id % len(COLOR_THEMES)]
        header_color = imgui.ImVec4(r / 255.0, g / 255.0, b / 255.0, 0.90)
        body_color = imgui.ImVec4(r / 255.0 * 0.3, g / 255.0 * 0.3, b / 255.0 * 0.3, 0.85)
        border_color = imgui.ImVec4(r / 255.0 * 0.7, g / 255.0 * 0.7, b / 255.0 * 0.7, 1.0)

        # Pin accent color — lighter version of the node color
        pin_color_u32 = imgui.get_color_u32(
            imgui.ImVec4(
                min(r / 255.0 + 0.3, 1.0),
                min(g / 255.0 + 0.3, 1.0),
                min(b / 255.0 + 0.3, 1.0),
                1.0,
            )
        )

        rounding = 6.0
        padding = imgui.ImVec4(8, 8, 8, 4)

        ne.push_style_color(ne.StyleColor.node_bg, body_color)
        ne.push_style_color(ne.StyleColor.node_border, border_color)
        ne.push_style_var(ne.StyleVar.node_rounding, rounding)
        ne.push_style_var(ne.StyleVar.node_padding, padding)
        ne.push_style_var(ne.StyleVar.node_border_width, 1.5)

        ne.begin_node(ne.NodeId(node_id))

        # ---- Header ----
        class_name = defn.get("class_name", "Unknown")
        node_name = node_data.get("nodeName", "")

        # Record header top-left (we'll draw a colored rect here post-end_node)
        header_min = imgui.get_cursor_screen_pos()

        imgui.begin_group()
        imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 1.0, 1.0, 1.0))
        imgui.text(f"  {class_name}  ")
        if node_name and node_name != class_name:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.9, 0.9, 0.9, 0.8))
            imgui.text(f"  [{node_name}]  ")
            imgui.pop_style_color()
        imgui.pop_style_color()
        imgui.end_group()

        header_content_width = imgui.get_item_rect_size().x
        header_max_y = imgui.get_cursor_screen_pos().y

        imgui.spacing()

        # ---- Pins (column layout: inputs left, outputs right) ----
        input_ports = defn.get("input_ports", [])
        output_ports = defn.get("output_ports", [])

        # Check which pins are connected
        connected_inputs: set[int] = set()
        connected_outputs: set[tuple[int, int]] = set()
        for conn in self._current_model_connections:
            pi, fid, tid = conn[0], conn[1], conn[2]
            if fid == node_id:
                connected_outputs.add((fid, pi))
            if tid == node_id:
                connected_inputs.add(tid)

        # Determine minimum width for the pin columns
        pin_section_min_width = max(header_content_width, 120.0)

        if input_ports or output_ports:
            # Left column: inputs
            imgui.begin_group()
            for i, port_entry in enumerate(input_ports):
                pin_name = port_entry[0] if isinstance(port_entry, (list, tuple)) else str(port_entry)
                pin_id = _encode_input_pin(node_id, i)
                is_connected = node_id in connected_inputs
                ne.begin_pin(ne.PinId(pin_id), ne.PinKind.input)
                self._draw_pin_icon(True, is_connected, pin_color_u32)
                imgui.same_line()
                imgui.text(pin_name)
                ne.end_pin()
            if not input_ports:
                imgui.dummy(imgui.ImVec2(1, 0))
            imgui.end_group()

            left_width = imgui.get_item_rect_size().x

            imgui.same_line()

            # Spacer to push outputs right
            spacer = max(pin_section_min_width - left_width - 80, 20.0)
            imgui.dummy(imgui.ImVec2(spacer, 1))
            imgui.same_line()

            # Right column: outputs
            imgui.begin_group()
            for i, port_entry in enumerate(output_ports):
                pin_name = port_entry[0] if isinstance(port_entry, (list, tuple)) else str(port_entry)
                pin_id = _encode_output_pin(node_id, i)
                is_connected = (node_id, i) in connected_outputs
                ne.begin_pin(ne.PinId(pin_id), ne.PinKind.output)
                imgui.text(pin_name)
                imgui.same_line()
                self._draw_pin_icon(False, is_connected, pin_color_u32)
                ne.end_pin()
            imgui.end_group()

        ne.end_node()

        # ---- Draw header background using node's background draw list ----
        # This must be done AFTER end_node() so the draw list is valid.
        node_pos = ne.get_node_position(ne.NodeId(node_id))
        node_size = ne.get_node_size(ne.NodeId(node_id))

        if node_size.x > 0 and node_size.y > 0:
            draw_list = ne.get_node_background_draw_list(ne.NodeId(node_id))

            # The header covers from node top to just below the header text
            header_height = header_max_y - header_min.y + 4
            node_screen_min = imgui.ImVec2(
                header_min.x - padding.x,
                header_min.y - padding.y,
            )
            header_rect_max = imgui.ImVec2(
                node_screen_min.x + node_size.x,
                node_screen_min.y + header_height + padding.y,
            )

            header_u32 = imgui.get_color_u32(header_color)
            draw_list.add_rect_filled(
                node_screen_min,
                header_rect_max,
                header_u32,
                rounding,
                imgui.ImDrawFlags_.round_corners_top,
            )

            # Subtle separator line under header
            sep_color = imgui.get_color_u32(imgui.ImVec4(0, 0, 0, 0.3))
            draw_list.add_line(
                imgui.ImVec2(node_screen_min.x, header_rect_max.y),
                imgui.ImVec2(header_rect_max.x, header_rect_max.y),
                sep_color,
                1.0,
            )

        ne.pop_style_var(3)
        ne.pop_style_color(2)

    # ------------------------------------------------------------------
    # Link rendering
    # ------------------------------------------------------------------

    def _render_links(self, model: GraphModel) -> None:
        """Draw all links and build the link-ID reverse map."""
        self._link_id_map.clear()
        link_id = 1

        for conn in model.connections:
            port_idx, from_id, to_id = conn[0], conn[1], conn[2]
            from_pin = _encode_output_pin(from_id, port_idx)
            to_pin = _encode_input_pin(to_id, 0)

            ne.link(ne.LinkId(link_id), ne.PinId(from_pin), ne.PinId(to_pin))

            self._link_id_map[link_id] = (port_idx, from_id, to_id)
            link_id += 1

    # ------------------------------------------------------------------
    # Interactive link creation
    # ------------------------------------------------------------------

    def _handle_create(self, model: GraphModel) -> None:
        """Handle drag-to-connect link creation."""
        if ne.begin_create():
            from_pin_id = ne.PinId()
            to_pin_id = ne.PinId()
            if ne.query_new_link(from_pin_id, to_pin_id):
                from_raw = from_pin_id.id()
                to_raw = to_pin_id.id()

                if from_raw != 0 and to_raw != 0:
                    from_node, from_port, from_is_input = _decode_pin(from_raw)
                    to_node, to_port, to_is_input = _decode_pin(to_raw)

                    # Normalise: ensure from is output and to is input
                    if from_is_input and not to_is_input:
                        from_node, to_node = to_node, from_node
                        from_port, to_port = to_port, from_port
                        from_is_input, to_is_input = to_is_input, from_is_input

                    # Validate: one output, one input, different nodes
                    valid = (
                        not from_is_input
                        and to_is_input
                        and from_node != to_node
                        and from_node in model.nodes
                        and to_node in model.nodes
                    )

                    if valid:
                        if ne.accept_new_item():
                            model.connect(from_node, from_port, to_node)
                            log.debug(
                                "Connected %d[%d] -> %d", from_node, from_port, to_node
                            )
                    else:
                        ne.reject_new_item()
            ne.end_create()

    # ------------------------------------------------------------------
    # Interactive deletion
    # ------------------------------------------------------------------

    def _handle_delete(self, model: GraphModel) -> None:
        """Handle interactive deletion of links and nodes."""
        if ne.begin_delete():
            # --- Delete links ---
            link_id = ne.LinkId()
            while ne.query_deleted_link(link_id):
                if ne.accept_deleted_item():
                    lid = link_id.id()
                    conn = self._link_id_map.get(lid)
                    if conn is not None:
                        port_idx, from_id, to_id = conn
                        model.disconnect(from_id, port_idx, to_id)
                        log.debug(
                            "Deleted link %d (%d[%d] -> %d)",
                            lid, from_id, port_idx, to_id,
                        )

            # --- Delete nodes ---
            node_id = ne.NodeId()
            while ne.query_deleted_node(node_id):
                nid = node_id.id()
                # Protect root-level container from deletion
                node_data = model.nodes.get(nid)
                if node_data is not None:
                    type_id = node_data.get("nodeTypeID", -1)
                    if type_id == 0:
                        ne.reject_deleted_item()
                        continue
                if ne.accept_deleted_item():
                    model.delete_node(nid)
                    log.debug("Deleted node %d", nid)
                    if self._selected_node_id == nid:
                        self._selected_node_id = None
            ne.end_delete()

    # ------------------------------------------------------------------
    # Pending node creation (from palette)
    # ------------------------------------------------------------------

    def _handle_pending_create(self, model: GraphModel) -> None:
        """Create a node that was requested externally (e.g. palette click)."""
        if self._pending_create_type is None:
            return

        type_id = self._pending_create_type
        self._pending_create_type = None

        if type_id not in NODE_TYPE_DEFINITIONS:
            log.warning("Ignoring unknown pending type_id=%d", type_id)
            return

        defn = NODE_TYPE_DEFINITIONS[type_id]
        if defn.get("metadata_only"):
            log.warning("Cannot create metadata-only node type=%d", type_id)
            return

        try:
            node = model.create_node(type_id)
        except ValueError as exc:
            log.error("Failed to create node: %s", exc)
            return

        # Position the new node at the drop position (drag) or current mouse pos (click)
        screen_pos = self._pending_drop_pos if self._pending_drop_pos is not None else imgui.get_mouse_pos()
        self._pending_drop_pos = None
        canvas_mouse = ne.screen_to_canvas(screen_pos)
        ne.set_node_position(
            ne.NodeId(node["nodeID"]),
            imgui.ImVec2(canvas_mouse.x, canvas_mouse.y),
        )
        log.debug(
            "Created node %s (#%d) at (%.0f, %.0f)",
            defn["class_name"], node["nodeID"],
            canvas_mouse.x, canvas_mouse.y,
        )

    # ------------------------------------------------------------------
    # Selection tracking
    # ------------------------------------------------------------------

    def _update_selection(self) -> None:
        """Track which single node is selected (if any)."""
        nodes = ne.get_selected_nodes()
        if nodes:
            self._selected_node_id = nodes[0].id()
        else:
            self._selected_node_id = None

    # ------------------------------------------------------------------
    # Background context menu (right-click on canvas)
    # ------------------------------------------------------------------

    def _handle_context_menus(self, model: GraphModel) -> None:
        """Show right-click context menus for background, nodes, and links."""
        ne.suspend()

        # --- Link context menu ---
        link_id = ne.LinkId()
        if ne.show_link_context_menu(link_id):
            self._ctx_menu_link_id = link_id.id()
            imgui.open_popup("##LinkContextMenu")

        if imgui.begin_popup("##LinkContextMenu"):
            lid = getattr(self, "_ctx_menu_link_id", None)
            conn = self._link_id_map.get(lid) if lid is not None else None
            if conn is not None:
                port_idx, from_id, to_id = conn
                from_defn = NODE_TYPE_DEFINITIONS.get(
                    model.nodes.get(from_id, {}).get("nodeTypeID", -1), {})
                ports = from_defn.get("output_ports", [])
                port_name = ports[port_idx][0] if port_idx < len(ports) else f"port {port_idx}"
                imgui.text_disabled(f"#{from_id}.{port_name} -> #{to_id}")
                imgui.separator()
                if imgui.menu_item("Delete Link", "", False)[0]:
                    model.disconnect(from_id, port_idx, to_id)
                    log.debug("Deleted link via context menu: %d[%d] -> %d",
                              from_id, port_idx, to_id)
            else:
                imgui.text_disabled("(unknown link)")
            imgui.end_popup()

        # --- Node context menu ---
        node_nid = ne.NodeId()
        if ne.show_node_context_menu(node_nid):
            self._ctx_menu_node_id = node_nid.id()
            imgui.open_popup("##NodeContextMenu")

        if imgui.begin_popup("##NodeContextMenu"):
            nid = getattr(self, "_ctx_menu_node_id", None)
            node = model.nodes.get(nid) if nid is not None else None
            if node is not None:
                type_id = node.get("nodeTypeID", -1)
                defn = NODE_TYPE_DEFINITIONS.get(type_id, {})
                imgui.text_disabled(f"{defn.get('class_name', '?')} #{nid}")
                imgui.separator()

                # Color submenu
                if imgui.begin_menu("Set Color"):
                    for ci, cname in enumerate(COLOR_THEME_NAMES):
                        cr, cg, cb = COLOR_THEMES[ci]
                        col = imgui.ImVec4(cr / 255, cg / 255, cb / 255, 1.0)
                        imgui.push_style_color(imgui.Col_.text, col)
                        if imgui.menu_item(cname, "", False)[0]:
                            model.set_node_property(nid, "nodeColorID", ci)
                        imgui.pop_style_color()
                    imgui.end_menu()

                imgui.separator()

                # Delete (protect root)
                can_delete = type_id != 0
                if imgui.menu_item("Delete Node", "Del", False, can_delete)[0]:
                    model.delete_node(nid)
                    if self._selected_node_id == nid:
                        self._selected_node_id = None
            else:
                imgui.text_disabled("(no node)")
            imgui.end_popup()

        # --- Background context menu ---
        if ne.show_background_context_menu():
            imgui.open_popup("##CanvasContextMenu")

        if imgui.begin_popup("##CanvasContextMenu"):
            imgui.text_disabled("Add Node")
            imgui.separator()

            # Group by category
            categories: dict[str, list[tuple[int, dict]]] = {}
            for tid, defn in sorted(NODE_TYPE_DEFINITIONS.items()):
                if defn.get("metadata_only"):
                    continue
                cat = defn.get("category", "Other")
                categories.setdefault(cat, []).append((tid, defn))

            for cat_name in sorted(categories.keys()):
                if imgui.begin_menu(cat_name):
                    for tid, defn in categories[cat_name]:
                        class_name = defn["class_name"]
                        if imgui.menu_item(f"{class_name}  (#{tid})", "", False)[0]:
                            self._pending_create_type = tid
                    imgui.end_menu()

            imgui.end_popup()

        ne.resume()

