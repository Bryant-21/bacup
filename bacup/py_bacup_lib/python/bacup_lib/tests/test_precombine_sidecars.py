from pathlib import Path

from bacup_lib.precombine_generation import precombine_sidecar_names, remove_existing_precombines
from bacup_lib.regen_pipeline import _deploy_precombine_sidecars, _remove_precombine_sidecars


def test_sidecar_names_follow_the_plugin_stem():
    assert precombine_sidecar_names("SeventySix.esm") == (
        "SeventySix - Geometry.csg",
        "SeventySix.cdx",
        "SeventySix - Exterior.cdx",
    )


def test_deploy_copies_sidecars_loose_and_drops_stale_ones(tmp_path: Path):
    output_root = tmp_path / "mod"
    game_data = tmp_path / "Data"
    output_root.mkdir()
    game_data.mkdir()
    (output_root / "Mod - Geometry.csg").write_bytes(b"bcsg")
    (output_root / "Mod - Exterior.cdx").write_bytes(b"bcdx")
    (game_data / "Mod.cdx").write_bytes(b"stale")

    assert _deploy_precombine_sidecars(output_root, game_data, ["Mod.esm"]) == 2

    assert (game_data / "Mod - Geometry.csg").read_bytes() == b"bcsg"
    assert (game_data / "Mod - Exterior.cdx").read_bytes() == b"bcdx"
    assert not (game_data / "Mod.cdx").exists()
    assert not list(game_data.glob("*manifest*"))


def test_undeploy_removes_sidecars_by_name(tmp_path: Path):
    (tmp_path / "Mod - Geometry.csg").write_bytes(b"bcsg")
    (tmp_path / "Mod.cdx").write_bytes(b"bcdx")
    (tmp_path / "Mod - Exterior.cdx").write_bytes(b"bcdx")
    (tmp_path / "Other.cdx").write_bytes(b"bcdx")

    assert sorted(_remove_precombine_sidecars(tmp_path, ["Mod.esm"])) == [
        "Mod - Exterior.cdx",
        "Mod - Geometry.csg",
        "Mod.cdx",
    ]
    assert (tmp_path / "Other.cdx").exists()


def test_regeneration_clears_sidecars_and_legacy_psg(tmp_path: Path):
    data = tmp_path / "data"
    groups = data / "Meshes" / "PreCombined" / "Mod.esm"
    groups.mkdir(parents=True)
    (groups / "00000800_00000001_OC.NIF").write_bytes(b"nif")
    (groups / "00000800_Physics.NIF").write_bytes(b"nif")
    (data / "Mod - Geometry.psg").write_bytes(b"bpsg")
    (tmp_path / "Mod - Geometry.csg").write_bytes(b"bcsg")
    (tmp_path / "Mod.cdx").write_bytes(b"bcdx")
    (tmp_path / "Mod - Exterior.cdx").write_bytes(b"bcdx")
    (tmp_path / "Mod.esm").write_bytes(b"TES4")

    assert remove_existing_precombines(data, tmp_path / "Mod.esm") == 5
    assert (tmp_path / "Mod.esm").exists()
    assert not groups.exists()
