"""Scene tree panel — imgui TreeNode hierarchy of NIF blocks.

Displays the NIF block hierarchy as a tree, with click-to-select
synchronized with the 3D viewport selection. Right-click context
menus provide block operations (add, remove, copy, paste, etc.).

Supports multiple NIF sessions — each gets a header row and its
own block tree underneath.
"""

import logging
from pathlib import Path

from imgui_bundle import imgui

from creation_lib.nif.types import categorize_block_type

from .collision_info import (
    is_collision_block,
    summarize_collision_block,
    summarize_np_body_shape,
)
from .properties_header import HEADER_BLOCK_ID
from ui.editor.nif_file_types import NIF_LIKE_FILETYPES

_log = logging.getLogger("nif_editor.scene_tree")

# Color coding by block category (matches spec 6-color scheme)
CATEGORY_COLORS = {
    "Scene Nodes": (0.9, 0.8, 0.5, 1.0),        # Yellow
    "Geometry": (0.6, 0.9, 0.6, 1.0),             # Green
    "Shader Properties": (0.5, 0.7, 0.9, 1.0),    # Blue
    "Material Data": (0.5, 0.7, 0.9, 1.0),         # Blue
    "Alpha/Blending": (0.5, 0.7, 0.9, 1.0),        # Blue
    "Animation Controllers": (0.4, 0.9, 0.9, 1.0), # Cyan
    "Collision": (0.9, 0.7, 0.4, 1.0),             # Orange
    "Constraints": (0.9, 0.7, 0.4, 1.0),           # Orange
}
DEFAULT_COLOR = (0.7, 0.7, 0.7, 1.0)
LARGE_TREE_DEFAULT_OPEN_BLOCK_LIMIT = 5000


def _should_default_open_root(nif) -> bool:
    return len(getattr(nif, "blocks", []) or []) <= LARGE_TREE_DEFAULT_OPEN_BLOCK_LIMIT


