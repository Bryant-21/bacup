"""Per-phase operators shared by conversion workflows.

Each module exposes a function with the canonical phase signature:

    def <phase_name>(records_or_assets, ctx, runner, progress) -> <result>

The unified workflow composes these phases. The functions are free-standing so
workflows can mix and match.
"""

from bacup_lib.pipeline.animations import convert_animations
from bacup_lib.pipeline.btos import convert_btos, convert_btos_v2
from bacup_lib.pipeline.drivers import synthesize_drivers
from bacup_lib.pipeline.havok import convert_havok
from bacup_lib.pipeline.materials import convert_materials
from bacup_lib.pipeline.nifs import convert_nifs
from bacup_lib.pipeline.sounds import copy_sounds, copy_sounds_native
from bacup_lib.pipeline.terrain import convert_terrain
from bacup_lib.pipeline.textures import convert_textures
from bacup_lib.pipeline.translate import resolve_dependencies

__all__ = [
    "resolve_dependencies",
    "convert_btos",
    "convert_btos_v2",
    "convert_nifs",
    "convert_textures",
    "convert_materials",
    "convert_havok",
    "synthesize_drivers",
    "convert_animations",
    "copy_sounds",
    "copy_sounds_native",
    "convert_terrain",
]
