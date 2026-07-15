import pytest

from creation_lib.esp.plugin import replace_plugin_with_localized_sidecars


@pytest.mark.parametrize(
    ("temp_name", "temp_sidecar_stem"),
    [
        ("B21_Test.esm.tmp", "B21_Test.esm"),
        ("B21_Test.esm.wrldcarry.tmp", "B21_Test.esm.wrldcarry"),
    ],
)
def test_replace_plugin_with_localized_sidecars_uses_final_plugin_stem(
    tmp_path,
    temp_name: str,
    temp_sidecar_stem: str,
) -> None:
    plugin_path = tmp_path / "B21_Test.esm"
    temp_plugin_path = tmp_path / temp_name
    strings_dir = tmp_path / "Strings"
    strings_dir.mkdir()
    plugin_path.write_bytes(b"old")
    temp_plugin_path.write_bytes(b"new")
    (strings_dir / "B21_Test_en.STRINGS").write_bytes(b"old strings")
    (strings_dir / f"{temp_sidecar_stem}_en.STRINGS").write_bytes(b"new strings")
    (strings_dir / f"{temp_sidecar_stem}_en.DLSTRINGS").write_bytes(b"new dlstrings")

    replace_plugin_with_localized_sidecars(temp_plugin_path, plugin_path)

    assert plugin_path.read_bytes() == b"new"
    assert (strings_dir / "B21_Test_en.STRINGS").read_bytes() == b"new strings"
    assert (strings_dir / "B21_Test_en.DLSTRINGS").read_bytes() == b"new dlstrings"
    assert not temp_plugin_path.exists()
    assert not (strings_dir / f"{temp_sidecar_stem}_en.STRINGS").exists()
    assert not (strings_dir / f"{temp_sidecar_stem}_en.DLSTRINGS").exists()
