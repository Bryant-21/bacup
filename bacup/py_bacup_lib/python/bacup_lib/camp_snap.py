"""FO76 workshop snapping for Tales (CampSnap/<plugin>.json).

FO76 snaps CAMP pieces by template, not by NIF connect points. A base form's SNTP names its snap
template (STMP); each template ENAM places a snap node (STND) at a transform. Two nodes join when
their types are the same and the type self-snaps, or when either type lists the other in NNAM.

Evidence (SeventySix.esm): STND FNAM bit 0 is the runtime's self-snap test (`A == B && flags & 1`);
SnapNode_Floor01 and SnapNode_Floor01Sm_A carry it, their *_NoSelfSnap variants do not.
A child template (PNAM) inherits its parent's nodes: ONAM + TNAM moves the parent node with that
id, a bare ONAM drops it, and a child ENAM with the same id replaces it.
GNAM is the template's own offset from master-template space to its object's space: walls inherit
tile-edge nodes (y 128) from workshopSnap_WrhsMaster01 and shift them onto the wall with (0,-128,0).
Only leaf templates carry it (68 of 1142), so it applies after flattening and is not inherited.
"""
from __future__ import annotations

import math
import struct
from dataclasses import dataclass, field
from pathlib import Path

from bacup_lib.esp_native_runtime import load_esp_native
from bacup_lib.plugin_catalog import form_key, write_document
from bacup_lib.reputation_catalog import plugin_masters

FEATURE = "CampSnap"
VERSION = 1

# STND EditorID (lowercase) -> FO4 connect point name. P-WS-* are not snap partners; Tales decides.
FO4_ALIASES = {
    "defaultsnapnode_origin": "P-WS-Origin",
    "defaultsnapnode_rotation": "P-WS-Rotation",
    "defaultsnapnode_sinkmax": "P-WS-SinkMax",
    "snapnode_floor01": "P-Floor",
    "snapnode_floor01_nofoundationsnap": "P-Floor",
    "snapnode_floor01foundation": "P-Floor",
    "snapnode_floor01foundation_ramp": "P-Floor",
    "snapnode_floor01foundation_stairside": "P-Floor",
    "snapnode_stairswithturn_top": "P-Floor",
    "snapnode_floor01sm_a": "P-Balcony01-Dif",
    "snapnode_floor01sm_b": "P-Balcony01",
    "snapnode_roof01twinpeak": "P-WrhsRoofTwinPk01",
    "snapnode_roof01l": "P-WrhsRoof02-Dif",
    "snapnode_roof01r": "P-WrhsRoof02-Dif2",
}

Transform = tuple[tuple[float, float, float], tuple[float, float, float]]


@dataclass
class SnapNode:
    id: int
    type: int
    pos: tuple[float, float, float]
    rot: tuple[float, float, float]


@dataclass
class SnapTemplate:
    editor_id: str = ""
    parent: int | None = None
    category: int = 0
    offset: tuple[float, float, float] = (0.0, 0.0, 0.0)
    nodes: list[SnapNode] = field(default_factory=list)
    # parent node id -> new transform, or None to drop the node
    overrides: dict[int, Transform | None] = field(default_factory=dict)


def _u32(data: bytes, offset: int = 0) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def _finite(values) -> bool:
    return all(math.isfinite(value) for value in values)


def parse_transform(data: bytes, offset: int = 0) -> Transform | None:
    """x,y,z,rx,ry,rz (24 bytes) or x,y,z,rz (16 bytes, the older form)."""
    remaining = len(data) - offset
    if remaining >= 24:
        values = struct.unpack_from("<6f", data, offset)
        pos, rot = values[:3], values[3:]
    elif remaining >= 16:
        x, y, z, rz = struct.unpack_from("<4f", data, offset)
        pos, rot = (x, y, z), (0.0, 0.0, rz)
    else:
        return None
    if not _finite(pos + rot):
        return None
    return pos, rot


def parse_enam(data: bytes) -> SnapNode | None:
    """id, STND, transform; 36-byte entries (form v187+) end in a float Tales does not use."""
    if len(data) < 24:
        return None
    transform = parse_transform(data, 8)
    if transform is None:
        return None
    return SnapNode(_u32(data), _u32(data, 4), *transform)


def parse_template(subrecords) -> SnapTemplate:
    """subrecords: [(signature, bytes)] in record order."""
    template = SnapTemplate()
    pending: int | None = None
    for signature, data in subrecords:
        if signature == "TNAM" and pending is not None:
            template.overrides[pending] = parse_transform(data)
            pending = None
            continue
        if pending is not None:
            template.overrides[pending] = None
            pending = None
        if signature == "EDID":
            template.editor_id = data.split(b"\0", 1)[0].decode("utf-8", "replace")
        elif signature == "PNAM" and len(data) >= 4:
            template.parent = _u32(data)
        elif signature == "STPT" and len(data) >= 4:
            template.category = _u32(data)
        elif signature == "GNAM" and len(data) >= 12 and _finite(offset := struct.unpack_from("<3f", data)):
            template.offset = offset
        elif signature == "ENAM":
            node = parse_enam(data)
            if node is not None:
                template.nodes.append(node)
        elif signature == "ONAM" and len(data) >= 4:
            pending = _u32(data)
    if pending is not None:
        template.overrides[pending] = None
    return template


