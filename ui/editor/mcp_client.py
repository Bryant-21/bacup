"""MCP client — direct access to nif-tools and fo4-data libraries.

Instead of subprocess communication with MCP servers, this module
wraps the underlying libraries directly for in-process use by the
editor UI panels.
"""

from typing import Any

from creation_lib.nif.nif_file import NifFile, NifBlock

from creation_lib.nif.operations.copy import (
    copy_blocks,
    collect_dependency_tree,
    deep_copy_block,
)

from creation_lib.db.store import Fo4DataStore


class NifOperations:
    """High-level NIF operations wrapping the nif_file library."""

    @staticmethod
    def rename_shape(nif: NifFile, block_id: int, new_name: str) -> str:
        """Rename a shape or node block."""
        block = nif.get_block(block_id)
        if not block:
            return f"Block {block_id} not found"
        old_name = block.get_field("Name") or ""
        block.set_field("Name", new_name)
        return f"Renamed block {block_id} from '{old_name}' to '{new_name}'"

    @staticmethod
    def set_field(nif: NifFile, block_id: int, field: str, value: Any) -> str:
        """Set a field value on a block."""
        block = nif.get_block(block_id)
        if not block:
            return f"Block {block_id} not found"
        block.set_field(field, value)
        return f"Set block {block_id} field '{field}' = {value}"

    @staticmethod
    def remove_blocks(nif: NifFile, block_ids: list[int]) -> str:
        """Remove blocks by ID, remapping references."""
        nif.remove_blocks(block_ids)
        return f"Removed {len(block_ids)} block(s)"

    @staticmethod
    def copy_blocks_from(
        source: NifFile, block_ids: list[int], target: NifFile, attach_to: int | None = None
    ) -> str:
        """Copy blocks between NIF files."""
        id_map = copy_blocks(source, block_ids, target, attach_to)
        return f"Copied {len(id_map)} block(s): {id_map}"

    @staticmethod
    def find_unreferenced_blocks(nif: NifFile) -> list[int]:
        """Find blocks not referenced by any other block (excluding root)."""
        referenced: set[int] = set()
        for block in nif.blocks:
            refs = block.get_refs(nif.schema)
            referenced.update(refs)
        unreferenced = []
        for block in nif.blocks:
            if block.block_id != 0 and block.block_id not in referenced:
                unreferenced.append(block.block_id)
        return unreferenced


# Module-level singletons
_fo4_store: Fo4DataStore | None = None
_nif_ops = NifOperations()


def get_fo4_client() -> Fo4DataStore:
    """Get or create the FO4 data search client."""
    global _fo4_store
    if _fo4_store is None:
        _fo4_store = Fo4DataStore()
    return _fo4_store


def get_nif_ops() -> NifOperations:
    """Get the NIF operations helper."""
    return _nif_ops
