from pathlib import Path

from ui.tools.animation.havok_converter import (
    HavokConverterTool,
    count_source_hkx,
    count_source_ps4_assets,
    ps4_destination_dir,
    ps4_output_dir,
    run_ps4_asset_batch,
)


def test_ps4_output_is_nested_under_the_source_folder(tmp_path: Path):
    assert Path(ps4_output_dir(str(tmp_path))) == tmp_path / "ps4"


def test_ps4_replace_destination_is_the_source_folder(tmp_path: Path):
    assert Path(ps4_destination_dir(str(tmp_path), True)) == tmp_path
    assert Path(ps4_destination_dir(str(tmp_path), False)) == tmp_path / "ps4"


def test_hkx_count_is_recursive_and_excludes_the_ps4_output(tmp_path: Path):
    (tmp_path / "nested").mkdir()
    (tmp_path / "nested" / "walk.hkx").write_bytes(b"source")
    (tmp_path / "idle.HKX").write_bytes(b"source")
    (tmp_path / "ignore.txt").write_text("not havok", encoding="utf-8")
    output = tmp_path / "ps4"
    output.mkdir()
    (output / "old.hkx").write_bytes(b"converted")

    assert count_source_hkx(str(tmp_path), str(output)) == 2


def test_ps4_asset_count_includes_nif_and_bto(tmp_path: Path):
    (tmp_path / "nested").mkdir()
    (tmp_path / "nested" / "skeleton.hkx").write_bytes(b"hkx")
    (tmp_path / "nested" / "body.NIF").write_bytes(b"nif")
    (tmp_path / "world.bto").write_bytes(b"bto")

    assert count_source_ps4_assets(str(tmp_path)) == {
        ".hkx": 1,
        ".nif": 1,
        ".bto": 1,
    }


def test_ps4_asset_batch_routes_all_formats_and_uses_16_workers(tmp_path: Path):
    source = tmp_path / "source"
    output = source / "ps4"
    (source / "nested").mkdir(parents=True)
    (source / "nested" / "idle.hkx").write_bytes(b"hkx")
    (source / "nested" / "body.nif").write_bytes(b"nif")
    (source / "terrain.bto").write_bytes(b"bto")
    nif_calls = []

    def convert_nif(src: str, dst: str):
        nif_calls.append((Path(src).suffix.lower(), Path(dst).relative_to(output).as_posix()))
        Path(dst).write_bytes(Path(src).read_bytes() + b"-ps4")

    result = run_ps4_asset_batch(
        str(source),
        str(output),
        False,
        havok_converter=lambda data: data + b"-ps4",
        nif_converter=convert_nif,
    )

    assert result == {
        "converted": 3,
        "skipped": 0,
        "errors": [],
        "workers": 16,
        "files": 3,
    }
    assert (output / "nested" / "idle.hkx").read_bytes() == b"hkx-ps4"
    assert sorted(nif_calls) == [
        (".bto", "terrain.bto"),
        (".nif", "nested/body.nif"),
    ]


def test_ps4_replace_routes_batch_to_the_source_and_disables_skip_existing(tmp_path: Path):
    tool = HavokConverterTool()
    tool._input_dir = str(tmp_path)
    tool._replace_source = True
    calls = []
    tool._start_batch = lambda target, *args: calls.append((target, args))

    tool._start_conversion()

    assert calls == [(tool._do_convert_ps4, (str(tmp_path), False))]


def test_replace_source_setting_defaults_off_and_round_trips():
    tool = HavokConverterTool()
    assert tool.get_default_settings()["replace_source"] is False

    tool.apply_settings({"replace_source": True})

    assert tool.collect_settings()["replace_source"] is True
