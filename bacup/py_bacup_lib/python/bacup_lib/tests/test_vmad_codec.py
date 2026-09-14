"""Boundary reader for VMAD script entries.

The codec exists so a failed script can lose its own binding without the record
losing the rest of its VMAD. Getting an entry boundary wrong would silently
corrupt a shipped plugin, so every property type is exercised and every parse is
checked against an exact re-encode.
"""

from __future__ import annotations

import struct

import pytest

from bacup_lib import vmad_codec


def _wstr(value: str) -> bytes:
    encoded = value.encode("cp1252")
    return len(encoded).to_bytes(2, "little") + encoded


def _prop(name: str, type_id: int, value: bytes = b"") -> bytes:
    return _wstr(name) + bytes([type_id, 1]) + value


def _script(name: str, properties: list[bytes] | None = None) -> bytes:
    properties = properties or []
    return (
        _wstr(name)
        + b"\x00"
        + len(properties).to_bytes(2, "little")
        + b"".join(properties)
    )


def _vmad(scripts: list[bytes], fragments: bytes = b"") -> bytes:
    return struct.pack("<hhH", 6, 2, len(scripts)) + b"".join(scripts) + fragments


def _struct_payload(members: list[bytes]) -> bytes:
    return len(members).to_bytes(4, "little", signed=True) + b"".join(members)


def _member(name: str, type_id: int, value: bytes = b"") -> bytes:
    return _wstr(name) + bytes([type_id, 1]) + value


ALL_PROPERTY_TYPES = [
    _prop("none", 0),
    _prop("object", 1, b"\x00\x00\xff\xff\x34\x12\x00\x00"),
    _prop("string", 2, _wstr("hello")),
    _prop("int", 3, (7).to_bytes(4, "little", signed=True)),
    _prop("float", 4, struct.pack("<f", 1.5)),
    _prop("bool", 5, b"\x01"),
    _prop("variable", 6),
    _prop("struct", 7, _struct_payload([_member("m", 3, (1).to_bytes(4, "little"))])),
    _prop("objects", 11, (2).to_bytes(4, "little", signed=True) + b"\x00" * 16),
    _prop("strings", 12, (2).to_bytes(4, "little", signed=True) + _wstr("a") + _wstr("")),
    _prop("ints", 13, (3).to_bytes(4, "little", signed=True) + b"\x00" * 12),
    _prop("floats", 14, (1).to_bytes(4, "little", signed=True) + b"\x00" * 4),
    _prop("bools", 15, (4).to_bytes(4, "little", signed=True) + b"\x01\x00\x01\x00"),
    _prop("variables", 16, (0).to_bytes(4, "little")),
    _prop(
        "structs",
        17,
        (2).to_bytes(4, "little", signed=True)
        + _struct_payload([_member("a", 2, _wstr("x"))])
        + _struct_payload([]),
    ),
]


def test_parse_splits_scripts_and_fragment_tail():
    data = _vmad(
        [_script("First", ALL_PROPERTY_TYPES), _script("Second")],
        fragments=b"\x02opaque fragment block",
    )

    vmad = vmad_codec.parse(data)

    assert [entry.name for entry in vmad.scripts] == ["First", "Second"]
    assert vmad.fragments == b"\x02opaque fragment block"
    assert vmad.with_scripts(vmad.scripts) == data


def test_dropping_one_script_keeps_the_rest_byte_exact():
    keep_a = _script("Keep", [_prop("string", 2, _wstr("v"))])
    drop = _script("Drop", ALL_PROPERTY_TYPES)
    keep_b = _script("AlsoKeep")
    data = _vmad([keep_a, drop, keep_b], fragments=b"tail")

    vmad = vmad_codec.parse(data)
    survivors = tuple(e for e in vmad.scripts if e.name != "Drop")

    assert vmad.with_scripts(survivors) == _vmad([keep_a, keep_b], fragments=b"tail")


def test_dropping_every_script_keeps_the_header_and_tail():
    data = _vmad([_script("Only")], fragments=b"tail")

    vmad = vmad_codec.parse(data)

    assert vmad.with_scripts(()) == struct.pack("<hhH", 6, 2, 0) + b"tail"


def test_empty_script_array_round_trips():
    data = _vmad([], fragments=b"fragments only")

    vmad = vmad_codec.parse(data)

    assert vmad.scripts == ()
    assert vmad.with_scripts(()) == data


@pytest.mark.parametrize(
    "data",
    [
        b"",
        b"\x06\x00\x02\x00",
        # Bogus script count: the entry parse runs off the end.
        b"\x06\x00\x02\x00opaque VMAD payload",
        # Unmodelled property type.
        _vmad([_script("S", [_prop("weird", 99)])]),
        # Object format the reader does not model.
        struct.pack("<hhH", 6, 9, 0),
    ],
)
def test_unreadable_vmad_raises_instead_of_guessing(data):
    with pytest.raises(vmad_codec.VmadFormatError):
        vmad_codec.parse(data)
