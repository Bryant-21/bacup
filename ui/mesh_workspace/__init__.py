"""Shared mesh workspace base for 3D editing tools.

Extracted from ui/weight_painter/ to provide a common foundation
for the weight painter and cloth maker (and future mesh tools).
"""
from .base_app import MeshWorkspaceBase
from .scene_settings import SceneSettings
from .undo import UndoStack
