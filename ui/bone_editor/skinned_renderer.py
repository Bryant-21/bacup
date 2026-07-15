"""GPU skinned mesh renderer for bone editor.

Re-exports from shared py_creation_lib/python/creation_lib/nif/rendering for backward compatibility.
"""
from creation_lib.nif.rendering.skinned_renderer import SkinnedRenderer, SkinnedMesh

__all__ = ["SkinnedRenderer", "SkinnedMesh"]
