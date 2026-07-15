"""Tool registry for the Bulk Toolbox workspace."""

from __future__ import annotations

from .base import BaseTool

# Categories in display order
CATEGORIES = ["DDS", "Image", "Materials", "NIF", "Havok", "Audio", "Mod Tools", "Palettes"]


def get_all_tools() -> list[BaseTool]:
    """Import and instantiate all registered tools.

    Tools are returned in category order, then alphabetical within category.
    New tools: import here and add to the list.
    """
    # Havok tools
    from .animation.annotation_extractor import AnnotationExtractorTool
    from .animation.havok_converter import HavokConverterTool
    from .animation.hkx_packer import HKXPackerTool

    # Mod Tools
    from .animation.subgraph_maker import SubGraphMakerTool
    from .assets.archlist_creator import ArchlistCreatorTool
    from .assets.bsa_extractor import BSAExtractorTool
    from .assets.folder_renamer import FolderRenamerTool
    from .assets.mass_bsa import MassBSATool
    from .assets.modlist_merger import ModlistMergerTool

    # DDS / Texture tools
    from .dds.inspector import DDSInspectorTool
    from .dds.png_exporter import DDSPNGExporterTool
    from .dds.resizer import DDSResizerTool

    # Image tools
    from .image.color_report import ColorReportTool
    from .image.image_utils import ImageUtilsTool
    from .image.upscaler import ImageUpscalerTool

    # Audio tools
    from .audio.extractor import AudioExtractorTool
    from .audio.gun_fire import GunFireTool
    from .audio.laser_beam import LaserBeamTool

    # Materials tools
    from .materials.bulk_copier import MaterialCopierTool

    # NIF tools
    from .meshes.collision_generator import CollisionGeneratorTool
    from .meshes.fbx_exporter import NifToFbxTool
    from .conversion.nif_converter import NifConverterTool

    # Palette tools
    from .palette.quantizer import ImageQuantizerTool

    tools: list[BaseTool] = [
        # DDS
        DDSInspectorTool(),
        DDSPNGExporterTool(),
        DDSResizerTool(),
        # Image
        ColorReportTool(),
        ImageUtilsTool(),
        ImageUpscalerTool(),
        # Materials
        MaterialCopierTool(),
        # NIF
        CollisionGeneratorTool(),
        NifToFbxTool(),
        NifConverterTool(),
        # Havok
        AnnotationExtractorTool(),
        HavokConverterTool(),
        HKXPackerTool(),
        # Audio
        AudioExtractorTool(),
        GunFireTool(),
        LaserBeamTool(),
        # Mod Tools
        SubGraphMakerTool(),
        ArchlistCreatorTool(),
        BSAExtractorTool(),
        MassBSATool(),
        FolderRenamerTool(),
        ModlistMergerTool(),
        # Palettes
        ImageQuantizerTool(),
    ]
    return tools
