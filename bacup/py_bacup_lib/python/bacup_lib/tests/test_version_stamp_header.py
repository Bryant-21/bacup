import struct

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


def test_no_snam_returns_none(tmp_path):
    esm = tmp_path / "nosnam.esm"
    esm.write_bytes(_tes4(_subrecord(b"HEDR", b"\x00" * 12)))
    assert read_plugin_snam_header(esm) is None


def test_non_tes4_returns_none(tmp_path):
    esm = tmp_path / "notplugin.esm"
    esm.write_bytes(b"GRUP" + b"\x00" * 40)
    assert read_plugin_snam_header(esm) is None


def test_missing_file_returns_none(tmp_path):
    assert read_plugin_snam_header(tmp_path / "nope.esm") is None
