"""Header view for the Properties panel.

Renders the NIF file header (NifHeader) when the NIF filename row is
selected in the scene tree. A small subset of fields is editable
(creator, export_info); the rest are read-only because they're derived
from the block list and editing them by hand would desync the file.
"""
from __future__ import annotations

import copy
import logging
import re
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from imgui_bundle import imgui

from creation_lib.nif.actions import NifAction, OperationResult

_log = logging.getLogger("nif_editor.properties_header")

HEADER_BLOCK_ID = -1
_LOD_TILE_RE = re.compile(
    r"^(?P<world>.+)\.(?P<level>-?\d+)\.(?P<x>-?\d+)\.(?P<y>-?\d+)"
    r"(?P<season>(?:\.[^.]+)*)\.(?P<kind>bto|btr)$",
    re.IGNORECASE,
)
_LOD_FILE_KINDS = {
    ".bto": "Object LOD (.bto)",
    ".btr": "Terrain LOD (.btr)",
}
_LOD_SHAPE_TYPES = {"BSTriShape", "BSSubIndexTriShape", "BSMeshLODTriShape"}


@dataclass
class SetHeaderFieldAction(NifAction):
    """Set a single field on NifFile.header, with undo support."""
    field_name: str
    old_value: Any
    new_value: Any
    _description: str = ""

    def __post_init__(self):
        self.old_value = copy.deepcopy(self.old_value)
        self.new_value = copy.deepcopy(self.new_value)
        if not self._description:
            self._description = f"Set header.{self.field_name}"

    def execute(self, nif) -> OperationResult:
        setattr(nif.header, self.field_name, copy.deepcopy(self.new_value))
        return OperationResult(True, self._description)

    def undo(self, nif) -> OperationResult:
        setattr(nif.header, self.field_name, copy.deepcopy(self.old_value))
        return OperationResult(True, f"Undo: {self._description}")

    def description(self) -> str:
        return self._description


def _endian_label(value: int) -> str:
    if value == 1:
        return "Little-endian (1)"
    if value == 0:
        return "Big-endian (0)"
    return f"Unknown ({value})"


