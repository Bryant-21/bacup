"""Texture tool workspaces — DDS, Image, Materials, Palettes."""

from ui.toolkit.workspaces.tool_workspace import ToolWorkspace
from ui.tools.dds.inspector import DDSInspectorTool
from ui.tools.dds.png_exporter import DDSPNGExporterTool
from ui.tools.dds.resizer import DDSResizerTool
from ui.tools.image.color_report import ColorReportTool
from ui.tools.image.image_utils import ImageUtilsTool
from ui.tools.image.upscaler import ImageUpscalerTool
from ui.tools.materials.bulk_copier import MaterialCopierTool
from ui.tools.palette.quantizer import ImageQuantizerTool


class DDSInspectorWorkspace(ToolWorkspace):
    name = "DDS Inspector"
    icon = "DDS"
    id = "dds_inspector"
    tool_class = DDSInspectorTool


class DDSPNGWorkspace(ToolWorkspace):
    name = "DDS to PNG"
    icon = "PNG"
    id = "dds_png"
    tool_class = DDSPNGExporterTool


class DDSResizerWorkspace(ToolWorkspace):
    name = "DDS Resizer"
    icon = "RSZ"
    id = "dds_resizer"
    tool_class = DDSResizerTool


class ColorReportWorkspace(ToolWorkspace):
    name = "Color Report"
    icon = "CLR"
    id = "color_report"
    tool_class = ColorReportTool


class ImageUtilsWorkspace(ToolWorkspace):
    name = "Image Utils"
    icon = "IMG"
    id = "image_utils"
    tool_class = ImageUtilsTool


class ImageUpscalerWorkspace(ToolWorkspace):
    name = "Image Upscaler"
    icon = "UPS"
    id = "img_upscaler"
    tool_class = ImageUpscalerTool


class MaterialCopierWorkspace(ToolWorkspace):
    name = "Material Copier"
    icon = "MCP"
    id = "mat_copier"
    tool_class = MaterialCopierTool


class ImageQuantizerWorkspace(ToolWorkspace):
    name = "Image Quantizer"
    icon = "QNT"
    id = "img_quantizer"
    tool_class = ImageQuantizerTool
