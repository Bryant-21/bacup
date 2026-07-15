"""DDS Inspector tool — view DDS file properties and format info."""

from __future__ import annotations

import logging
import os
import struct

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, pick_file

_log = logging.getLogger("tools.dds_inspector")

# Common DXGI format names
DXGI_NAMES = {
    0: "UNKNOWN",
    71: "BC1_UNORM", 72: "BC1_UNORM_SRGB",
    74: "BC2_UNORM", 75: "BC2_UNORM_SRGB",
    77: "BC3_UNORM", 78: "BC3_UNORM_SRGB",
    80: "BC4_UNORM", 81: "BC4_SNORM",
    83: "BC5_UNORM", 84: "BC5_SNORM",
    95: "BC6H_UF16", 96: "BC6H_SF16",
    98: "BC7_UNORM", 99: "BC7_UNORM_SRGB",
    28: "R8G8B8A8_UNORM", 29: "R8G8B8A8_UNORM_SRGB",
    87: "B8G8R8A8_UNORM",
    61: "R8_UNORM",
}

# DDS pixel format fourCC -> name
FOURCC_NAMES = {
    b"DXT1": "DXT1 (BC1)",
    b"DXT2": "DXT2",
    b"DXT3": "DXT3 (BC2)",
    b"DXT4": "DXT4",
    b"DXT5": "DXT5 (BC3)",
    b"ATI1": "ATI1 (BC4)",
    b"ATI2": "ATI2 (BC5)",
    b"BC4U": "BC4_UNORM",
    b"BC4S": "BC4_SNORM",
    b"BC5U": "BC5_UNORM",
    b"BC5S": "BC5_SNORM",
    b"DX10": "DX10 (extended header)",
}


def _read_dds_info(path: str) -> dict:
    """Read DDS header and return info dict."""
    try:
        from creation_lib.dds import native_runtime

        native_info = native_runtime.texdiag_info(path)
        if native_info is not None:
            return {
                "file": os.path.basename(path),
                "size_bytes": int(native_info["file_size"]),
                "height": int(native_info["height"]),
                "width": int(native_info["width"]),
                "depth": int(native_info["depth"]),
                "mip_count": int(native_info["mip_levels"]),
                "dxgi_format": int(native_info["dxgi_format"]),
                "dxgi_name": str(native_info["format"]),
                "format": str(native_info["format"]),
                "dimension": str(native_info["dimension"]),
                "alpha_mode": str(native_info["alpha_mode"]),
                "bits_per_pixel": int(native_info["bits_per_pixel"]),
                "bits_per_color": int(native_info["bits_per_color"]),
                "image_count": int(native_info["image_count"]),
                "is_compressed": bool(native_info["is_compressed"]),
                "is_cubemap": bool(native_info["is_cubemap"]),
            }
    except Exception as exc:
        _log.debug("native texdiag info failed for %s: %s", path, exc)

    info = {"file": os.path.basename(path), "size_bytes": os.path.getsize(path)}

    with open(path, "rb") as f:
        header = f.read(148)

    if len(header) < 128 or header[:4] != b"DDS ":
        info["error"] = "Not a valid DDS file"
        return info

    info["height"] = struct.unpack_from("<I", header, 12)[0]
    info["width"] = struct.unpack_from("<I", header, 16)[0]
    info["pitch_or_linear"] = struct.unpack_from("<I", header, 20)[0]
    info["depth"] = struct.unpack_from("<I", header, 24)[0]
    info["mip_count"] = struct.unpack_from("<I", header, 28)[0]

    # Pixel format
    pf_flags = struct.unpack_from("<I", header, 80)[0]
    fourcc = header[84:88]
    info["fourcc"] = fourcc.decode("ascii", errors="replace")

    if fourcc in FOURCC_NAMES:
        info["format"] = FOURCC_NAMES[fourcc]
    else:
        info["format"] = f"fourCC={info['fourcc']}"

    # DX10 extended header
    if fourcc == b"DX10" and len(header) >= 148:
        dxgi_fmt = struct.unpack_from("<I", header, 128)[0]
        info["dxgi_format"] = dxgi_fmt
        info["dxgi_name"] = DXGI_NAMES.get(dxgi_fmt, f"DXGI_{dxgi_fmt}")
        info["format"] = info["dxgi_name"]

    return info


class DDSInspectorTool(BaseTool):
    name = "DDS Inspector"
    tool_id = "dds_inspector"
    description = "View DDS file properties"
    category = "DDS"

    def __init__(self):
        super().__init__()
        self._dds_path = ""
        self._info: dict | None = None

    def draw_content(self) -> None:
        if begin_form("##dds_inspector"):
            _, clicked = draw_path_row("DDS File", self._dds_path)
            if clicked:
                path = pick_file("Select DDS file", [("DDS", "*.dds"), ("All", "*.*")])
                if path:
                    self._dds_path = path
                    self._refresh()
            end_form()

        if imgui.button("Refresh"):
            self._refresh()

        imgui.separator()

        if self._info:
            info = self._info
            if "error" in info:
                imgui.text_colored(imgui.ImVec4(1, 0.3, 0.3, 1), info["error"])
                return

            imgui.text(f"File: {info.get('file', '?')}")
            imgui.text(f"Size: {info.get('width', 0)} x {info.get('height', 0)}")
            imgui.text(f"Mipmaps: {info.get('mip_count', 0)}")
            imgui.text(f"Format: {info.get('format', '?')}")
            if "dxgi_format" in info:
                imgui.text(f"DXGI: {info.get('dxgi_name', '?')} ({info['dxgi_format']})")
            if "dimension" in info:
                imgui.text(f"Dimension: {info.get('dimension', '?')}")
            if "alpha_mode" in info:
                imgui.text(f"Alpha: {info.get('alpha_mode', '?')}")
            if "bits_per_pixel" in info:
                imgui.text(f"BPP: {info.get('bits_per_pixel', 0)}")
            imgui.text(f"File size: {info.get('size_bytes', 0):,} bytes")
            if info.get("depth", 0) > 1:
                imgui.text(f"Depth: {info['depth']}")

        else:
            imgui.text_disabled("Select a DDS file to inspect.")

    def _refresh(self):
        if not self._dds_path or not os.path.isfile(self._dds_path):
            self._info = None
            return

        self._info = _read_dds_info(self._dds_path)
