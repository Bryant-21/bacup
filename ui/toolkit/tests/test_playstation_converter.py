from pathlib import Path

import pytest

from ui.tools.playstation_converter import (
    convert_mod_to_playstation,
    discover_ba2_archives,
)


def test_discover_ba2_archives_matches_plugin_and_excludes_ps(tmp_path):
    plugin = tmp_path / "Example.esp"
    plugin.write_bytes(b"")
    main = tmp_path / "Example - Main.ba2"
    textures = tmp_path / "Example - Textures.ba2"
    main.write_bytes(b"")
    textures.write_bytes(b"")
    (tmp_path / "Example - Main_ps.ba2").write_bytes(b"")
    (tmp_path / "Other - Main.ba2").write_bytes(b"")

    assert discover_ba2_archives(plugin) == [main, textures]


def test_conversion_extracts_converts_and_packs_ps_archives(tmp_path):
    plugin = tmp_path / "Example.esl"
    archive = tmp_path / "Example - Main.ba2"
    output = tmp_path / "output"
    plugin.write_bytes(b"")
    archive.write_bytes(b"")
    calls = []

    def extract(source, destination, *, workers):
        calls.append(("extract", Path(source).name, workers))
        asset = Path(destination) / "Meshes" / "test.hkx"
        asset.parent.mkdir(parents=True)
        asset.write_bytes(b"pc")

    def convert(source, destination, skip_existing, *, max_workers):
        assert source == destination
        assert skip_existing is False
        calls.append(("convert", max_workers))
        return {"converted": 1, "skipped": 0, "errors": []}

    def pack(mod_name, **kwargs):
        calls.append(("pack", mod_name, kwargs["pc"], kwargs["ps"]))
        destination = Path(kwargs["archive_output_dir"])
        destination.mkdir(parents=True, exist_ok=True)
        (destination / f"{mod_name} - Main_ps.ba2").write_bytes(b"ps")

    result = convert_mod_to_playstation(
        plugin,
        [archive],
        output,
        extractor=extract,
        asset_converter=convert,
        packer=pack,
    )

    assert calls == [
        ("extract", "Example - Main.ba2", 16),
        ("convert", 16),
        ("pack", "Example", False, True),
    ]
    assert [Path(path).name for path in result["archives"]] == ["Example - Main_ps.ba2"]


def test_conversion_requires_replace_for_existing_ps_archive(tmp_path):
    plugin = tmp_path / "Example.esm"
    archive = tmp_path / "Example - Main.ba2"
    output = tmp_path / "output"
    plugin.write_bytes(b"")
    archive.write_bytes(b"")
    output.mkdir()
    (output / "Example - Main_ps.ba2").write_bytes(b"old")

    with pytest.raises(FileExistsError, match="Enable replacement"):
        convert_mod_to_playstation(plugin, [archive], output)


def test_conversion_passes_opt_in_at9_settings_to_packer(tmp_path):
    plugin = tmp_path / "Example.esp"
    archive = tmp_path / "Example - Main.ba2"
    output = tmp_path / "output"
    plugin.write_bytes(b"")
    archive.write_bytes(b"")
    pack_calls = []

    def pack(mod_name, **kwargs):
        pack_calls.append((mod_name, kwargs))
        output.mkdir(parents=True, exist_ok=True)
        (output / "Example - Main_ps.ba2").write_bytes(b"ps")

    convert_mod_to_playstation(
        plugin,
        [archive],
        output,
        convert_wav_to_at9=True,
        extractor=lambda *_args, **_kwargs: None,
        asset_converter=lambda *_args, **_kwargs: {"converted": 0, "errors": []},
        packer=pack,
    )

    assert pack_calls[0][1]["ps_at9"] is True


def test_conversion_does_not_pack_when_asset_conversion_fails(tmp_path):
    plugin = tmp_path / "Example.esp"
    archive = tmp_path / "Example - Main.ba2"
    plugin.write_bytes(b"")
    archive.write_bytes(b"")

    def extract(_source, _destination, *, workers):
        assert workers == 16

    def convert(*_args, **_kwargs):
        return {"converted": 0, "errors": [{"path": "bad.hkx", "error": "invalid"}]}

    with pytest.raises(RuntimeError, match="bad.hkx"):
        convert_mod_to_playstation(
            plugin,
            [archive],
            tmp_path / "output",
            extractor=extract,
            asset_converter=convert,
            packer=lambda *_args, **_kwargs: pytest.fail("packer should not run"),
        )
