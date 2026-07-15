"""Shared WRLD service-record hooks for FO76->FO4 worldspace ports."""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Any

_log = logging.getLogger("conversion.worldspace_services")

def patch_target_worldspace_subrecords(
    *,
    source_plugin: Any,
    target_plugin_path: Path,
    worldspace_editor_id: str,
    target_game: str,
    seed_source_strings: bool = False,
) -> int:
    """Carry the FO76 source worldspace header onto the synthesized FO4 WRLD.

    Both regen paths build a bare 5-field WRLD skeleton (EDID/NAMA/DATA/NAM0/
    NAM9); the renderer/map only activate once the header (DNAM land data, ICON
    map image translated from FO76 NAM5, MNAM/ONAM/NAM4 map frame, and the
    CNAM/XLCN/NAM2/NAM3 climate/location/water links) is present. The caller has
    already saved and closed the target handle, so the ESP is reloaded into a
    fresh eager handle, the native carry copies/translates from the still-open
    ``source_plugin``, and the result is saved back in place. Returns the number
    of subrecords carried.
    """
    from creation_lib.esp import Plugin
    from creation_lib.esp.plugin import replace_plugin_with_localized_sidecars

    target_plugin = Plugin.load(target_plugin_path, game=target_game)
    try:
        # For a localized cell slice, seed the FULL source string table onto the
        # reloaded handle (which only re-hydrated the referenced subset) and set
        # the Localized flag before the carry. allocate_target_string_id appends
        # after max(used)+1, so a full table guarantees the WRLD FULL id lands
        # above every source id and can't collide with a map-marker/LCTN id.
        if seed_source_strings and source_plugin.is_localized:
            tables = source_plugin.localized_strings_by_language
            if tables:
                target_plugin.is_localized = True
                target_plugin.localized_strings_by_language = tables
                target_plugin.localized_default_language = (
                    source_plugin.localized_default_language or "en"
                )
                target_plugin.localized_string_table_types = (
                    source_plugin.localized_string_table_types
                )
        result = target_plugin.carry_worldspace_header_from_source(
            source_plugin,
            source_worldspace_editor_id=worldspace_editor_id,
            target_worldspace_editor_id=worldspace_editor_id,
        )
        copied = int(result.get("copied", 0))
        for warning in result.get("warnings", []):
            _log.warning("WRLD header carry [%s]: %s", worldspace_editor_id, warning)

        # Carry per-cell max-height (CELL.MHDT) from the coordinate-matched source
        # cells onto the synthesized exterior cells. Without it, flying actors
        # (vertibirds) path into terrain. Reuses the open target handle/save.
        mhdt = target_plugin.sync_cell_max_height_from_source(
            source_plugin,
            source_worldspace_editor_id=worldspace_editor_id,
            target_worldspace_editor_id=worldspace_editor_id,
        )
        cells_changed = int(mhdt.get("cells_changed", 0))
        for warning in mhdt.get("warnings", []):
            _log.warning("CELL MHDT carry [%s]: %s", worldspace_editor_id, warning)
        _log.info(
            "CELL MHDT carry [%s]: %d cells set (source_indexed=%d target_seen=%d "
            "unmatched=%d malformed_source=%d)",
            worldspace_editor_id,
            cells_changed,
            int(mhdt.get("source_cells_indexed", 0)),
            int(mhdt.get("target_cells_seen", 0)),
            int(mhdt.get("unmatched_target_cells", 0)),
            int(mhdt.get("malformed_source_cells", 0)),
        )

        if copied > 0 or cells_changed > 0:
            # Save to a sibling temp then atomically replace, rather than saving
            # in place. plugin_handle_load mmaps the target, and on Windows the
            # writer cannot overwrite its own mapped section — a direct in-place
            # save of a large ESM (the full plugin port's ~1.3 GB worldspace)
            # fails with os error 1224. Writing a new file sidesteps the mmap;
            # the handle is then closed (dropping the mmap) before the replace.
            # Harmless for the small cell-slice ESP, which used to save in place.
            tmp_out = f"{os.fspath(target_plugin_path)}.wrldcarry.tmp"
            target_plugin.save(tmp_out)
    finally:
        target_plugin.close()
    if copied > 0 or cells_changed > 0:
        replace_plugin_with_localized_sidecars(tmp_out, target_plugin_path)
    return copied