class SceneTreePanel:
    """imgui panel displaying the NIF block hierarchy as a tree."""

    def __init__(self, app):
        self.app = app
        self._visible = True
        self.window_name = "Scene Tree"
        self._selected_nif_id = None
        self._selected_block_id = None
        self._filter_text = ""
        self._insert_search = ""
        self._expand_path = set()       # Block IDs to force-open in tree
        self._scroll_to_selected = False  # Scroll to selected on next frame

        # Listen for selection changes from viewport
        if hasattr(app, 'selection_mgr'):
            app.selection_mgr.on_selection_changed(self._on_viewport_select)

    def _on_viewport_select(self, nif_id, block_id):
        """Sync tree selection when viewport selection changes."""
        self._selected_nif_id = nif_id
        self._selected_block_id = block_id
        if block_id is not None:
            self._expand_to_block(block_id)
            self._scroll_to_selected = True
        else:
            self._expand_path = set()
            self._scroll_to_selected = False

    def _expand_to_block(self, target_id: int):
        """Compute ancestor path from root to target block, store in _expand_path."""
        nif = self.app.nif_file
        if not nif or not nif.blocks:
            self._expand_path = set()
            return

        # BFS to find path from root (block 0) to target
        parent_map = {0: None}
        queue = [0]
        found = (target_id == 0)
        while queue and not found:
            bid = queue.pop(0)
            block = nif.get_block(bid)
            if not block:
                continue
            for _, ref_ids in self._get_ref_fields(block, nif):
                for ref_id in ref_ids:
                    if ref_id not in parent_map:
                        parent_map[ref_id] = bid
                        queue.append(ref_id)
                        if ref_id == target_id:
                            found = True
                            break
                if found:
                    break

        # Walk back from target to root to get ancestor set
        path = set()
        cur = target_id
        while cur is not None and cur in parent_map:
            path.add(cur)
            cur = parent_map[cur]
        if cur is not None:
            path.add(cur)
        self._expand_path = path

    def draw(self):
        """Draw the scene tree panel."""
        if not self._visible:
            return

        expanded, opened = imgui.begin(self.window_name, True)
        if not opened:
            self._visible = False
            imgui.end()
            return

        registry = getattr(self.app, 'registry', None)
        if not registry or not registry.sessions:
            imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "No NIF loaded")
            imgui.end()
            return

        # Filter
        changed, self._filter_text = imgui.input_text(
            "Filter", self._filter_text
        )

        imgui.separator()

        # Total block count across all sessions
        total = sum(len(s.nif.blocks) for s in registry.all_sessions() if s.nif)
        nif_count = len(registry.sessions)
        label = f"{total} blocks" if nif_count == 1 else f"{total} blocks across {nif_count} NIFs"
        imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0), label)

        # Scrolling region
        imgui.begin_child("tree_scroll", imgui.ImVec2(0, 0), imgui.ChildFlags_.borders.value)

        # Draw each NIF session with header + block tree
        for session in registry.all_sessions():
            if not session.nif or not session.nif.blocks:
                continue
            self._draw_nif_header(session)
            self._draw_block_tree(session.nif, session.nif.blocks[0],
                                  nif_id=session.nif_id)

        # Clear expand path after drawing (one-shot)
        self._expand_path = set()

        imgui.end_child()
        imgui.end()

    def _draw_nif_header(self, session):
        """Draw the NIF label row — click to view/edit header in Properties."""
        name = Path(session.file_path).name
        dirty = " *" if session.dirty else ""
        role = "(main)" if session.parent_nif_id is None else f"(attached: {session.attachment_point})"
        label = f"{name} {role}{dirty}"

        is_header_selected = (
            self._selected_nif_id == session.nif_id
            and self._selected_block_id == HEADER_BLOCK_ID
        )

        flags = imgui.TreeNodeFlags_.leaf.value | imgui.TreeNodeFlags_.no_tree_push_on_open.value
        if is_header_selected:
            flags |= imgui.TreeNodeFlags_.selected.value
            imgui.push_style_color(imgui.Col_.header.value, imgui.ImVec4(0.2, 0.3, 0.5, 1.0))

        imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.9, 0.9, 0.5, 1.0))
        imgui.tree_node_ex(f"##nif_{session.nif_id}", flags, label)
        imgui.pop_style_color()
        if is_header_selected:
            imgui.pop_style_color()

        # Left-click: select header for properties panel
        if imgui.is_item_clicked(0):
            self._select_block(HEADER_BLOCK_ID, session.nif_id)
        # Right-click selects too so context menu acts on this NIF
        if imgui.is_item_clicked(1):
            self._select_block(HEADER_BLOCK_ID, session.nif_id)

        # Right-click context menu
        if imgui.begin_popup_context_item(f"nif_ctx_{session.nif_id}"):
            if imgui.menu_item("Save", "", False)[0]:
                if hasattr(self.app, '_save_session'):
                    self.app._save_session(session.nif_id)
            if session.parent_nif_id is not None:
                if imgui.menu_item("Detach NIF", "", False)[0]:
                    if hasattr(self.app, 'detach_nif'):
                        self.app.detach_nif(session.nif_id)
            if session.nif_id == "main":
                if imgui.menu_item("Bash NIF...", "", False)[0]:
                    self._open_bash_picker()
            imgui.end_popup()

    def _get_ref_fields(self, block, nif):
        """Yield (field_name, [block_ids]) for all Ref/Ptr fields."""
        from creation_lib.nif.schema import build_field_def_map
        fdef_map = build_field_def_map(nif.schema, block.type_name)

        for name, value in block.fields:
            fdef = fdef_map.get(name)
            if fdef is None:
                continue
            if fdef.type in ("Ref", "Ptr") or fdef.template in ("Ref", "Ptr"):
                refs = []
                if isinstance(value, int) and value >= 0:
                    refs.append(value)
                elif isinstance(value, list):
                    refs.extend(v for v in value if isinstance(v, int) and v >= 0)
                if refs:
                    yield name, refs

    def _block_matches_filter(self, nif, block, filter_lower, visited=None):
        """Check if a block or any of its ref-linked descendants match the filter."""
        if visited is None:
            visited = set()
        if block.block_id in visited:
            return False
        visited.add(block.block_id)

        name_field = block.get_field("Name")
        if isinstance(name_field, list):
            name_field = "".join(str(c) for c in name_field)
        name = name_field or ""

        if filter_lower in name.lower() or filter_lower in block.type_name.lower():
            return True

        # Recurse into all ref-linked descendants
        for _, ref_ids in self._get_ref_fields(block, nif):
            for ref_id in ref_ids:
                child = nif.get_block(ref_id)
                if child and self._block_matches_filter(nif, child, filter_lower, visited):
                    return True
        return False

    def _draw_block_tree(self, nif, block, depth=0, visited=None, nif_id="main"):
        """Recursively draw a block and its children as tree nodes."""
        if visited is None:
            visited = set()

        schema = nif.schema
        block_id = block.block_id
        type_name = block.type_name

        # Cycle detection — render as dimmed link node
        if block_id in visited:
            imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.5, 0.5, 0.5, 0.6))
            imgui.tree_node_ex(f"-> [{block_id}] {type_name} (see above)",
                           imgui.TreeNodeFlags_.leaf.value | imgui.TreeNodeFlags_.no_tree_push_on_open.value)
            if imgui.is_item_clicked(0):
                self._select_block(block_id, nif_id)
            imgui.pop_style_color()
            return
        visited.add(block_id)

        # Get name
        name_field = block.get_field("Name")
        if isinstance(name_field, list):
            name_field = "".join(str(c) for c in name_field)
        name = name_field or ""

        # Filter — check this block and all descendants
        if self._filter_text:
            filter_lower = self._filter_text.lower()
            if not self._block_matches_filter(nif, block, filter_lower):
                return

        # Hidden state — check per-session hidden IDs
        registry = getattr(self.app, 'registry', None)
        session = registry.sessions.get(nif_id) if registry else None
        hidden_ids = session.hidden_block_ids if session else set()
        is_hidden = block_id in hidden_ids

        # Build display label
        label = f"[{block_id}] {type_name}"
        if name:
            label += f': "{name}"'
        # Append a short collision summary (e.g. "(convex hull, 42 verts)")
        coll_detail_lines: list[str] = []
        if is_collision_block(type_name):
            suffix, coll_detail_lines = summarize_collision_block(nif, block)
            if suffix:
                label += f"  {suffix}"
        else:
            # Node with attached collision: inline summary from the attached chain.
            coll_ref = block.get_field("Collision Object")
            if isinstance(coll_ref, int) and coll_ref >= 0:
                coll_obj = nif.get_block(coll_ref)
                if coll_obj is not None:
                    _, coll_detail_lines = summarize_collision_block(nif, coll_obj)
                    # Short suffix: shape type if resolvable.
                    shape_type = self._node_collision_shape_type(nif, coll_obj)
                    if shape_type:
                        label += f"  (collision: {shape_type})"
        if is_hidden:
            label = "[H] " + label

        # Determine if this has any ref children
        ref_fields = list(self._get_ref_fields(block, nif))
        has_children = len(ref_fields) > 0

        # BSConnectPoint::Parents has expandable connect point entries
        if type_name == "BSConnectPoint::Parents":
            cp_list = block.get_field("Connect Points") or []
            if cp_list:
                has_children = True
        is_node = schema.is_subtype_of(type_name, "NiNode")
        is_shape = schema.is_subtype_of(type_name, "BSTriShape")
        is_selected = (block_id == self._selected_block_id and nif_id == self._selected_nif_id)

        # Color coding by category (dimmed when hidden)
        category = categorize_block_type(type_name, schema)
        color = CATEGORY_COLORS.get(category, DEFAULT_COLOR)
        if is_hidden:
            color = (color[0] * 0.45, color[1] * 0.45, color[2] * 0.45, 0.55)
        imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(*color))

        # Highlight selected
        if is_selected:
            imgui.push_style_color(
                imgui.Col_.header.value, imgui.ImVec4(0.2, 0.3, 0.5, 1.0)
            )

        flags = imgui.TreeNodeFlags_.open_on_arrow.value
        if not has_children:
            flags |= imgui.TreeNodeFlags_.leaf.value
        if is_selected:
            flags |= imgui.TreeNodeFlags_.selected.value
        if depth == 0 and _should_default_open_root(nif):
            flags |= imgui.TreeNodeFlags_.default_open.value

        # Force-open nodes on the path to the selected block
        if block_id in self._expand_path and has_children:
            imgui.set_next_item_open(True)

        node_open = imgui.tree_node_ex(label, flags)

        # Scroll to selected node
        if is_selected and self._scroll_to_selected:
            imgui.set_scroll_here_y(0.5)
            self._scroll_to_selected = False

        # Handle left-click selection
        if imgui.is_item_clicked(0):
            self._select_block(block_id, nif_id)

        # Right-click also selects (so context menu acts on correct block)
        if imgui.is_item_clicked(1):
            self._select_block(block_id, nif_id)

        # Right-click context menu (uses last item = tree_node)
        self._draw_context_menu(block_id, nif, block, is_node, is_shape)

        # Tooltip on hover
        if imgui.is_item_hovered():
            imgui.begin_tooltip()
            imgui.text(f"Type: {type_name}")
            imgui.text(f"Block ID: {block_id}")
            imgui.text(f"Category: {category}")
            if is_shape:
                try:
                    vd = block.get_field("Vertex Data") or []
                    tri = block.get_field("Triangles") or []
                    imgui.text(f"Vertices: {len(vd)}")
                    imgui.text(f"Triangles: {len(tri)}")
                except Exception:
                    pass
            elif is_collision_block(type_name):
                for line in coll_detail_lines:
                    imgui.text(line)
            elif coll_detail_lines:
                imgui.separator()
                imgui.text_colored(imgui.ImVec4(0.9, 0.7, 0.4, 1.0), "Attached Collision:")
                for line in coll_detail_lines:
                    imgui.text(line)
            imgui.end_tooltip()

        if is_selected:
            imgui.pop_style_color()
        imgui.pop_style_color()

        # Recurse into ref children grouped by field name
        if node_open:
            for field_name, ref_ids in ref_fields:
                # Show field name header if multiple ref fields or non-obvious name
                if len(ref_fields) > 1:
                    imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.5, 0.5, 0.5, 0.8))
                    imgui.tree_node_ex(f"{field_name}",
                                   imgui.TreeNodeFlags_.leaf.value | imgui.TreeNodeFlags_.no_tree_push_on_open.value)
                    imgui.pop_style_color()

                for ref_id in ref_ids:
                    child = nif.get_block(ref_id)
                    if child:
                        self._draw_block_tree(nif, child, depth + 1, visited, nif_id=nif_id)

            # Show individual connect points as expandable children
            if type_name == "BSConnectPoint::Parents":
                self._draw_connect_point_children(nif, block, nif_id=nif_id)

            imgui.tree_pop()

    @staticmethod
    def _node_collision_shape_type(nif, coll_obj) -> str | None:
        """Resolve bhkCollisionObject -> Body -> Shape -> type_name for inline hint."""
        try:
            np_shape = summarize_np_body_shape(nif, coll_obj)
            if np_shape:
                return np_shape
            body_ref = coll_obj.get_field("Body")
            if not isinstance(body_ref, int) or body_ref < 0:
                return None
            body = nif.get_block(body_ref)
            if body is None:
                return None
            shape_ref = body.get_field("Shape")
            if isinstance(shape_ref, int) and shape_ref >= 0:
                shape = nif.get_block(shape_ref)
                if shape is not None:
                    inner_ref = shape.get_field("Shape")
                    if isinstance(inner_ref, int) and inner_ref >= 0:
                        inner = nif.get_block(inner_ref)
                        if inner is not None:
                            return inner.type_name
                    return shape.type_name
            return body.type_name
        except Exception:
            return None

    def _select_block(self, block_id: int, nif_id: str = "main"):
        """Select a block in both the tree and the selection manager."""
        if hasattr(self.app, 'selection_mgr'):
            if block_id == HEADER_BLOCK_ID:
                # Header lives outside the block tree — clear 3D selection.
                self.app.selection_mgr.deselect()
            else:
                self.app.selection_mgr.select_by_id(nif_id, block_id)
        # Set tree state AFTER any deselect callback so the highlight sticks.
        self._selected_nif_id = nif_id
        self._selected_block_id = block_id
        # For blocks without a SceneNode (shaders, textures, etc.) and for
        # the header sentinel, select_by_id won't notify — call properties
        # directly so it refreshes.
        if hasattr(self.app, 'properties'):
            self.app.properties._on_select(nif_id, block_id)
        # Update active NIF
        if nif_id and nif_id in self.app.registry.sessions:
            self.app.registry.active_id = nif_id

    def _draw_connect_point_children(self, nif, block, nif_id="main"):
        """Draw individual BSConnectPoint entries as selectable tree leaves."""
        connect_points = block.get_field("Connect Points") or []
        if not connect_points:
            return

        cp_display = getattr(self.app, 'connect_point_display', None)

        for i, cp in enumerate(connect_points):
            if not isinstance(cp, dict):
                continue

            cp_name = cp.get("Name", "")
            if isinstance(cp_name, list):
                cp_name = "".join(str(c) for c in cp_name)
            cp_parent = cp.get("Parent", "")
            if isinstance(cp_parent, list):
                cp_parent = "".join(str(c) for c in cp_parent)

            # Check if a child NIF is attached to this CP
            attached = self._get_attached_nif(nif_id, cp_name)

            label = f"CP[{i}]: {cp_name}"
            if attached:
                label += f" \u2192 {Path(attached.file_path).name}"
            if cp_parent:
                label += f"  ({cp_parent})"

            # Check if this specific CP is selected
            is_selected = (
                cp_display is not None
                and cp_display._selected_cp_block_id == block.block_id
                and cp_display._selected_cp_index == i
            )

            # Orange color for connect point entries (green tint if attached)
            if attached:
                imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(0.5, 0.9, 0.5, 1.0))
            else:
                imgui.push_style_color(imgui.Col_.text.value, imgui.ImVec4(1.0, 0.7, 0.3, 1.0))
            if is_selected:
                imgui.push_style_color(imgui.Col_.header.value, imgui.ImVec4(0.3, 0.4, 0.2, 1.0))

            flags = imgui.TreeNodeFlags_.leaf.value | imgui.TreeNodeFlags_.no_tree_push_on_open.value
            if is_selected:
                flags |= imgui.TreeNodeFlags_.selected.value

            imgui.tree_node_ex(f"{label}##{block.block_id}_{i}", flags)

            if imgui.is_item_clicked(0):
                self._select_connect_point(block.block_id, i)

            # Right-click context menu for attach/detach
            if imgui.begin_popup_context_item(f"cp_ctx_{block.block_id}_{i}"):
                if attached:
                    if imgui.menu_item(f"Detach {Path(attached.file_path).name}", "", False)[0]:
                        if hasattr(self.app, 'detach_nif'):
                            self.app.detach_nif(attached.nif_id)
                    if imgui.menu_item("Replace NIF...", "", False)[0]:
                        if hasattr(self.app, 'detach_nif'):
                            self.app.detach_nif(attached.nif_id)
                        self._open_attach_picker(nif_id, cp_name)
                else:
                    if imgui.menu_item("Attach NIF...", "", False)[0]:
                        self._open_attach_picker(nif_id, cp_name)
                imgui.end_popup()

            # Tooltip with details
            if imgui.is_item_hovered():
                imgui.begin_tooltip()
                imgui.text(f"Name: {cp_name}")
                imgui.text(f"Parent: {cp_parent}")
                t = cp.get("Translation", {})
                imgui.text(f"Pos: ({t.get('x', 0):.2f}, {t.get('y', 0):.2f}, {t.get('z', 0):.2f})")
                r = cp.get("Rotation", {})
                imgui.text(f"Rot: w={r.get('w', 1):.3f} x={r.get('x', 0):.3f} y={r.get('y', 0):.3f} z={r.get('z', 0):.3f}")
                imgui.text(f"Scale: {cp.get('Scale', 1.0)}")
                if attached:
                    imgui.separator()
                    imgui.text(f"Attached: {Path(attached.file_path).name}")
                imgui.end_tooltip()

            if is_selected:
                imgui.pop_style_color()
            imgui.pop_style_color()

    def _select_connect_point(self, block_id: int, cp_index: int):
        """Select a specific connect point and notify the display."""
        # Also select the parent block in the properties panel
        self._selected_block_id = block_id
        if hasattr(self.app, 'selection_mgr'):
            # Find the specific CP node (not just first matching block_id,
            # since all CPs from one block share the same block_id)
            sel = self.app.selection_mgr
            found = False
            for node in sel._nodes:
                if (node.block_id == block_id
                        and getattr(node, '_cp_index', None) == cp_index):
                    sel.select(node)
                    found = True
                    break
            if not found:
                sel.select_by_block_id(block_id)

        # Tell connect point display to highlight just this one
        cp_display = getattr(self.app, 'connect_point_display', None)
        if cp_display:
            cp_display.select_connect_point(block_id, cp_index)

    def _get_attached_nif(self, parent_nif_id: str, cp_name: str):
        """Find a child session attached to the given CP, or None."""
        registry = getattr(self.app, 'registry', None)
        if not registry:
            return None
        for child in registry.get_children(parent_nif_id):
            if child.attachment_point == cp_name:
                return child
        return None

    def _open_attach_picker(self, parent_nif_id: str, cp_name: str):
        """Open a file dialog to pick a NIF to attach."""
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file
            path = pick_file(
                f"Attach NIF to {cp_name}",
                NIF_LIKE_FILETYPES,
            )
            if path:
                if hasattr(self.app, 'attach_nif'):
                    self.app.attach_nif(path, parent_nif_id, cp_name)
                else:
                    _log.error("_open_attach_picker: app has no attach_nif method")
        except Exception:
            _log.exception("_open_attach_picker: failed")

    def _open_bash_picker(self):
        """Open a file dialog to pick a NIF to bash into the current root."""
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file
            path = pick_file(
                "Bash NIF into current",
                NIF_LIKE_FILETYPES,
            )
            if path:
                if hasattr(self.app, 'bash_nif'):
                    self.app.bash_nif(path)
                else:
                    _log.error("_open_bash_picker: app has no bash_nif method")
        except Exception:
            _log.exception("_open_bash_picker: failed")

    def _draw_insert_menu(self, label: str, block_ops, parent_id: int):
        """Draw a categorized insert block submenu with search and FO4 compat indicators."""
        from ui.editor.block_ops import get_all_block_types, get_block_type_description

        if imgui.begin_menu(label):
            # Search bar at top
            changed, self._insert_search = imgui.input_text(
                "##insert_search", self._insert_search
            )
            imgui.separator()

            categories = get_all_block_types()
            filter_text = self._insert_search.lower()

            for cat_name, types in categories.items():
                if not types:
                    continue
                # Filter by search text
                filtered = (
                    [(t, c) for t, c in types if filter_text in t.lower()]
                    if filter_text else types
                )
                if not filtered:
                    continue

                if imgui.begin_menu(f"{cat_name} ({len(filtered)})"):
                    for type_name, compat in filtered:
                        # Dim non-FO4 types
                        if compat == "non_fo4":
                            imgui.push_style_color(
                                imgui.Col_.text.value, imgui.ImVec4(0.5, 0.5, 0.5, 0.6)
                            )
                        if imgui.menu_item(type_name, "", False)[0]:
                            block_ops.insert_block(type_name, parent_id)
                        # Tooltip with description and compat warning
                        if imgui.is_item_hovered():
                            desc = get_block_type_description(type_name)
                            if desc or compat == "non_fo4":
                                imgui.begin_tooltip()
                                if desc:
                                    imgui.text(desc)
                                if compat == "non_fo4":
                                    imgui.text_colored(
                                        imgui.ImVec4(1.0, 0.4, 0.4, 1.0), "Not FO4-compatible"
                                    )
                                imgui.end_tooltip()
                        if compat == "non_fo4":
                            imgui.pop_style_color()
                    imgui.end_menu()

            imgui.end_menu()

    def _draw_context_menu(self, block_id: int, nif, block, is_node: bool, is_shape: bool):
        """Draw right-click context menu for a block.

        Must be called immediately after the imgui item (tree_node) it applies to.
        Uses begin_popup_context_item which auto-associates with the last item.
        """
        block_ops = getattr(self.app, 'block_ops', None)
        if not block_ops:
            return

        if imgui.begin_popup_context_item(f"ctx_{block_id}"):
            # -- Visibility toggle --
            hidden_ids = self.app.hidden_block_ids if hasattr(self.app, 'hidden_block_ids') else set()
            is_hidden = block_id in hidden_ids
            if imgui.menu_item("Show" if is_hidden else "Hide", "", False)[0]:
                if hasattr(self.app, 'toggle_node_visibility'):
                    self.app.toggle_node_visibility(block_id)

            imgui.separator()

            # -- All blocks --
            if imgui.menu_item("Copy Branch", "", False)[0]:
                block_ops.copy_branch(block_id)

            if imgui.menu_item("Paste Branch", "", False, block_ops.has_clipboard)[0]:
                target = block_id if is_node else None
                block_ops.paste_branch(target)

            if imgui.menu_item("Paste Branch Into New", "", False)[0]:
                block_ops.paste_branch_into_new(block_id)

            if imgui.menu_item("Duplicate Branch", "", False)[0]:
                block_ops.duplicate_branch(block_id)

            imgui.separator()

            if imgui.menu_item("Move Up", "", False)[0]:
                block_ops.move_in_parent(block_id, -1)

            if imgui.menu_item("Move Down", "", False)[0]:
                block_ops.move_in_parent(block_id, 1)

            imgui.separator()

            # -- Node-specific: Insert Child --
            if is_node:
                self._draw_insert_menu("Insert Child", block_ops, block_id)

            # -- Node-specific: Collision submenu --
            if is_node and hasattr(self.app, "open_collision_dialog"):
                coll_ref = block.get_field("Collision Object")
                has_coll = (
                    isinstance(coll_ref, int) and coll_ref >= 0
                )
                imgui.separator()
                if imgui.begin_menu("Collision"):
                    gen_label = (
                        "Regenerate Collision..." if has_coll else "Generate Collision..."
                    )
                    if imgui.menu_item(gen_label, "", False)[0]:
                        self.app.open_collision_dialog(block_id)
                    if imgui.menu_item("Remove Collision", "", False, has_coll)[0]:
                        self._remove_collision_on_node(nif, block_id)
                    imgui.end_menu()

            # -- Shape-specific: Collision on parent node --
            if is_shape and hasattr(self.app, "open_collision_dialog"):
                parent_node_id = _shape_collision_parent_id(nif, block_ops, block_id)
                if parent_node_id is not None:
                    parent = nif.get_block(parent_node_id)
                    coll_ref = parent.get_field("Collision Object") if parent else None
                    has_coll = isinstance(coll_ref, int) and coll_ref >= 0
                else:
                    has_coll = False
                imgui.separator()
                if imgui.begin_menu("Collision"):
                    gen_label = (
                        "Regenerate Collision on Parent..."
                        if has_coll
                        else "Generate Collision on Parent..."
                    )
                    if imgui.menu_item(gen_label, "", False, parent_node_id is not None)[0]:
                        self.app.open_collision_dialog(
                            parent_node_id,
                            source_block_ids=[block_id],
                        )
                    imgui.end_menu()

            # -- Shape-specific: Attach Property --
            if is_shape:
                if imgui.begin_menu("Attach Property"):
                    if imgui.menu_item("BSLightingShaderProperty", "", False)[0]:
                        block_ops.attach_property(block_id, "BSLightingShaderProperty")
                    if imgui.menu_item("BSEffectShaderProperty", "", False)[0]:
                        block_ops.attach_property(block_id, "BSEffectShaderProperty")
                    if imgui.menu_item("NiAlphaProperty", "", False)[0]:
                        block_ops.attach_property(block_id, "NiAlphaProperty")
                    imgui.end_menu()

            # -- Any block: Insert Sibling (adds to parent) --
            if not is_node and block_id != 0:
                parent = block_ops._find_parent(nif, block_id)
                if parent:
                    self._draw_insert_menu("Insert Sibling", block_ops, parent.block_id)

            # Animation editor
            anim_types = ("NiControllerSequence", "NiControllerManager",
                          "NiTransformInterpolator", "NiFloatInterpolator",
                          "NiTransformData", "NiFloatData", "NiKeyframeData")
            if block.type_name in anim_types:
                if imgui.menu_item("Edit Animation...", "", False)[0]:
                    anim_editor = getattr(self.app, 'animation_editor', None)
                    if anim_editor:
                        anim_editor.open_for_block(block_id)

            imgui.separator()

            # Remove (not for root)
            can_remove = block_id != 0
            if imgui.menu_item("Remove", "", False, can_remove)[0]:
                block_ops.remove_block(block_id)

            imgui.end_popup()

    def _remove_collision_on_node(self, nif, block_id: int):
        """Invoke creation_lib.nif collision removal and rebuild scene."""
        try:
            from creation_lib.nif.operations.collision import remove_collision
            result = remove_collision(nif, node_block_id=block_id)
        except Exception:
            _log.exception("remove_collision failed")
            return
        if not getattr(result, "success", False):
            _log.warning("remove_collision: %s", getattr(result, "description", "failed"))
            return
        if hasattr(self.app, "_nif_dirty"):
            self.app._nif_dirty = True
        try:
            if hasattr(self.app, "rebuild_scene_from_nif"):
                self.app.rebuild_scene_from_nif()
        except Exception:
            _log.exception("rebuild_scene_from_nif failed after remove_collision")
        renderer = getattr(self.app, "renderer", None)
        if renderer is not None:
            try:
                renderer._collision_dirty = True
            except Exception:
                pass


def _shape_collision_parent_id(nif, block_ops, shape_block_id: int) -> int | None:
    parent = block_ops._find_parent(nif, shape_block_id)
    if parent is None:
        return None
    if not nif.schema.is_subtype_of(parent.type_name, "NiNode"):
        return None
    return int(parent.block_id)


def _get_ref_id(ref) -> int:
    """Extract block index from a reference value."""
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1
