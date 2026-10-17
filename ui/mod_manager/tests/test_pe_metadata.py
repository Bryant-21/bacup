"""Generated inert PE sections; inspected DLLs are never loaded."""
from pathlib import Path
import struct
import sys

import pytest

from creation_lib.mod_manager.paths import ManagerError
from creation_lib.mod_manager.pe_metadata import exported_data, file_version, packed_runtime


def image(path, symbol="", data=b"", version=()):
    raw = bytearray(0x1400)
    raw[:2] = b"MZ"
    struct.pack_into("<I", raw, 60, 0x100)
    raw[0x100:0x104] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", raw, 0x104, 0x8664, 1, 0, 0, 0, 0xf0, 0x2022)
    struct.pack_into("<H", raw, 0x118, 0x20b)
    struct.pack_into("<I", raw, 0x118 + 108, 16)
    section = 0x118 + 0xf0
    struct.pack_into("<IIII", raw, section + 8, 0x1000, 0x1000, 0x1000, 0x400)
    if symbol:
        struct.pack_into("<II", raw, 0x118 + 112, 0x1000, 0x90)
        struct.pack_into("<IIIII", raw, 0x400 + 20, 1, 1, 0x1040, 0x1050, 0x1060)
        struct.pack_into("<I", raw, 0x440, 0x1100)
        struct.pack_into("<I", raw, 0x450, 0x1080)
        struct.pack_into("<H", raw, 0x460, 0)
        raw[0x480:0x480 + len(symbol)] = symbol.encode("ascii")
        raw[0x500:0x500 + len(data)] = data
    if version:
        text = ".".join(map(str, version)).encode("ascii")
        raw[0x80:0x80 + len(text)] = text
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)
    return path


def fixture_version(path):
    text = Path(path).read_bytes()[0x80:0xb0].split(b"\0", 1)[0].decode("ascii")
    return tuple(map(int, text.split("."))) if text else None


def test_bounded_data_export_absent_and_fixed_version_resource(tmp_path):
    path = image(tmp_path / "inert.dll", "Version", b"hello")
    assert exported_data(path, "Version", 5) == b"hello"
    assert exported_data(path, "Missing", 5) is None
    assert exported_data(image(tmp_path / "empty.dll"), "Missing", 8) is None
    assert file_version(path) is None
    assert file_version(Path(sys._base_executable)) is not None


@pytest.mark.parametrize("fault", ["section", "counts", "ordinal", "forwarder", "rva", "unterminated", "overlap"])
def test_rejects_malformed_exports(tmp_path, fault):
    path = image(tmp_path / "bad.dll", "Version", b"12345678")
    raw = bytearray(path.read_bytes())
    if fault == "section":
        struct.pack_into("<I", raw, 0x118 + 0xf0 + 20, len(raw))
    elif fault == "counts":
        struct.pack_into("<I", raw, 0x414, 100001)
    elif fault == "ordinal":
        struct.pack_into("<H", raw, 0x460, 1)
    elif fault == "forwarder":
        struct.pack_into("<I", raw, 0x440, 0x1080)
    elif fault == "rva":
        struct.pack_into("<I", raw, 0x440, 0x3000)
    elif fault == "unterminated":
        raw[0x480:0x580] = b"x" * 256
    else:
        struct.pack_into("<H", raw, 0x106, 2)
        raw[0x118 + 0xf0 + 40:0x118 + 0xf0 + 80] = raw[0x118 + 0xf0:0x118 + 0xf0 + 40]
    path.write_bytes(raw)
    with pytest.raises(ManagerError):
        exported_data(path, "Version", 8)


@pytest.mark.parametrize("build", [(-1, 10, 163), (256, 0, 0), (1, 256, 0), (1, 10, 4096)])
def test_runtime_packing_rejects_invalid_builds(build):
    with pytest.raises(ManagerError):
        packed_runtime(build)


def test_zero_exports_and_request_bounds(tmp_path):
    path = image(tmp_path / "empty.dll", "Version", b"123")
    raw = bytearray(path.read_bytes())
    struct.pack_into("<II", raw, 0x414, 0, 0)
    path.write_bytes(raw)
    assert exported_data(path, "Version", 3) is None
    for symbol, size in (("a", 0), ("a", 4097), ("é", 1)):
        with pytest.raises(ManagerError):
            exported_data(path, symbol, size)
