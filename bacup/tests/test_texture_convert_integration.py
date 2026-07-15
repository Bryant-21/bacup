from pathlib import Path


def _write_dds(path: Path, rgba: tuple[int, int, int, int]) -> None:
    from creation_lib.dds import native_runtime as dds_native

    path.parent.mkdir(parents=True, exist_ok=True)
    dds_native.write_dds_rgba(
        str(path),
        1,
        1,
        bytes(rgba),
        format="R8G8B8A8_UNORM",
    )


def test_batch_convert_fo76_to_fo4_writes_specular_bundle(tmp_path):
    from bacup_lib.texture import batch_convert
    from creation_lib.core.game_profiles import FO4_PROFILE, FO76_PROFILE

    src = tmp_path / "src"
    out = tmp_path / "out"
    _write_dds(src / "armor_d.dds", (255, 0, 0, 255))
    _write_dds(src / "armor_r.dds", (0, 0, 0, 255))
    _write_dds(src / "armor_l.dds", (128, 255, 0, 192))

    report = batch_convert(src, out, FO76_PROFILE, FO4_PROFILE)

    assert report.errors == 0
    assert (out / "armor_d.dds").exists()
    assert (out / "armor_s.dds").exists()


def test_batch_convert_unsupported_native_pair_copies_with_target_suffix(tmp_path):
    from bacup_lib.texture import batch_convert
    from creation_lib.core.game_profiles import FO4_PROFILE, STARFIELD_PROFILE
    from creation_lib.dds import native_runtime as dds_native

    src = tmp_path / "src"
    out = tmp_path / "out"
    source_path = src / "helmet_color.dds"
    _write_dds(source_path, (11, 22, 33, 44))

    report = batch_convert(src, out, STARFIELD_PROFILE, FO4_PROFILE)

    output_path = out / "helmet_d.dds"
    assert report.errors == 0
    assert output_path.exists()
    assert dds_native.read_dds_rgba(str(output_path))["rgba"] == dds_native.read_dds_rgba(
        str(source_path)
    )["rgba"]
