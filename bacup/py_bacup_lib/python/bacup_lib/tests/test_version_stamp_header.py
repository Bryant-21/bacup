import struct

import pytest

from bacup_lib.version_stamp import read_plugin_snam_header


def _subrecord(sig: bytes, payload: bytes) -> bytes:
    return sig + struct.pack("<H", len(payload)) + payload


def _tes4(subrecords: bytes, flags: int = 0) -> bytes:
    # FO4 record header: sig(4) + data_size(4) + flags(4) + 12 trailing bytes = 24.
    header = b"TES4" + struct.pack("<I", len(subrecords)) + struct.pack("<I", flags) + b"\x00" * 12
    return header + subrecords


def test_reads_snam_from_header(tmp_path):
    subrecords = _subrecord(b"HEDR", b"\x00" * 12) + _subrecord(b"SNAM", b"alpha2\x00")
    esm = tmp_path / "stamped.esm"
    esm.write_bytes(_tes4(subrecords))
    assert read_plugin_snam_header(esm) == "alpha2"


@pytest.mark.parametrize(
    "payload",
    [
        pytest.param(_tes4(_subrecord(b"HEDR", b"\x00" * 12)), id="no_snam"),
        pytest.param(b"GRUP" + b"\x00" * 40, id="not_tes4"),
        pytest.param(None, id="missing_file"),
    ],
)
def test_unstamped_or_unreadable_plugin_returns_none(tmp_path, payload):
    esm = tmp_path / "plugin.esm"
    if payload is not None:
        esm.write_bytes(payload)
    assert read_plugin_snam_header(esm) is None
