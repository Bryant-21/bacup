"""Block operations — undoable add/remove/duplicate/copy/paste/reorder.

All operations use SnapshotAction for undo since they modify the block list.
"""

import copy
import logging

from imgui_bundle import imgui

from creation_lib.nif.actions import SnapshotAction
from creation_lib.nif.schema import get_schema
from creation_lib.nif.types import categorize_block_type, BLOCK_CATEGORIES

_log = logging.getLogger("nif_editor.block_ops")

# FO4 version tags — types with ANY of these are FO4-compatible
_FO4_VERSIONS = frozenset({
    "#FO4#", "#FO4_AND_LATER#", "#F76#",
    "#BETHESDA#", "#FO3_AND_LATER#", "#SKY_AND_LATER#",
    "#SSE#", "#BS_GTE_152#",
})

# Version tags that definitely exclude FO4
_NON_FO4_VERSIONS = frozenset({
    "V2_3", "V4_0_0_2", "#SKY#", "#FO3#", "#STF#",
})


def get_all_block_types() -> dict[str, list[tuple[str, str]]]:
    """Return {category: [(type_name, compat)]} for all non-abstract block types.

    compat is one of: "fo4", "maybe", "non_fo4"
    - "fo4": version list includes a FO4-compatible tag
    - "maybe": no version restriction (works across all versions)
    - "non_fo4": version list only contains non-FO4 tags
    """
    schema = get_schema()
    categorized: dict[str, list[tuple[str, str]]] = {cat: [] for cat in BLOCK_CATEGORIES}
    categorized["Other"] = []
    for name, obj in schema.niobjects.items():
        if obj.abstract:
            continue
        cat = categorize_block_type(name, schema)
        compat = _classify_fo4_compat(obj.versions)
        categorized.setdefault(cat, []).append((name, compat))
    # Sort each category alphabetically by type name
    for cat in categorized:
        categorized[cat].sort(key=lambda t: t[0])
    return categorized


def _classify_fo4_compat(versions: list[str]) -> str:
    """Classify a type's FO4 compatibility based on its version list."""
    if not versions:
        return "maybe"  # No restriction — works everywhere
    version_set = frozenset(versions)
    if version_set & _FO4_VERSIONS:
        return "fo4"
    return "non_fo4"


def get_block_type_description(type_name: str) -> str:
    """Return the nif.xml description for a block type, or empty string."""
    schema = get_schema()
    obj = schema.niobjects.get(type_name)
    return obj.description if obj else ""