def flatten(key: int, templates: dict[int, SnapTemplate], cache: dict[int, list[SnapNode]] | None = None,
            visiting: set[int] | None = None) -> list[SnapNode]:
    cache = {} if cache is None else cache
    visiting = set() if visiting is None else visiting
    if key in cache:
        return cache[key]
    template = templates.get(key)
    if template is None or key in visiting:
        return []
    visiting.add(key)
    nodes = []
    if template.parent is not None:
        for node in flatten(template.parent, templates, cache, visiting):
            if node.id not in template.overrides:
                nodes.append(node)
            elif (transform := template.overrides[node.id]) is not None:
                nodes.append(SnapNode(node.id, node.type, *transform))
    by_id = {node.id: index for index, node in enumerate(nodes)}
    for node in template.nodes:
        if node.id in by_id:
            nodes[by_id[node.id]] = node
        else:
            by_id[node.id] = len(nodes)
            nodes.append(node)
    visiting.discard(key)
    cache[key] = nodes
    return nodes


def parse_node_type(subrecords) -> dict:
    """STND -> editor id, self-snap flag (FNAM bit 0) and raw NNAM adjacency."""
    editor_id, flags, adjacent = "", 0, []
    for signature, data in subrecords:
        if signature == "EDID":
            editor_id = data.split(b"\0", 1)[0].decode("utf-8", "replace")
        elif signature == "FNAM" and len(data) >= 4:
            flags = _u32(data)
        elif signature == "NNAM" and len(data) >= 4:
            adjacent.append(_u32(data))
    return {"editor_id": editor_id, "self_snap": bool(flags & 1), "adjacent": adjacent}


def short_float(value: float) -> float:
    """Shortest decimal that reads back as the same f32, so the JSON shows 64.0, not 63.99999..."""
    if value == 0:
        return 0.0
    target = struct.pack("<f", value)
    for digits in range(1, 10):
        candidate = float(f"{value:.{digits}g}")
        if struct.pack("<f", candidate) == target:
            return candidate
    return value


def build_document(node_types: dict[int, dict], templates: dict[int, SnapTemplate], form_templates: dict[int, int],
                   key) -> dict:
    """key(raw form id) -> FormKey. form_templates: base form raw id -> its SNTP STMP raw id."""
    cache: dict[int, list[SnapNode]] = {}
    flattened = {raw: flatten(raw, templates, cache) for raw in templates}
    document_templates = {}
    for raw in sorted(templates, key=key):
        offset = templates[raw].offset
        document_templates[key(raw)] = {
            "editor_id": templates[raw].editor_id,
            "category": templates[raw].category,
            "nodes": [{"id": node.id, "type": key(node.type),
                       "pos": [short_float(v + o) for v, o in zip(node.pos, offset)],
                       "rot": [short_float(v) for v in node.rot]} for node in flattened[raw]],
        }
    forms = {key(form): key(template) for form, template in form_templates.items() if flattened.get(template)}
    aliases: dict[str, list[str]] = {}
    for raw in sorted(node_types, key=key):
        name = FO4_ALIASES.get(node_types[raw]["editor_id"].lower())
        if name:
            aliases.setdefault(name, []).append(key(raw))
    return {
        "version": VERSION,
        "settings": {},
        "node_types": {key(raw): {"editor_id": node["editor_id"], "self_snap": node["self_snap"],
                                  "adjacent": [key(other) for other in node["adjacent"]]}
                       for raw, node in sorted(node_types.items(), key=lambda item: key(item[0]))},
        "templates": document_templates,
        "forms": dict(sorted(forms.items())),
        "fo4_aliases": dict(sorted(aliases.items())),
    }


def key_function(plugin: str, masters: list[str]):
    def key(raw: int) -> str:
        index = raw >> 24
        if index > len(masters):
            raise ValueError(f"Invalid master index {index} for {plugin}:{raw:08X}")
        return form_key(masters[index] if index < len(masters) else plugin, raw & 0xFFFFFF)
    return key


def read_source(native, handle: int) -> tuple[dict[int, dict], dict[int, SnapTemplate], dict[int, int]]:
    def subrecords(raw):
        return [(signature, bytes(data)) for signature, data, _ in native.plugin_handle_record_subrecords(handle, raw) or []]

    def raw_ids(signature):
        return [row[4] for row in native.plugin_handle_record_index_rows(handle, signatures=[signature], form_keys=None)]

    node_types = {raw: parse_node_type(subrecords(raw)) for raw in raw_ids("STND")}
    templates = {raw: parse_template(subrecords(raw)) for raw in raw_ids("STMP")}
    forms = {}
    for raw in native.plugin_handle_record_form_ids_with_subrecords(handle, ["SNTP"]):
        sntp = next((data for signature, data in subrecords(raw) if signature == "SNTP" and len(data) >= 4), None)
        if sntp is not None:
            forms[raw] = _u32(sntp)
    return node_types, templates, forms


def emit_camp_snap(source_plugin: Path, output_mod: Path) -> dict:
    source_plugin = Path(source_plugin)
    native = load_esp_native()
    handle = native.plugin_handle_load_index(str(source_plugin), game="fo76")
    try:
        node_types, templates, forms = read_source(native, handle)
        key = key_function(source_plugin.name, plugin_masters(native, handle))
    finally:
        native.plugin_handle_close(handle)
    document = build_document(node_types, templates, forms, key)
    path = write_document(FEATURE, source_plugin.name, document, output_mod)
    return {"path": path, "node_types": len(document["node_types"]), "templates": len(document["templates"]),
            "forms": len(document["forms"]), "skipped_forms": len(forms) - len(document["forms"])}
