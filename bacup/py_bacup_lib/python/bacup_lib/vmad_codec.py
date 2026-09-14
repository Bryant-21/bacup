"""Boundary-only reader for the FO4 ``VMAD`` subrecord.

Just enough of the format to find where each entry in the top-level ``Scripts``
array starts and ends, so a single failed script binding can be dropped without
rewriting the rest of the subrecord. Property *values* are walked for their
length only — never decoded — and the record-type fragment block that follows
the array is carried as opaque bytes.

Layouts follow xEdit's FO4 definitions, the same source the native ESP decoder
in ``py_creation_lib/native/esp/src/authoring_serialize.rs`` reads.
"""

from __future__ import annotations

import struct
from collections.abc import Sequence
from dataclasses import dataclass

_HEADER = struct.Struct("<hhH")

# Property payload sizes that do not depend on the data, keyed by xEdit's
# wbPropTypeEnum. Types 0 (None) and 6 (Variable) carry no bytes at all.
_FIXED_VALUE_SIZES = {0: 0, 1: 8, 3: 4, 4: 4, 5: 1, 6: 0}
# `<i32 count>` followed by count elements of this many bytes.
_ARRAY_ELEMENT_SIZES = {11: 8, 13: 4, 14: 4, 15: 1}


class VmadFormatError(ValueError):
    """The bytes do not match the layout this reader models."""


@dataclass(frozen=True, slots=True)
class ScriptEntry:
    name: str
    raw: bytes


@dataclass(frozen=True, slots=True)
class Vmad:
    version: int
    object_format: int
    scripts: tuple[ScriptEntry, ...]
    fragments: bytes

    def with_scripts(self, scripts: tuple[ScriptEntry, ...]) -> bytes:
        return (
            _HEADER.pack(self.version, self.object_format, len(scripts))
            + b"".join(entry.raw for entry in scripts)
            + self.fragments
        )


def parse(data: bytes) -> Vmad:
    """Split a VMAD into its header, top-level scripts, and fragment tail."""
    if len(data) < _HEADER.size:
        raise VmadFormatError("VMAD shorter than its header")
    version, object_format, script_count = _HEADER.unpack_from(data, 0)
    if object_format not in (1, 2):
        raise VmadFormatError(f"unsupported VMAD object format {object_format}")
    offset = _HEADER.size
    scripts: list[ScriptEntry] = []
    for _ in range(script_count):
        start = offset
        name, offset = _read_wstring(data, offset)
        offset = _skip_script_body(data, offset)
        scripts.append(ScriptEntry(name, data[start:offset]))
    return Vmad(version, object_format, tuple(scripts), data[offset:])


def _read_wstring(data: bytes, offset: int) -> tuple[str, int]:
    if offset + 2 > len(data):
        raise VmadFormatError("truncated string length")
    (length,) = struct.unpack_from("<H", data, offset)
    end = offset + 2 + length
    if end > len(data):
        raise VmadFormatError("truncated string")
    return data[offset + 2 : end].decode("cp1252"), end


def _skip_script_body(data: bytes, offset: int) -> int:
    """Skip the flags byte and property list of a script entry."""
    offset = _advance(data, offset, 1)
    if offset + 2 > len(data):
        raise VmadFormatError("truncated property count")
    (property_count,) = struct.unpack_from("<H", data, offset)
    offset += 2
    for _ in range(property_count):
        _name, offset = _read_wstring(data, offset)
        offset = _skip_typed_value(data, offset)
    return offset


def _skip_typed_value(data: bytes, offset: int) -> int:
    """Skip a `<u8 type><u8 flags><value>` triple."""
    if offset + 2 > len(data):
        raise VmadFormatError("truncated property header")
    value_type = data[offset]
    offset += 2
    fixed = _FIXED_VALUE_SIZES.get(value_type)
    if fixed is not None:
        return _advance(data, offset, fixed)
    if value_type == 2:
        _value, offset = _read_wstring(data, offset)
        return offset
    if value_type == 7:
        return _skip_struct(data, offset)
    if value_type == 16:
        # xEdit models "Array of Variable" as a bare count with no elements.
        return _advance(data, offset, 4)
    element_size = _ARRAY_ELEMENT_SIZES.get(value_type)
    if element_size is not None:
        count, offset = _read_count(data, offset)
        return _advance(data, offset, count * element_size)
    if value_type == 12:
        count, offset = _read_count(data, offset)
        for _ in range(count):
            _element, offset = _read_wstring(data, offset)
        return offset
    if value_type == 17:
        count, offset = _read_count(data, offset)
        for _ in range(count):
            offset = _skip_struct(data, offset)
        return offset
    raise VmadFormatError(f"unknown VMAD property type {value_type}")


def _skip_struct(data: bytes, offset: int) -> int:
    count, offset = _read_count(data, offset)
    for _ in range(count):
        _member, offset = _read_wstring(data, offset)
        offset = _skip_typed_value(data, offset)
    return offset


def _read_count(data: bytes, offset: int) -> tuple[int, int]:
    if offset + 4 > len(data):
        raise VmadFormatError("truncated element count")
    (count,) = struct.unpack_from("<i", data, offset)
    if count < 0:
        raise VmadFormatError("negative element count")
    return count, offset + 4


def _advance(data: bytes, offset: int, size: int) -> int:
    end = offset + size
    if end > len(data):
        raise VmadFormatError("truncated value")
    return end


def build_script(
    name: str,
    properties: Sequence[tuple[str, int, bytes]],
) -> ScriptEntry:
    """Encode a top-level ``Scripts`` entry from `(name, type, value)` triples.

    The status bytes match what the native writer emits: ``0`` for the script
    (local) and ``1`` for every property (edited).
    """
    body = b"".join(
        _write_wstring(property_name) + bytes((value_type, 1)) + value
        for property_name, value_type, value in properties
    )
    raw = (
        _write_wstring(name)
        + b"\x00"
        + len(properties).to_bytes(2, "little")
        + body
    )
    return ScriptEntry(name, raw)


def object_value(form_id: int, *, object_format: int, alias: int = -1) -> bytes:
    """Encode a type-1 Object property value."""
    if object_format == 2:
        return struct.pack("<HhI", 0, alias, form_id)
    if object_format == 1:
        return struct.pack("<IhH", form_id, alias, 0)
    raise VmadFormatError(f"unsupported VMAD object format {object_format}")


def object_array_value(
    form_ids: Sequence[int],
    *,
    object_format: int,
    alias: int = -1,
) -> bytes:
    """Encode a type-11 Array of Object property value."""
    return len(form_ids).to_bytes(4, "little", signed=True) + b"".join(
        object_value(form_id, object_format=object_format, alias=alias)
        for form_id in form_ids
    )


def int_array_value(values: Sequence[int]) -> bytes:
    """Encode a type-13 Array of Int32 property value."""
    return len(values).to_bytes(4, "little", signed=True) + b"".join(
        value.to_bytes(4, "little", signed=True) for value in values
    )


def _write_wstring(value: str) -> bytes:
    encoded = value.encode("cp1252")
    return len(encoded).to_bytes(2, "little") + encoded
