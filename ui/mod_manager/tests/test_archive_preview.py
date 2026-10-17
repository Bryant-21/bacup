import threading

import pytest
from PIL import Image

from creation_lib.ba2.native_runtime import pack_archive, extract_one_bounded
from creation_lib.mod_manager.archive_index import Archive, preview, source_stamp
from creation_lib.mod_manager.conflicts import Provider
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_launch_targets import targets  # noqa: F401


def provider(path):
    size, modified = source_stamp(path)
    return Archive(path.name, Provider("fixture", "Generated", 0, True, False, str(path), size, modified),
                   True, "listed", "fixture", 0)


def test_cli_archive_preview_uses_selected_game_target(targets, tmp_path):  # noqa: F811
    import json
    from click.testing import CliRunner
    from cli.main import cli
    library, profile, games, ids = targets
    source = tmp_path / "preview-source"
    source.mkdir()
    for index, (game, target) in enumerate(zip(games, ids, strict=True)):
        (source / "readme.txt").write_text(f"Target {index}")
        pack_archive(str(source), str(game / "Data" / "Preview.ba2"), "fo4", compress=True)
        result = CliRunner().invoke(cli, ["--format", "json", "manager", "archives", "--library", str(library.root),
                                         "--profile", profile, "--target", target, "--archive", "Preview.ba2", "--member", "readme.txt"])
        assert result.exit_code == 0, result.output
        assert json.loads(result.output)["text"] == f"Target {index}"


@pytest.mark.parametrize("kind", ["fo4og", "fo4", "tes4", "skyrim", "sse"])
@pytest.mark.parametrize("compressed", [False, True])
def test_native_archive_previews_are_bounded_inert_and_format_independent(tmp_path, kind, compressed):
    source = tmp_path / "source"
    (source / "docs").mkdir(parents=True)
    (source / "docs/readme.txt").write_text("hello archive " * 6000, encoding="utf-8")
    (source / "docs/binary.bin").write_bytes(b"\0\xff" * 3000)
    (source / "docs/utf16.txt").write_bytes("hello \u263a".encode("utf-16"))
    Image.new("RGB", (16, 8), (20, 40, 60)).save(source / "docs/art.png")
    Image.new("RGB", (16, 8), (20, 40, 60)).save(source / "docs/art.dds")
    path = tmp_path / "fixture.ba2"
    pack_archive(str(source), str(path), kind, compress=compressed)
    before = path.read_bytes()
    archive = provider(path)
    text = preview(archive, "DOCS/README.TXT")
    assert text.kind == "text" and text.truncated and len(text.text) == 32768
    binary = preview(archive, "docs/binary.bin")
    assert binary.kind == "hex" and binary.truncated and binary.bytes == 6000
    assert preview(archive, "docs/utf16.txt").text == "hello \u263a"
    assert preview(archive, "docs/art.png").pixels.shape == (8, 16, 3)
    assert preview(archive, "docs/art.dds").pixels.shape == (8, 16, 3)
    with pytest.raises(ValueError, match="preview limit"):
        extract_one_bounded(path, "docs/readme.txt", 1024)
    assert path.read_bytes() == before


def test_archive_preview_rejects_stale_missing_unsafe_and_cancelled(tmp_path):
    source = tmp_path / "source"
    source.mkdir()
    (source / "fixture.txt").write_text("fixture")
    path = tmp_path / "fixture.ba2"
    pack_archive(str(source), str(path), "fo4", compress=True)
    archive = provider(path)
    for name in ["../escape", "C:/outside", "missing.txt"]:
        with pytest.raises(ManagerError):
            preview(archive, name)
    cancelled = threading.Event()
    cancelled.set()
    with pytest.raises(Cancelled):
        preview(archive, "fixture.txt", cancelled)
    path.write_bytes(path.read_bytes() + b"changed")
    with pytest.raises(ManagerError, match="changed"):
        preview(archive, "fixture.txt")


@pytest.mark.parametrize("kind", ["fo4ogdds", "fo4dds"])
@pytest.mark.parametrize("compressed", [False, True])
def test_dx10_texture_archive_preview_reconstructs_dds_with_a_byte_bound(tmp_path, kind, compressed):
    source = tmp_path / "textures"
    source.mkdir()
    Image.new("RGBA", (32, 16), (20, 40, 60, 255)).save(source / "fixture.dds")
    path = tmp_path / "textures.ba2"
    pack_archive(str(source), str(path), kind, compress=compressed)
    assert preview(provider(path), "fixture.dds").pixels.shape == (16, 32, 3)
    with pytest.raises(ValueError, match="preview limit"):
        extract_one_bounded(path, "fixture.dds", 128)


@pytest.mark.parametrize("kind", ["tes4", "skyrim", "sse"])
def test_bsa_compression_cannot_expand_past_a_falsified_size_header(tmp_path, kind):
    import struct
    source = tmp_path / "source"
    source.mkdir()
    (source / "fixture.txt").write_bytes(b"generated" * 4096)
    path = tmp_path / "fixture.bsa"
    pack_archive(str(source), str(path), kind, compress=True)
    data = bytearray(path.read_bytes())
    offset = data.index(struct.pack("<I", 9 * 4096))
    data[offset:offset + 4] = struct.pack("<I", 1)
    path.write_bytes(data)
    with pytest.raises(ValueError, match="preview limit|size mismatch"):
        extract_one_bounded(path, "fixture.txt", 1024)
