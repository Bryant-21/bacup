"""Node Palette panel — imgui tree view for selecting behavior node types.

Displays all non-metadata node types organized by category with a search
filter. When the user clicks a type, `selected_type_id` is set for that
frame so the canvas can read it and spawn a new node.
"""

from imgui_bundle import imgui

from .node_types import NODE_TYPE_DEFINITIONS, CATEGORY_ORDER, CATEGORY_LABELS


class NodePalettePanel:
    """Dockable imgui panel listing every placeable behavior node type."""

    def __init__(self) -> None:
        self._filter_text: str = ""
        self.selected_type_id: int | None = None  # set when user clicks a node
        self.window_name: str = "Node Palette"
        self._visible: bool = True

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def render(self) -> None:
        """Render the palette inside a docked imgui window.

        After calling this, check ``self.selected_type_id``.  If it is not
        *None*, the user clicked a node type this frame.
        """
        if not self._visible:
            return
        visible, _ = imgui.begin(self.window_name)
        if not visible:
            imgui.end()
            return

        # Reset every frame — the caller reads the value between render calls.
        self.selected_type_id = None

        # --- Search input ------------------------------------------------
        changed, self._filter_text = imgui.input_text_with_hint(
            "##palette_filter", "Search nodes...", self._filter_text
        )

        imgui.separator()

        # --- Category tree -----------------------------------------------
        filter_lower = self._filter_text.lower()

        for cat in CATEGORY_ORDER:
            label = CATEGORY_LABELS.get(cat, cat)

            # Collect visible (non-metadata) nodes that match the filter.
            nodes_in_cat: list[tuple[int, str]] = []
            for type_id, defn in NODE_TYPE_DEFINITIONS.items():
                if defn.get("metadata_only"):
                    continue
                if defn.get("category") != cat:
                    continue
                if filter_lower and filter_lower not in defn["class_name"].lower():
                    continue
                nodes_in_cat.append((type_id, defn["class_name"]))

            if not nodes_in_cat:
                continue

            # Auto-open categories when a filter is active so matches are
            # immediately visible without extra clicks.
            flags = imgui.TreeNodeFlags_.default_open.value if filter_lower else 0
            if imgui.tree_node_ex(label, flags):
                for type_id, class_name in sorted(nodes_in_cat, key=lambda x: x[1]):
                    clicked, _ = imgui.selectable(f"  {class_name}##pal_{type_id}", False)
                    if clicked:
                        self.selected_type_id = type_id
                    if imgui.begin_drag_drop_source(imgui.DragDropFlags_.none):
                        imgui.set_drag_drop_payload_py_id("NODE_TYPE", type_id)
                        imgui.text(f"  {class_name}")
                        imgui.end_drag_drop_source()
                imgui.tree_pop()

        imgui.end()