def _as_int(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return None


def _value_len(value: Any) -> int | None:
    return len(value) if isinstance(value, list) else None


def _format_string(value: Any) -> str:
    if isinstance(value, list):
        return "".join(str(part) for part in value)
    if value is None:
        return ""
    return str(value)


def _format_vec3(value: Any) -> str | None:
    if isinstance(value, dict):
        coords = [value.get("x"), value.get("y"), value.get("z")]
    elif isinstance(value, (list, tuple)) and len(value) >= 3:
        coords = list(value[:3])
    else:
        return None
    if any(coord is None for coord in coords):
        return None
    try:
        return "({:.2f}, {:.2f}, {:.2f})".format(*(float(coord) for coord in coords))
    except (TypeError, ValueError):
        return None


def _parse_lod_tile_name(file_path: str) -> dict[str, Any] | None:
    match = _LOD_TILE_RE.match(Path(file_path).name)
    if match is None:
        return None
    season = match.group("season") or ""
    return {
        "world": match.group("world"),
        "level": int(match.group("level")),
        "x": int(match.group("x")),
        "y": int(match.group("y")),
        "season": season if match.group("kind").lower() == "bto" else "",
    }


def _shape_summary(block) -> dict[str, Any]:
    vertex_desc = _as_int(block.get_field("Vertex Desc"))
    summary = {
        "block_id": block.block_id,
        "type": block.type_name,
        "name": _format_string(block.get_field("Name")),
        "vertices": _as_int(block.get_field("Num Vertices"))
        or _value_len(block.get_field("Vertex Data"))
        or 0,
        "triangles": _as_int(block.get_field("Num Triangles"))
        or _value_len(block.get_field("Triangles"))
        or 0,
        "segments": _as_int(block.get_field("Num Segments"))
        or _value_len(block.get_field("Segment"))
        or 0,
        "vertex_desc": vertex_desc,
        "translation": _format_vec3(block.get_field("Translation")),
        "scale": block.get_field("Scale"),
        "shader_property": _as_int(block.get_field("Shader Property")),
        "alpha_property": _as_int(block.get_field("Alpha Property")),
    }
    return summary


def _shape_summary_line(summary: dict[str, Any]) -> str:
    label = f"[{summary['block_id']}] {summary['type']}"
    if summary["name"]:
        label += f" {summary['name']!r}"
    parts = [
        f"verts={summary['vertices']}",
        f"tris={summary['triangles']}",
    ]
    if summary["segments"]:
        parts.append(f"segments={summary['segments']}")
    if summary["vertex_desc"] is not None:
        parts.append(f"vertex_desc=0x{summary['vertex_desc']:X}")
    if summary["translation"]:
        parts.append(f"translation={summary['translation']}")
    if summary["scale"] is not None:
        parts.append(f"scale={summary['scale']}")
    if summary["shader_property"] is not None and summary["shader_property"] >= 0:
        parts.append(f"shader={summary['shader_property']}")
    if summary["alpha_property"] is not None and summary["alpha_property"] >= 0:
        parts.append(f"alpha={summary['alpha_property']}")
    return f"{label}: {', '.join(parts)}"


def _collect_texture_paths(nif) -> list[str]:
    textures: set[str] = set()
    for block in nif.blocks:
        if block.type_name != "BSShaderTextureSet":
            continue
        for texture in block.get_field("Textures") or []:
            text = _format_string(texture)
            if text:
                textures.add(text)
    return sorted(textures, key=str.lower)


def _collect_bto_btr_diagnostics(nif, file_path: str) -> dict[str, Any] | None:
    suffix = Path(file_path).suffix.lower()
    kind = _LOD_FILE_KINDS.get(suffix)
    if kind is None:
        return None

    type_counts = Counter(block.type_name for block in nif.blocks)
    fields_by_type: dict[str, set[str]] = defaultdict(set)
    remainders = []
    shape_summaries = []
    for block in nif.blocks:
        for field_name, _value in block.fields:
            fields_by_type[block.type_name].add(field_name)
        remainder = getattr(block, "_remainder", b"")
        if remainder:
            remainders.append(
                {
                    "block_id": block.block_id,
                    "type": block.type_name,
                    "bytes": len(remainder),
                }
            )
        if block.type_name in _LOD_SHAPE_TYPES:
            shape_summaries.append(_shape_summary(block))

    totals = {
        "vertices": sum(summary["vertices"] for summary in shape_summaries),
        "triangles": sum(summary["triangles"] for summary in shape_summaries),
        "segments": sum(summary["segments"] for summary in shape_summaries),
    }

    return {
        "kind": kind,
        "tile": _parse_lod_tile_name(file_path),
        "block_type_counts": sorted(
            type_counts.items(), key=lambda item: item[0].lower()
        ),
        "fields_by_type": [
            (type_name, sorted(fields, key=str.lower))
            for type_name, fields in sorted(
                fields_by_type.items(), key=lambda item: item[0].lower()
            )
        ],
        "shape_summaries": shape_summaries,
        "shape_lines": [_shape_summary_line(summary) for summary in shape_summaries],
        "texture_paths": _collect_texture_paths(nif),
        "remainders": remainders,
        "totals": totals,
    }


def _push_header_action(app, nif_id: str, action: SetHeaderFieldAction) -> None:
    """Execute action and push onto the undo stack."""
    session = app.registry.get_session(nif_id)
    action.execute(session.nif)
    app.undo_manager.push(nif_id, action)


def _draw_lines_child(name: str, lines: list[str], height: int = 180) -> None:
    imgui.begin_child(name, imgui.ImVec2(0, height), imgui.ChildFlags_.borders.value)
    for line in lines:
        imgui.text(line)
    imgui.end_child()


def _draw_bto_btr_diagnostics(nif, file_path: str) -> None:
    diagnostics = _collect_bto_btr_diagnostics(nif, file_path)
    if diagnostics is None:
        return

    imgui.separator()
    imgui.text_colored(imgui.ImVec4(0.7, 0.7, 0.9, 1.0), "BTO/BTR Read Diagnostics")
    imgui.separator()

    imgui.text(f"LOD file kind: {diagnostics['kind']}")
    tile = diagnostics["tile"]
    if tile:
        imgui.text(
            f"Tile: world={tile['world']} level={tile['level']} "
            f"x={tile['x']} y={tile['y']}"
        )
        if tile["season"]:
            imgui.text(f"Season suffix: {tile['season']}")

    totals = diagnostics["totals"]
    imgui.text(
        f"Shape totals: {len(diagnostics['shape_summaries'])} shapes, "
        f"{totals['vertices']} verts, {totals['triangles']} tris, "
        f"{totals['segments']} segments"
    )
    if diagnostics["remainders"]:
        rem_bytes = sum(item["bytes"] for item in diagnostics["remainders"])
        imgui.text_colored(
            imgui.ImVec4(0.9, 0.7, 0.3, 1.0),
            f"Unread remainder bytes: {rem_bytes} across {len(diagnostics['remainders'])} blocks",
        )
    else:
        imgui.text("Unread remainder bytes: 0")

    if diagnostics["shape_lines"] and imgui.tree_node(
        f"LOD Shape Readout [{len(diagnostics['shape_lines'])}]"
    ):
        _draw_lines_child("lod_shape_readout", diagnostics["shape_lines"], 180)
        imgui.tree_pop()

    if diagnostics["texture_paths"] and imgui.tree_node(
        f"Texture Paths [{len(diagnostics['texture_paths'])}]"
    ):
        _draw_lines_child("lod_texture_paths", diagnostics["texture_paths"], 160)
        imgui.tree_pop()

    if diagnostics["block_type_counts"] and imgui.tree_node(
        f"Block Type Counts [{len(diagnostics['block_type_counts'])}]"
    ):
        lines = [
            f"{type_name}: {count}"
            for type_name, count in diagnostics["block_type_counts"]
        ]
        _draw_lines_child("lod_block_type_counts", lines, 160)
        imgui.tree_pop()

    if diagnostics["fields_by_type"] and imgui.tree_node(
        f"Unique Read Fields by Block Type [{len(diagnostics['fields_by_type'])}]"
    ):
        lines = [
            f"{type_name}: {', '.join(fields)}"
            for type_name, fields in diagnostics["fields_by_type"]
        ]
        _draw_lines_child("lod_unique_read_fields", lines, 260)
        imgui.tree_pop()

    if diagnostics["remainders"] and imgui.tree_node(
        f"Unread Remainders [{len(diagnostics['remainders'])}]"
    ):
        lines = [
            f"[{item['block_id']}] {item['type']}: {item['bytes']} bytes"
            for item in diagnostics["remainders"]
        ]
        _draw_lines_child("lod_unread_remainders", lines, 140)
        imgui.tree_pop()


def draw_header_props(app, nif_id: str) -> None:
    """Render the NIF header in the properties panel."""
    try:
        session = app.registry.get_session(nif_id)
    except KeyError:
        imgui.text_colored(imgui.ImVec4(0.8, 0.3, 0.3, 1.0),
                           f"No session: {nif_id}")
        return

    nif = session.nif
    if nif is None:
        imgui.text_colored(imgui.ImVec4(0.5, 0.5, 0.5, 1.0), "No NIF loaded")
        return

    h = nif.header

    imgui.text_colored(imgui.ImVec4(0.9, 0.8, 0.5, 1.0), "NIF Header")
    imgui.same_line()
    imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0), f"({nif_id})")
    imgui.separator()

    imgui.begin_child("header_scroll", imgui.ImVec2(0, 0))

    # Editable
    imgui.text_colored(imgui.ImVec4(0.7, 0.9, 0.7, 1.0), "Editable")
    imgui.separator()

    changed, new_creator = imgui.input_text("Creator", h.creator or "")
    if changed:
        _push_header_action(
            app, nif_id,
            SetHeaderFieldAction(field_name="creator",
                                 old_value=h.creator, new_value=new_creator),
        )

    if imgui.tree_node(f"Export Info [{len(h.export_info)}]"):
        remove_idx: int | None = None
        for i, line in enumerate(h.export_info):
            imgui.push_id(f"export_info_{i}")
            changed_i, new_line = imgui.input_text(f"[{i}]", line or "")
            if changed_i:
                new_list = list(h.export_info)
                new_list[i] = new_line
                _push_header_action(
                    app, nif_id,
                    SetHeaderFieldAction(field_name="export_info",
                                         old_value=h.export_info,
                                         new_value=new_list),
                )
            imgui.same_line()
            if imgui.small_button("X"):
                remove_idx = i
            imgui.pop_id()
        if remove_idx is not None:
            new_list = list(h.export_info)
            del new_list[remove_idx]
            _push_header_action(
                app, nif_id,
                SetHeaderFieldAction(field_name="export_info",
                                     old_value=h.export_info,
                                     new_value=new_list),
            )
        if imgui.small_button("+ Add line"):
            new_list = list(h.export_info) + [""]
            _push_header_action(
                app, nif_id,
                SetHeaderFieldAction(field_name="export_info",
                                     old_value=h.export_info,
                                     new_value=new_list),
            )
        imgui.tree_pop()

    # Read-only info
    imgui.separator()
    imgui.text_colored(imgui.ImVec4(0.7, 0.7, 0.9, 1.0), "Version / Format")
    imgui.separator()

    imgui.text(f"Header string: {h.header_string}")
    version_str = ".".join(str(v) for v in (h.version or ()))
    imgui.text(f"Version: {version_str}")
    imgui.text(f"Version packed: 0x{h.version_packed:08X}")
    imgui.text(f"User version: {h.user_version}")
    imgui.text(f"BS version: {h.bs_version}")
    imgui.text(f"Endian: {_endian_label(h.endian_type)}")
    if session.game_profile is not None:
        imgui.text(f"Detected game: {session.game_profile.display_name}")

    imgui.separator()
    imgui.text_colored(imgui.ImVec4(0.7, 0.7, 0.9, 1.0), "Derived (read-only)")
    imgui.separator()

    imgui.text(f"Block count: {len(nif.blocks)}")
    imgui.text(f"Block type names: {len(h.block_type_names)}")
    imgui.text(f"String table: {len(h.strings)} entries")
    imgui.text(f"Max string length: {h.max_string_length}")
    imgui.text(f"Groups: {len(h.groups)}")
    if h.sf_export_data:
        imgui.text(f"Starfield export data: {len(h.sf_export_data)} bytes")

    _draw_bto_btr_diagnostics(nif, session.file_path)

    if h.strings and imgui.tree_node(f"Strings [{len(h.strings)}]"):
        imgui.begin_child("strings_scroll",
                          imgui.ImVec2(0, 200),
                          imgui.ChildFlags_.borders.value)
        for i, s in enumerate(h.strings):
            imgui.text(f"[{i}] {s}")
        imgui.end_child()
        imgui.tree_pop()

    if h.block_type_names and imgui.tree_node(
        f"Block Type Names [{len(h.block_type_names)}]"
    ):
        imgui.begin_child("btn_scroll",
                          imgui.ImVec2(0, 200),
                          imgui.ChildFlags_.borders.value)
        for i, name in enumerate(h.block_type_names):
            imgui.text(f"[{i}] {name}")
        imgui.end_child()
        imgui.tree_pop()

    imgui.end_child()
