"""Workspace registry — lazy-imports and registers workspace classes."""

from __future__ import annotations

from importlib import import_module


_WORKSPACE_SPECS = (
    # Core workspaces
    ("mod_builder", "ui.toolkit.workspaces.mod_builder_workspace", "ModBuilderWorkspace"),
    ("esp_editor", "ui.esp_editor.workspace", "EspEditorWorkspace"),
    ("nif", "ui.toolkit.workspaces.nif_workspace", "NifWorkspace"),
    ("search", "ui.toolkit.workspaces.search_workspace", "SearchWorkspace"),
    ("papyrus", "ui.toolkit.workspaces.papyrus_workspace", "PapyrusWorkspace"),

    # Texture workspaces
    ("palette", "ui.toolkit.workspaces.palette_workspace", "PaletteWorkspace"),
    ("materials", "ui.toolkit.workspaces.material_workspace", "MaterialWorkspace"),
    ("mat_copier", "ui.toolkit.workspaces.texture_tools", "MaterialCopierWorkspace"),
    ("dds_inspector", "ui.toolkit.workspaces.texture_tools", "DDSInspectorWorkspace"),
    ("dds_png", "ui.toolkit.workspaces.texture_tools", "DDSPNGWorkspace"),
    ("dds_resizer", "ui.toolkit.workspaces.texture_tools", "DDSResizerWorkspace"),
    ("color_report", "ui.toolkit.workspaces.texture_tools", "ColorReportWorkspace"),
    ("image_utils", "ui.toolkit.workspaces.texture_tools", "ImageUtilsWorkspace"),
    ("img_upscaler", "ui.toolkit.workspaces.texture_tools", "ImageUpscalerWorkspace"),
    ("img_quantizer", "ui.toolkit.workspaces.texture_tools", "ImageQuantizerWorkspace"),

    # Audio workspaces
    ("voice_changer", "ui.toolkit.workspaces.voice_changer_workspace", "VoiceChangerWorkspace"),
    ("voice_browser", "creation_lib.ui.workspaces.voice_browser", "VoiceBrowserWorkspace"),
    ("audio_extractor", "ui.toolkit.workspaces.audio_tools", "AudioExtractorWorkspace"),
    ("gun_fire", "ui.toolkit.workspaces.audio_tools", "GunFireWorkspace"),
    ("laser_beam", "ui.toolkit.workspaces.audio_tools", "LaserBeamWorkspace"),

    # Mesh workspaces
    ("weight_painter", "ui.toolkit.workspaces.weight_painter_workspace", "WeightPainterWorkspace"),
    ("cloth_maker", "ui.toolkit.workspaces.cloth_maker_workspace", "ClothMakerWorkspace"),
    ("swf", "ui.toolkit.workspaces.swf_editor_workspace", "SwfEditorWorkspace"),

    # Animation workspaces
    ("bone_editor", "ui.toolkit.workspaces.bone_editor_workspace", "BoneEditorWorkspace"),
    ("aligner", "ui.toolkit.workspaces.aligner_workspace", "AlignerWorkspace"),

    # Havok workspaces (includes Behavior Graph)
    ("behavior", "ui.toolkit.workspaces.behavior_workspace", "BehaviorWorkspace"),
    ("annotation_extract", "ui.toolkit.workspaces.havok_tools", "AnnotationExtractorWorkspace"),
    ("hkx_viewer", "ui.toolkit.workspaces.havok_tools", "HKXViewerWorkspace"),
    ("hkx_packer", "ui.toolkit.workspaces.havok_tools", "HKXPackerWorkspace"),
    ("hkx_converter", "ui.toolkit.workspaces.havok_tools", "HKXConverterWorkspace"),

    # NIF tool workspaces
    ("nif_collision", "ui.toolkit.workspaces.nif_tools", "NIFCollisionWorkspace"),
    ("nif_fbx", "ui.toolkit.workspaces.nif_tools", "NIFToFBXWorkspace"),
    (
        "worldspace_export",
        "ui.toolkit.workspaces.worldspace_export_workspace",
        "WorldspaceExportWorkspace",
    ),
    ("world_viewer", "ui.world_viewer.workspace", "WorldViewerWorkspace"),
    ("lodgen", "ui.lodgen.workspace", "LodgenWorkspace"),

    # Mod tool workspaces
    ("bsa_viewer", "ui.bsa_viewer.workspace", "BSAViewerWorkspace"),
    ("subgraph_maker", "ui.toolkit.workspaces.mod_tools", "SubGraphMakerWorkspace"),
    ("bsa_extractor", "ui.toolkit.workspaces.mod_tools", "BSAExtractorWorkspace"),
    ("mass_bsa", "ui.toolkit.workspaces.mod_tools", "MassBSAWorkspace"),
    ("archlist_creator", "ui.toolkit.workspaces.mod_tools", "ArchlistCreatorWorkspace"),
    ("folder_renamer", "ui.toolkit.workspaces.mod_tools", "FolderRenamerWorkspace"),
    ("modlist_merger", "ui.toolkit.workspaces.mod_tools", "ModlistMergerWorkspace"),
)


def create_workspaces(toolkit_settings=None, workspace_ids=None):
    """Instantiate and return requested workspaces in display order.

    Imports are deferred to this function so that heavy workspace
    dependencies (ModernGL, GLM, numpy, etc.) are not loaded at
    module-import time — only when the toolkit actually starts up.
    """
    requested = set(workspace_ids) if workspace_ids is not None else None
    workspaces = []

    for workspace_id, module_name, class_name in _WORKSPACE_SPECS:
        if requested is not None and workspace_id not in requested:
            continue
        workspace_class = getattr(import_module(module_name), class_name)
        workspaces.append(workspace_class(toolkit_settings=toolkit_settings))

    return workspaces


def create_all_workspaces(toolkit_settings=None):
    """Instantiate and return all available workspaces in display order."""
    return create_workspaces(toolkit_settings=toolkit_settings)