class BlockOperations:
    """Stateful block operations manager attached to the app."""

    def __init__(self, app):
        self.app = app
        # Clipboard for copy/paste — stores (block_ids, deep-copied blocks)
        self._clipboard = None
        self._insert_type_idx = 0

    def insert_block(self, type_name: str, parent_block_id: int | None = None):
        """Insert a new block of the given type, optionally as child of parent."""
        nif = self.app.nif_file
        if not nif:
            return

        cmd = SnapshotAction(_description=f"Insert {type_name}")
        cmd.capture_before(nif)

        try:
            new_block = nif.add_block(type_name)
            if new_block is None:
                return
            new_id = new_block.block_id

            # If parent specified, add to its Children array
            if parent_block_id is not None:
                parent = nif.get_block(parent_block_id)
                if parent and nif.schema.is_subtype_of(parent.type_name, "NiNode"):
                    children = parent.get_field("Children") or []
                    children.append(new_id)
                    parent.set_field("Children", children)
                    num = parent.get_field("Num Children")
                    if num is not None:
                        parent.set_field("Num Children", len(children))

            cmd.capture_after(nif)
            self.app.undo_manager.push(self.app.registry.active_id, cmd)
            self.app._nif_dirty = True
            self.app.rebuild_scene_from_nif()
        except Exception as e:
            _log.error("Insert block failed: %s", e)
            cmd.undo(nif)

    def remove_block(self, block_id: int):
        """Remove a block and all descendants."""
        nif = self.app.nif_file
        if not nif or block_id == 0:  # Never remove root
            return

        cmd = SnapshotAction(_description=f"Remove block {block_id}")
        cmd.capture_before(nif)

        try:
            # Collect the block and all descendants
            to_remove = self._collect_descendants(nif, block_id)
            nif.remove_blocks(to_remove)

            cmd.capture_after(nif)
            self.app.undo_manager.push(self.app.registry.active_id, cmd)
            self.app._nif_dirty = True
            self.app.rebuild_scene_from_nif()
        except Exception as e:
            _log.error("Remove block failed: %s", e)
            cmd.undo(nif)

    def duplicate_branch(self, block_id: int):
        """Duplicate a block and all its descendants."""
        nif = self.app.nif_file
        if not nif:
            return

        cmd = SnapshotAction(_description=f"Duplicate block {block_id}")
        cmd.capture_before(nif)

        try:
            source_ids = self._collect_descendants(nif, block_id)
            # Deep-copy blocks and add them as new blocks
            id_map = {}
            for sid in source_ids:
                src = nif.get_block(sid)
                if not src:
                    continue
                new_block = copy.deepcopy(src)
                new_id = len(nif.blocks)
                new_block.block_id = new_id
                nif.blocks.append(new_block)
                id_map[sid] = new_id

            # Remap internal references in the duplicated blocks
            for old_id, new_id in id_map.items():
                new_block = nif.get_block(new_id)
                if new_block:
                    self._remap_refs(new_block, id_map, nif.schema)

            # Update header
            nif.header.num_blocks = len(nif.blocks)
            type_names = list(set(b.type_name for b in nif.blocks))
            nif.header.block_type_names = type_names
            nif.header.block_type_index = [type_names.index(b.type_name) for b in nif.blocks]
            nif.header.block_sizes = [0] * len(nif.blocks)

            # Attach to the same parent as the original
            parent = self._find_parent(nif, block_id)
            if parent and block_id in id_map:
                children = parent.get_field("Children") or []
                children.append(id_map[block_id])
                parent.set_field("Children", children)
                num = parent.get_field("Num Children")
                if num is not None:
                    parent.set_field("Num Children", len(children))

            cmd.capture_after(nif)
            self.app.undo_manager.push(self.app.registry.active_id, cmd)
            self.app._nif_dirty = True
            self.app.rebuild_scene_from_nif()
        except Exception as e:
            _log.error("Duplicate failed: %s", e)
            cmd.undo(nif)

    def copy_branch(self, block_id: int):
        """Copy a branch to the clipboard."""
        nif = self.app.nif_file
        if not nif:
            return

        source_ids = self._collect_descendants(nif, block_id)
        # Deep-copy the blocks for clipboard
        self._clipboard = copy.deepcopy([nif.get_block(bid) for bid in source_ids])
        _log.info("Copied %d blocks to clipboard", len(self._clipboard))

    def paste_branch(self, parent_block_id: int | None = None):
        """Paste clipboard blocks into the NIF."""
        nif = self.app.nif_file
        if not nif or not self._clipboard:
            return

        cmd = SnapshotAction(_description="Paste branch")
        cmd.capture_before(nif)

        try:
            # Add clipboard blocks as new blocks
            id_map = {}  # old_id -> new_id
            for block in self._clipboard:
                new_block = copy.deepcopy(block)
                old_id = new_block.block_id
                new_id = len(nif.blocks)
                new_block.block_id = new_id
                nif.blocks.append(new_block)
                id_map[old_id] = new_id

            # Remap internal references
            for old_id, new_id in id_map.items():
                new_block = nif.get_block(new_id)
                if new_block:
                    self._remap_refs(new_block, id_map, nif.schema)

            # Update header
            nif.header.num_blocks = len(nif.blocks)
            # Rebuild block type index
            type_names = list(set(b.type_name for b in nif.blocks))
            nif.header.block_type_names = type_names
            nif.header.block_type_index = [type_names.index(b.type_name) for b in nif.blocks]
            nif.header.block_sizes = [0] * len(nif.blocks)

            # Attach root of pasted branch to parent
            if parent_block_id is not None and self._clipboard:
                first_new_id = id_map.get(self._clipboard[0].block_id)
                if first_new_id is not None:
                    parent = nif.get_block(parent_block_id)
                    if parent and nif.schema.is_subtype_of(parent.type_name, "NiNode"):
                        children = parent.get_field("Children") or []
                        children.append(first_new_id)
                        parent.set_field("Children", children)
                        num = parent.get_field("Num Children")
                        if num is not None:
                            parent.set_field("Num Children", len(children))

            cmd.capture_after(nif)
            self.app.undo_manager.push(self.app.registry.active_id, cmd)
            self.app._nif_dirty = True
            self.app.rebuild_scene_from_nif()
        except Exception as e:
            _log.error("Paste failed: %s", e)
            cmd.undo(nif)

    def paste_branch_into_new(self, block_id: int):
        """Create a blank NIF and paste block_id's branch under its root."""
        source_nif = self.app.nif_file
        if not source_nif:
            return

        branch = self._copy_branch(source_nif, block_id)
        if not branch:
            return

        game_id = self.app._default_new_nif_game_id()
        queue = getattr(self.app, "queue_paste_branch_into_new", None)
        if callable(queue):
            queue(branch, block_id, game_id)
            return

        self.execute_paste_branch_into_new(branch, block_id, game_id)

    def execute_paste_branch_into_new(
        self,
        branch: list,
        source_block_id: int,
        game_id: str,
    ):
        """Apply a copied branch snapshot into a new blank NIF."""
        if not branch:
            return

        session = self.app.new_blank_nif(game_id)
        if session is None:
            return

        target_nif = session.nif
        id_map = self._append_branch(target_nif, branch)
        self._refresh_header(target_nif)
        root = target_nif.get_block(0)
        first_new_id = id_map.get(branch[0].block_id)
        if root and first_new_id is not None:
            children = root.get_field("Children") or []
            children.append(first_new_id)
            root.set_field("Children", children)
            root.set_field("Num Children", len(children))

        session.dirty = True
        self.app._nif_dirty = True
        self.app.rebuild_scene_from_nif("main")
        scene_tree = getattr(self.app, "scene_tree", None)
        if scene_tree is not None and first_new_id is not None:
            scene_tree._selected_block_id = first_new_id
            scene_tree._selected_nif_id = "main"
            scene_tree._scroll_to_selected = True
            scene_tree._expand_to_block(first_new_id)
        self.app.status_text = f"Pasted branch into new NIF: block {source_block_id}"
        _log.info(
            "Paste Branch Into New complete: source_block_id=%s pasted_root=%s blocks=%d",
            source_block_id,
            first_new_id,
            len(branch),
        )

    def _copy_branch(self, source_nif, block_id: int) -> list:
        collected_ids = self._collect_descendants(source_nif, block_id)
        source_ids = [block_id] + [bid for bid in collected_ids if bid != block_id]
        branch = [copy.deepcopy(source_nif.get_block(bid)) for bid in source_ids]
        return [block for block in branch if block is not None]

    def _append_branch(self, target_nif, branch: list) -> dict[int, int]:
        id_map = {}
        for block in branch:
            new_block = copy.deepcopy(block)
            old_id = new_block.block_id
            new_id = len(target_nif.blocks)
            new_block.block_id = new_id
            target_nif.blocks.append(new_block)
            id_map[old_id] = new_id

        for old_id, new_id in id_map.items():
            new_block = target_nif.get_block(new_id)
            if new_block:
                self._remap_refs(new_block, id_map, target_nif.schema)
        return id_map

    def move_in_parent(self, block_id: int, direction: int):
        """Move a block up (-1) or down (+1) within its parent's Children array."""
        nif = self.app.nif_file
        if not nif:
            return

        parent = self._find_parent(nif, block_id)
        if not parent:
            return

        children = parent.get_field("Children") or []
        # Find the block_id in children (may be int or dict)
        idx = None
        for i, ref in enumerate(children):
            ref_id = ref if isinstance(ref, int) else ref.get("value", ref.get("Value", -1)) if isinstance(ref, dict) else -1
            if int(ref_id) == block_id:
                idx = i
                break

        if idx is None:
            return

        new_idx = idx + direction
        if new_idx < 0 or new_idx >= len(children):
            return

        cmd = SnapshotAction(_description=f"Move block {block_id} {'up' if direction < 0 else 'down'}")
        cmd.capture_before(nif)

        children[idx], children[new_idx] = children[new_idx], children[idx]
        parent.set_field("Children", children)

        cmd.capture_after(nif)
        self.app.undo_manager.push(self.app.registry.active_id, cmd)
        self.app._nif_dirty = True
        self.app.rebuild_scene_from_nif()

    def attach_property(self, shape_block_id: int, prop_type: str):
        """Create a new property block and attach it to a shape."""
        nif = self.app.nif_file
        if not nif:
            return

        shape = nif.get_block(shape_block_id)
        if not shape:
            return

        cmd = SnapshotAction(_description=f"Attach {prop_type}")
        cmd.capture_before(nif)

        try:
            new_block = nif.add_block(prop_type)
            if new_block is None:
                return
            new_id = new_block.block_id

            # Link to shape based on property type
            if "Shader" in prop_type:
                shape.set_field("Shader Property", new_id)
            elif "Alpha" in prop_type:
                shape.set_field("Alpha Property", new_id)

            # If it's a shader property, also create a texture set
            if prop_type == "BSLightingShaderProperty":
                ts_block = nif.add_block("BSShaderTextureSet")
                if ts_block is not None:
                    new_block.set_field("Texture Set", ts_block.block_id)

            cmd.capture_after(nif)
            self.app.undo_manager.push(self.app.registry.active_id, cmd)
            self.app._nif_dirty = True
            self.app.rebuild_scene_from_nif()
        except Exception as e:
            _log.error("Attach property failed: %s", e)
            cmd.undo(nif)

    @property
    def has_clipboard(self) -> bool:
        return self._clipboard is not None and len(self._clipboard) > 0

    # -------------------------------------------------------------------
    # Helpers
    # -------------------------------------------------------------------

    def _collect_descendants(self, nif, block_id: int) -> list[int]:
        """Collect block_id and all blocks reachable via Children/Ref fields."""
        visited = set()
        queue = [block_id]
        while queue:
            bid = queue.pop(0)
            if bid in visited or bid < 0 or bid >= len(nif.blocks):
                continue
            visited.add(bid)
            block = nif.get_block(bid)
            if block:
                refs = block.get_refs(nif.schema)
                queue.extend(refs)
        return sorted(visited)

    def _find_parent(self, nif, block_id: int):
        """Find the parent NiNode that has block_id in its Children."""
        for block in nif.blocks:
            if nif.schema.is_subtype_of(block.type_name, "NiNode"):
                children = block.get_field("Children") or []
                for ref in children:
                    ref_id = ref if isinstance(ref, int) else ref.get("value", ref.get("Value", -1)) if isinstance(ref, dict) else -1
                    if int(ref_id) == block_id:
                        return block
        return None

    def _remap_refs(self, block, id_map: dict, schema):
        """Remap Ref/Ptr fields in a block using the id_map."""
        from creation_lib.nif.nif_file import remap_block_refs
        remap_block_refs(block, id_map, schema)

    def _refresh_header(self, nif) -> None:
        nif.header.num_blocks = len(nif.blocks)
        type_names = list(dict.fromkeys(b.type_name for b in nif.blocks))
        nif.header.block_type_names = type_names
        nif.header.block_type_index = [
            type_names.index(b.type_name) for b in nif.blocks
        ]
        nif.header.block_sizes = [0] * len(nif.blocks)
