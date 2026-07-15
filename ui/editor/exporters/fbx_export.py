"""Export loaded NIF scene to FBX format using Autodesk FBX SDK.

Reads geometry, skeleton, and skinning from the NIF data.
"""

import logging
from pathlib import Path

try:
    from creation_lib.fbx import export_nif_to_fbx, FbxExportOptions

    HAS_FBX_EXPORT = True
except ImportError:
    HAS_FBX_EXPORT = False

_log = logging.getLogger("nif_editor.fbx_export")


def export_fbx(
    nif,
    output_path: str | None = None,
    nif_path: str | None = None,
    include_skeleton: bool = True,
    include_weights: bool = True,
) -> str | None:
    """Export the loaded NIF to an FBX file.

    Args:
        nif: NifFile instance with loaded data.
        output_path: Optional output file path. Auto-generated if None.
        nif_path: Original NIF file path (for auto-generating output name).
        include_skeleton: Export bone hierarchy.
        include_weights: Export skin weights.

    Returns:
        The output file path on success, or None on failure.
    """
    if not HAS_FBX_EXPORT:
        _log.error("FBX export not available (Autodesk FBX SDK not installed)")
        return None

    if nif is None:
        _log.error("No NIF loaded to export")
        return None

    if output_path is None:
        if nif_path:
            output_path = str(Path(nif_path).with_suffix(".fbx"))
        else:
            output_path = "export.fbx"

    options = FbxExportOptions(
        include_skeleton=include_skeleton,
        include_weights=include_weights,
    )

    return export_nif_to_fbx(nif, output_path, options)
