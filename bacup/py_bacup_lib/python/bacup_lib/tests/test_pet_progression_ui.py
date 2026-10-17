import hashlib
import struct

import pytest

from bacup_lib import pet_progression_ui as ui


def field(tag, value):
    return {tag: {"raw_hex": value.hex()}}


def test_absent_board_ignores_stale_converted_movie(tmp_path):
    stale = tmp_path / "output" / ui.OUTPUT / ui.MENU
    stale.parent.mkdir(parents=True)
    stale.write_bytes(b"old converted movie")
    result = ui.convert_pet_progression_ui(tmp_path / "source", tmp_path / "output")
    assert result["available"] is False
    assert "absent" in result["reason"]


def test_missing_import_disables_board_without_writing_partial_output(tmp_path, monkeypatch):
    source = tmp_path / "source/interface"
    source.mkdir(parents=True)
    (source / ui.MENU).write_bytes(b"source")
    monkeypatch.setattr(ui, "imports", lambda _: ["missing.swf"])
    result = ui.convert_pet_progression_ui(source.parent, tmp_path / "output")
    assert result["available"] is False
    assert "missing.swf" in result["reason"]
    assert not (tmp_path / "output").exists()


def test_corrupt_board_is_optional(tmp_path):
    source = tmp_path / "interface"
    source.mkdir()
    (source / ui.MENU).write_bytes(b"not a SWF")
    assert ui.convert_pet_progression_ui(tmp_path, tmp_path / "output")["available"] is False


def test_complete_board_emits_verified_dependency_manifest(tmp_path, monkeypatch):
    from creation_lib.swf import native_runtime

    def movie(action):
        body = b"\x08\x00\x00\x1e\x01\x00" + struct.pack("<HI", (82 << 6) | 63, len(action)) + action + b"\0\0"
        return b"FWS\x11" + struct.pack("<I", len(body) + 8) + body

    source = tmp_path / "interface"
    source.mkdir()
    original, converted = movie(b"source"), movie(b"bridge")
    (source / ui.MENU).write_bytes(original)

    def compile_bridge(data, sources, dependencies):
        assert data == original
        assert set(sources) == {"PetProgressionTrackMenu"}
        assert dependencies == [original]
        return converted

    monkeypatch.setattr(native_runtime, "replace_as3_classes", compile_bridge)
    result = ui.convert_pet_progression_ui(tmp_path, tmp_path / "output")
    assert result["available"] is True
    assert (tmp_path / "output" / result["menu"]).read_bytes() == converted
    assert result["files"][0]["sha256"] == hashlib.sha256(converted).hexdigest()
    assert (source / ui.MENU).read_bytes() == original


def test_nodes_preserve_64bit_uid_and_ignore_fallback_and_trailing_fields():
    fields = [field("PGTI", (2**60 + 3).to_bytes(8, "little")), field("NAM0", (5).to_bytes(4, "little")),
              field("NAM1", b"\0\0\0\0"), field("NAM1", b"<ID=123>Primary\0"),
              field("NAM5", (123).to_bytes(4, "little")), field("ITME", b""),
              field("NAM1", b"<ID=124>Fallback\0"), field("NAM0", b"\1")]
    assert ui.parse_track(fields) == [{"uid": str(2**60 + 3), "level": 5, "name": "Primary", "icon_id": 123}]


def test_incomplete_or_duplicate_node_is_rejected():
    with pytest.raises(ValueError):
        ui.parse_track([field("PGTI", (1).to_bytes(8, "little"))])
    node = [field("PGTI", (1).to_bytes(8, "little")), field("NAM0", (5).to_bytes(4, "little")), field("NAM1", b"<ID=123>Test\0")]
    with pytest.raises(ValueError):
        ui.parse_track(node + node)


def test_prerequisites_after_primary_reward_keep_the_full_uid():
    uid = 2**60 + 5
    fields = [field("PGTI", (1).to_bytes(8, "little")), field("NAM0", (5).to_bytes(4, "little")),
              field("NAM1", b"<ID=123>Test\0"), field("ITME", b""), field("NAME", uid.to_bytes(8, "little"))]
    assert ui.parse_track(fields)[0]["requires"] == [str(uid)]
