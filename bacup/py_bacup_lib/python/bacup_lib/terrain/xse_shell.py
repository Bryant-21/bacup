from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path

FO4_CELL_SIZE = 4096.0
DEFAULT_FIRST_FORM_ID = 0x000800


@dataclass(frozen=True, slots=True)
class XseShellRequest:
    mod_dir: Path
    plugin_name: str
    worldspace_editor_id: str
    cell_min_x: int
    cell_min_y: int
    cell_max_x: int
    cell_max_y: int
    first_form_id: int = DEFAULT_FIRST_FORM_ID


@dataclass(frozen=True, slots=True)
class XseShellResult:
    plugin_yaml: Path
    records_dir: Path
    cell_count: int


def write_xse_shell(request: XseShellRequest) -> XseShellResult:
    if request.cell_max_x < request.cell_min_x or request.cell_max_y < request.cell_min_y:
        raise ValueError("cell max coordinates must be greater than or equal to cell min coordinates")

    yaml_dir = request.mod_dir / "yaml"
    records_dir = yaml_dir / "records"
    world_form_id = request.first_form_id
    cell_count = (request.cell_max_x - request.cell_min_x + 1) * (
        request.cell_max_y - request.cell_min_y + 1
    )
    next_object_id = request.first_form_id + 1 + cell_count

    request.mod_dir.mkdir(parents=True, exist_ok=True)
    records_dir.mkdir(parents=True, exist_ok=True)
    (request.mod_dir / ".game").write_text("fo4\n", encoding="utf-8")

    plugin_yaml = yaml_dir / "plugin.yaml"
    plugin_yaml.write_text(
        _plugin_yaml(request.plugin_name, next_object_id),
        encoding="utf-8",
    )

    world_dir = records_dir / "WRLD" / (
        f"{request.worldspace_editor_id} - {form_id_hex(world_form_id)}_{request.plugin_name}"
    )
    world_dir.mkdir(parents=True, exist_ok=True)
    (world_dir / "RecordData.yaml").write_text(
        _world_yaml(request, world_form_id),
        encoding="utf-8",
    )

    index = 0
    for cell_y in range(request.cell_min_y, request.cell_max_y + 1):
        for cell_x in range(request.cell_min_x, request.cell_max_x + 1):
            cell_dir = (
                world_dir
                / f"{floor_div(cell_x, 32)}, {floor_div(cell_y, 32)}"
                / f"{floor_div(cell_x, 8)}, {floor_div(cell_y, 8)}"
                / f"{cell_x}, {cell_y}"
            )
            cell_dir.mkdir(parents=True, exist_ok=True)
            cell_form_id = request.first_form_id + 1 + index
            (cell_dir / "RecordData.yaml").write_text(
                _cell_yaml(request, cell_x, cell_y, cell_form_id),
                encoding="utf-8",
            )
            index += 1

    return XseShellResult(
        plugin_yaml=plugin_yaml,
        records_dir=records_dir,
        cell_count=cell_count,
    )


def _plugin_yaml(plugin_name: str, next_object_id: int) -> str:
    return (
        f"format_version: 1\n"
        f"plugin: {plugin_name}\n"
        f"game: fo4\n"
        f"header_size: 24\n"
        f"header:\n"
        f"  version: 1.0\n"
        f"  num_records: 0\n"
        f"  next_object_id: '{form_id_hex(next_object_id)}'\n"
        f"  author: ''\n"
        f"  description: ''\n"
        f"  masters:\n"
        f"    - Fallout4.esm\n"
        f"  master_sizes:\n"
        f"    - 0\n"
        f"  overridden_forms: []\n"
        f"  flags: []\n"
        f"  version_control: 0\n"
        f"  extra_subrecords: []\n"
    )


def _world_yaml(request: XseShellRequest, form_id: int) -> str:
    min_x_world = request.cell_min_x * FO4_CELL_SIZE
    min_y_world = request.cell_min_y * FO4_CELL_SIZE
    max_x_world = (request.cell_max_x + 1) * FO4_CELL_SIZE
    max_y_world = (request.cell_max_y + 1) * FO4_CELL_SIZE
    return (
        f"signature: WRLD\n"
        f"form_id: \"{form_id_hex(form_id)}:{request.plugin_name}\"\n"
        f"form_version: 131\n"
        f"version2: 1\n"
        f"eid: {request.worldspace_editor_id}\n"
        f"subrecords:\n"
        f"  - signature: EDID\n"
        f"    data_hex: \"{zstring_hex(request.worldspace_editor_id)}\"\n"
        f"  - signature: NAMA\n"
        f"    data_hex: \"{f32_hex(1.0)}\"\n"
        f"  - signature: DATA\n"
        f"    data_hex: \"00\"\n"
        f"  - signature: NAM0\n"
        f"    data_hex: \"{f32_hex(min_x_world)}{f32_hex(min_y_world)}\"\n"
        f"  - signature: NAM9\n"
        f"    data_hex: \"{f32_hex(max_x_world)}{f32_hex(max_y_world)}\"\n"
    )


def _cell_yaml(request: XseShellRequest, cell_x: int, cell_y: int, form_id: int) -> str:
    cell_eid = cell_editor_id(request.worldspace_editor_id, cell_x, cell_y)
    xclc_hex = f"{i32_hex(cell_x)}{i32_hex(cell_y)}009ED6AB"
    return (
        f"signature: CELL\n"
        f"form_id: \"{form_id_hex(form_id)}:{request.plugin_name}\"\n"
        f"form_version: 131\n"
        f"version2: 1\n"
        f"eid: {cell_eid}\n"
        f"subrecords:\n"
        f"  - signature: EDID\n"
        f"    data_hex: \"{zstring_hex(cell_eid)}\"\n"
        f"  - signature: DATA\n"
        f"    data_hex: \"{u16_hex(2)}\"\n"
        f"  - signature: XCLC\n"
        f"    data_hex: \"{xclc_hex}\"\n"
        f"  - signature: XCLW\n"
        f"    data_hex: \"FFFF7F7F\"\n"
    )


def cell_editor_id(worldspace_editor_id: str, cell_x: int, cell_y: int) -> str:
    # CK strips '_' from CELL EditorIDs on first save — emit the post-strip
    # form up front so local YAML/ESP and deployed ESP agree. CELL-specific.
    return f"{worldspace_editor_id}CellX{coordinate_token(cell_x)}Y{coordinate_token(cell_y)}"


def coordinate_token(value: int) -> str:
    sign = "N" if value < 0 else "P"
    return f"{sign}{abs(value):03}"


def zstring_hex(value: str) -> str:
    return bytes_hex(value.encode("utf-8") + b"\0")


def bytes_hex(value: bytes) -> str:
    return value.hex().upper()


def u16_hex(value: int) -> str:
    return bytes_hex(struct.pack("<H", value))


def i32_hex(value: int) -> str:
    return bytes_hex(struct.pack("<i", value))


def f32_hex(value: float) -> str:
    return bytes_hex(struct.pack("<f", value))


def form_id_hex(value: int) -> str:
    return f"{value & 0x00FF_FFFF:06X}"


def floor_div(value: int, divisor: int) -> int:
    quotient = value // divisor
    return quotient
