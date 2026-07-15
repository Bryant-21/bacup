# ui/mesh_workspace/texture_loader.py
"""Shared diffuse texture loader for mesh workspaces (cloth maker, weight painter).

Thin facade that extracts and loads the diffuse texture for a BSTriShape.
Falls back gracefully (returns None) on any failure so the mesh renders
without a texture (the existing default behavior).
"""
from __future__ import annotations

import logging
from pathlib import Path

import moderngl

_log = logging.getLogger("mesh_workspace.texture_loader")


def resolve_diffuse_for_shape(
    ctx: moderngl.Context,
    nif,
    shape_block,
    texture_dirs: list[Path],
    ba2_mgr=None,
) -> moderngl.Texture | None:
    """Extract and load the diffuse texture for a BSTriShape.

    Reads the shape's shader property to find the diffuse texture path
    (via BSShaderTextureSet slot 0, BSEffectShaderProperty Source Texture,
    or .bgsm/.bgem material file), resolves it against texture_dirs,
    decodes the DDS, and uploads to GPU.

    Returns a moderngl.Texture or None on any failure.
    """
    try:
        # Get shader property reference
        shader_ref = shape_block.get_field("Shader Property")
        ref_id = _get_ref_id(shader_ref) if shader_ref is not None else -1
        if ref_id < 0:
            return None

        shader_prop = nif.get_block(ref_id)
        if not shader_prop:
            return None

        # Extract diffuse texture path
        diffuse_path = _extract_diffuse_path(nif, shader_prop, texture_dirs, ba2_mgr)
        if not diffuse_path:
            return None

        # Resolve to filesystem path
        resolved = _resolve_path(diffuse_path, texture_dirs)
        if resolved is None:
            return None

        # Decode and upload
        return _load_dds_to_gpu(ctx, resolved)

    except Exception as e:
        _log.debug("Failed to resolve diffuse texture: %s", e)
        return None


def _extract_diffuse_path(nif, shader_prop, texture_dirs, ba2_mgr) -> str | None:
    """Extract the diffuse texture path string from a shader property."""
    block_type = shader_prop.type_name

    # Try material file (BGSM/BGEM) from the Name field first
    mat_name = shader_prop.get_field("Name") or ""
    if isinstance(mat_name, list):
        mat_name = "".join(str(c) for c in mat_name)
    mat_name_clean = mat_name.strip().rstrip("\x00")
    mat_lower = mat_name_clean.lower()

    if mat_name_clean and (mat_lower.endswith(".bgsm") or mat_lower.endswith(".bgem")):
        diffuse = _diffuse_from_material(mat_name_clean, texture_dirs)
        if diffuse:
            return diffuse

    # BSEffectShaderProperty: Source Texture field
    if "BSEffectShaderProperty" in block_type:
        src = shader_prop.get_field("Source Texture") or ""
        if src:
            return src
        return None

    # BSLightingShaderProperty: BSShaderTextureSet slot 0
    tex_set_ref = shader_prop.get_field("Texture Set")
    ref_id = _get_ref_id(tex_set_ref) if tex_set_ref is not None else -1
    if ref_id >= 0:
        tex_set = nif.get_block(ref_id)
        if tex_set:
            textures = tex_set.get_field("Textures") or []
            if textures and textures[0]:
                return textures[0]

    return None


def _diffuse_from_material(mat_name: str, texture_dirs: list[Path]) -> str | None:
    """Parse a BGSM/BGEM file and return the diffuse texture path."""
    try:
        # Resolve the material file itself
        resolved = _resolve_path(mat_name, texture_dirs)
        if resolved is None:
            # Try with Materials/ prefix
            resolved = _resolve_path(f"Materials/{mat_name}", texture_dirs)
        if resolved is None:
            return None

        mat_lower = mat_name.lower()
        if mat_lower.endswith(".bgsm"):
            from creation_lib.material_tools.bgsm_bin import read_bgsm
            with open(resolved, "rb") as f:
                data = read_bgsm(f)
            diffuse = (data.DiffuseTexture or "").rstrip("\x00")
            return diffuse if diffuse else None
        elif mat_lower.endswith(".bgem"):
            from creation_lib.material_tools.bgem_bin import read_bgem
            with open(resolved, "rb") as f:
                data = read_bgem(f)
            diffuse = (data.BaseTexture or "").rstrip("\x00")
            return diffuse if diffuse else None
    except Exception as e:
        _log.debug("Failed to parse material %s: %s", mat_name, e)
    return None


def _resolve_path(tex_path: str, texture_dirs: list[Path]) -> Path | None:
    """Resolve a game-relative texture/material path to an absolute file path.

    Case-insensitive search through texture_dirs.
    """
    if not tex_path:
        return None
    tex_path = tex_path.replace("\\", "/").strip()

    candidates = [tex_path]
    if not tex_path.lower().startswith("textures/") and not tex_path.lower().startswith("materials/"):
        candidates.append(f"Textures/{tex_path}")

    for base_dir in texture_dirs:
        if not base_dir.is_dir():
            continue
        for candidate in candidates:
            # Try direct join first (fast path, case-sensitive)
            direct = base_dir / candidate
            if direct.is_file():
                return direct
            # Case-insensitive walk
            resolved = _case_insensitive_resolve(base_dir, candidate)
            if resolved is not None:
                return resolved
    return None


def _case_insensitive_resolve(base: Path, rel_path: str) -> Path | None:
    """Walk path segments case-insensitively from base."""
    current = base
    for segment in rel_path.split("/"):
        if not segment:
            continue
        if not current.is_dir():
            return None
        seg_lower = segment.lower()
        found = None
        try:
            for child in current.iterdir():
                if child.name.lower() == seg_lower:
                    found = child
                    break
        except PermissionError:
            return None
        if found is None:
            return None
        current = found
    return current if current.is_file() else None


def _load_dds_to_gpu(ctx: moderngl.Context, filepath: Path) -> moderngl.Texture | None:
    """Load a DDS (or PNG/TGA) texture file and upload to GPU."""
    try:
        # Try using the editor's dds_loader if available (handles BC1-BC7 natively)
        from creation_lib.renderer.dds_loader import decode_texture, upload_decoded
        decoded = decode_texture(str(filepath))
        if decoded:
            tex = upload_decoded(ctx, decoded)
            _log.info("Loaded diffuse texture: %s (%dx%d)",
                      filepath.name, decoded.size[0], decoded.size[1])
            return tex
    except ImportError:
        pass
    except Exception as e:
        _log.debug("dds_loader failed for %s: %s", filepath.name, e)

    # Fallback: Pillow for simple formats
    try:
        from PIL import Image
        img = Image.open(filepath).convert("RGBA")
        tex = ctx.texture(img.size, 4, img.tobytes())
        tex.build_mipmaps()
        tex.filter = (moderngl.LINEAR_MIPMAP_LINEAR, moderngl.LINEAR)
        _log.info("Loaded diffuse texture (Pillow): %s (%dx%d)",
                  filepath.name, img.size[0], img.size[1])
        return tex
    except Exception as e:
        _log.debug("Pillow fallback failed for %s: %s", filepath.name, e)

    return None


def _get_ref_id(ref) -> int:
    """Extract block index from a NIF reference value."""
    if isinstance(ref, (int, float)):
        return int(ref)
    if isinstance(ref, dict):
        return int(ref.get("value", ref.get("Value", -1)))
    return -1
