import pytest
import hashlib
from pathlib import Path

from bacup_lib import quick_boy_ui as ui


def test_missing_background_fails_closed():
    from bacup_lib.quest_area_ui import tag
    import struct
    # An otherwise valid one-frame movie without the required PipboyMenu symbol.
    body = b"\x08\x00\x00\x1e\x01\x00" + tag(0, b"")
    with pytest.raises(ValueError, match="PipboyMenu"):
        ui.build_movie(b"FWS\x0f" + struct.pack("<I", len(body) + 8) + body)


def test_missing_screen_does_not_emit_movie(tmp_path):
    with pytest.raises(FileNotFoundError, match="screen geometry"):
        ui.convert_quick_boy_ui(tmp_path / "source", tmp_path / "data")
    assert not (tmp_path / "data" / ui.OUTPUT).exists()


@pytest.mark.parametrize("supported", [False, True])
@pytest.mark.parametrize("menu_source", ui.MENU_SOURCES)
def test_screen_conversion_and_provenance(tmp_path, monkeypatch, supported, menu_source):
    from creation_lib.nif import native_runtime

    source, output = tmp_path / "source", tmp_path / "data"
    screen = source / ui.SCREEN_SOURCE
    screen.parent.mkdir(parents=True)
    screen.write_bytes(b"installed mesh fixture")
    texture = source / ui.BACKDROP_TEXTURE
    texture.parent.mkdir(parents=True)
    texture.write_bytes(b"installed backdrop texture fixture")
    (source / "interface").mkdir()
    (source / menu_source).write_bytes(b"installed movie fixture")
    monkeypatch.setattr(ui, "build_movie", lambda source: (b"generated movie fixture", {}))
    monkeypatch.setattr(ui, "merge_ui_translations", lambda *args: None)

    def convert(src, dst, source_game, target_game, unused, options):
        assert Path(src) == screen and Path(dst) == output / ui.SCREEN_OUTPUT
        assert (source_game, target_game) == ("fo76", "fo4")
        assert options["source_path"] == ui.SCREEN_SOURCE.as_posix()
        if supported:
            Path(dst).write_bytes(b"converted mesh fixture")
        return {"supported": supported}

    monkeypatch.setattr(native_runtime, "convert_nif_file_raw", convert)
    if not supported:
        with pytest.raises(ValueError, match="screen conversion failed"):
            ui.convert_quick_boy_ui(source, output)
        assert not (output / ui.OUTPUT).exists()
    else:
        report = ui.convert_quick_boy_ui(source, output)
        assert report["source_movie"] == menu_source.as_posix()
        assert report["source_sha256"] == hashlib.sha256((source / menu_source).read_bytes()).hexdigest()
        assert report["screen_source_sha256"] == hashlib.sha256(screen.read_bytes()).hexdigest()
        assert {entry["path"] for entry in report["files"]} == {
            ui.OUTPUT.as_posix(), ui.SCREEN_OUTPUT.as_posix(), ui.BACKDROP_TEXTURE.as_posix()}
        for entry in report["files"]:
            assert entry["sha256"] == hashlib.sha256((output / entry["path"]).read_bytes()).hexdigest()


@pytest.mark.parametrize("symbol", ui.MENU_SYMBOLS)
def test_menu_variants_extract_only_background(symbol):
    import struct
    from bacup_lib.quest_area_ui import tag, symbol_ids
    from bacup_lib.legendary_perks_ui import swf_tags
    from creation_lib.swf.native_runtime import unbacked_symbol_classes
    background = struct.pack("<HH", 1, 1) + tag(1, b"") + tag(0, b"")
    placement = b"\x22" + struct.pack("<HH", 1, 1) + b"MainBackground_mc\0"
    root = struct.pack("<HH", 2, 1) + tag(26, placement) + tag(1, b"") + tag(0, b"")
    bindings = struct.pack("<HH", 1, 2) + symbol.encode() + b"\0"
    body = (b"\x08\x00\x00\x1e\x01\x00" + tag(39, background) + tag(39, root)
            + tag(76, bindings) + tag(71, b"fonts_en.swf\0\x01\0\0\0") + tag(0, b""))
    movie, report = ui.build_movie(b"FWS\x0f" + struct.pack("<I", len(body) + 8) + body)
    assert report["source_member"] == symbol + ".MainBackground_mc"
    assert symbol_ids(swf_tags(movie)) == {"B21_QuickBoy": 0, "B21_QuickBoyArt": 2}
    assert not any(code in (57, 71) for code, _ in swf_tags(movie))
    assert not unbacked_symbol_classes(movie)


def test_new_menu_preferred_and_invalid_new_menu_not_masked(tmp_path, monkeypatch):
    screen = tmp_path / ui.SCREEN_SOURCE
    screen.parent.mkdir(parents=True)
    screen.write_bytes(b"mesh")
    texture = tmp_path / ui.BACKDROP_TEXTURE
    texture.parent.mkdir(parents=True)
    texture.write_bytes(b"texture")
    interface = tmp_path / "interface"
    interface.mkdir()
    (interface / "newpipboymenu.swf").write_bytes(b"new")
    (interface / "pipboymenu.swf").write_bytes(b"old")
    def check(source):
        assert source == b"new"
        raise ValueError("invalid new menu")
    monkeypatch.setattr(ui, "build_movie", check)
    with pytest.raises(ValueError, match="invalid new menu"):
        ui.convert_quick_boy_ui(tmp_path, tmp_path / "output")
    assert not (tmp_path / "output").exists()
