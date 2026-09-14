"""Texture dependency extraction helpers for the dependency walker."""
from __future__ import annotations

import logging

_log = logging.getLogger("conversion.walker")

# Texture field names that appear inline on BSEffectShaderProperty /
# BSLightingShaderProperty blocks when there's no linked .bgsm/.bgem
# material file. Mirrors ``_INLINE_SHADER_TEXTURE_FIELDS`` in
# ``py_creation_lib/python/creation_lib/preprocessor/nifs.py`` — keep in sync.
_INLINE_SHADER_TEXTURE_FIELDS = (
    "Source Texture",
    "Greyscale Texture",
    "Env Map Texture",
    "Normal Texture",
    "Env Mask Texture",
    "Reflectance Texture",
    "Lighting Texture",
    "Emit Gradient Texture",
)


def extract_inline_textures(nif_path: str) -> list[str]:
    """Open a NIF and return every inline shader texture ref it holds.

    Fallback for when the ``NifIndexLookup.get_textures`` index lacks inline shader
    textures. Slower than an index lookup, so call it only on NIFs in a walk graph
    (typically ~30-100 per weapon conversion).

    Skips shader blocks with a linked ``.bgsm``/``.bgem``: the engine ignores their
    inline fields and the textures come from the BGSM walker. Returns ``[]`` on any
    read error.
    """
    try:
        from creation_lib.nif.nif_file import NifFile
    except Exception:
        return []
    try:
        nif = NifFile.load(nif_path)
    except Exception as e:
        _log.debug("Inline shader texture walk failed for %s: %s", nif_path, e)
        return []

    texs: list[str] = []
    for block in nif.blocks:
        if block.type_name not in (
            "BSEffectShaderProperty",
            "BSLightingShaderProperty",
        ):
            continue
        # Skip blocks that delegate to a .bgsm/.bgem — the material walker
        # handles those and the inline fields are inert.
        mat = block.get_field("Name") or ""
        if (
            isinstance(mat, str)
            and mat
            and (mat.lower().endswith(".bgsm") or mat.lower().endswith(".bgem"))
        ):
            continue

        # Path A: direct fields on the block (older FO4 layout).
        for field_name in _INLINE_SHADER_TEXTURE_FIELDS:
            val = block.get_field(field_name)
            if isinstance(val, str) and val.strip():
                texs.append(val.strip().replace("\\", "/"))

        # Path B: nested ``Shader Property Data`` struct (FO76+).
        data = block.get_field("Shader Property Data")
        if isinstance(data, dict):
            for field_name in _INLINE_SHADER_TEXTURE_FIELDS:
                val = data.get(field_name)
                if isinstance(val, str) and val.strip():
                    texs.append(val.strip().replace("\\", "/"))

    # Dedupe while preserving order.
    seen: set[str] = set()
    out: list[str] = []
    for t in texs:
        key = t.lower()
        if key not in seen:
            seen.add(key)
            out.append(t)
    return out
