from pathlib import Path

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenPaths


def _paths(tmp_path: Path, *, output_root: Path, deploy_data_dir: Path) -> RegenPaths:
    return RegenPaths(
        source_extracted_dir=tmp_path / "fo76_extracted",
        source_data_dir=tmp_path / "fo76" / "Data",
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=deploy_data_dir,
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=output_root,
        resource_dir=tmp_path / "resource",
    )


class _Timing:
    def record(self, *_args, **_kwargs) -> None:
        pass


# NOTE: "Terrain" is intentionally not one of these labels. Task 1 (adding the
# Terrain archive family to _MAIN_FAMILY_ORDER / _GENERATED_LABEL_BASES in
# archive_plan.py) has not landed on this branch yet, so a "SeventySix -
# Terrain.ba2" fixture would silently fail discover_mod_archives's
# _is_generated_archive_name filter today, for reasons unrelated to this
# task's swap/discard logic. "Sounds" stands in for "an independent,
# already-recognized family that must survive an unrelated swap untouched" —
# once Task 1 lands, Terrain behaves identically (it's not special-cased
# anywhere in _archives_for_labels/_swap_deploy_archives).
_DEPLOYED_BA2S = (
    "Meshes",
    "Meshes1",
    "MeshesExtra",
    "Materials",
    "Textures",
    "Textures2",
    "LOD",
    "Sounds",
)
_PACKED_BA2S = ("Meshes", "Meshes1", "Materials")


def _seed_deploy_dir(deploy_data_dir: Path) -> None:
    deploy_data_dir.mkdir(parents=True)
    (deploy_data_dir / "SeventySix.esm").write_bytes(b"old-esm")
    for label in _DEPLOYED_BA2S:
        (deploy_data_dir / f"SeventySix - {label}.ba2").write_bytes(f"old-{label}".encode())


def _seed_output_dir(output_root: Path) -> None:
    output_root.mkdir(parents=True)
    (output_root / "SeventySix.esm").write_bytes(b"new-esm")
    for label in _PACKED_BA2S:
        (output_root / f"SeventySix - {label}.ba2").write_bytes(f"new-{label}".encode())


def test_archives_for_labels_meshes_prefix_excludes_meshes_extra():
    names = [
        "SeventySix - Meshes.ba2",
        "SeventySix - Meshes1.ba2",
        "SeventySix - MeshesExtra.ba2",
        "SeventySix - MeshesExtra1.ba2",
        "SeventySix - Materials.ba2",
    ]

    assert regen_pipeline._archives_for_labels(names, ("Meshes",)) == [
        "SeventySix - Meshes.ba2",
        "SeventySix - Meshes1.ba2",
    ]
    assert regen_pipeline._archives_for_labels(names, ("MeshesExtra",)) == [
        "SeventySix - MeshesExtra.ba2",
        "SeventySix - MeshesExtra1.ba2",
    ]
    assert regen_pipeline._archives_for_labels(names, ("Meshes", "MeshesExtra")) == [
        "SeventySix - Meshes.ba2",
        "SeventySix - Meshes1.ba2",
        "SeventySix - MeshesExtra.ba2",
        "SeventySix - MeshesExtra1.ba2",
    ]


def test_archives_for_labels_matches_lod_general_and_texture_archives():
    names = [
        "SeventySix - LOD.ba2",
        "SeventySix - LOD2.ba2",
        "SeventySix - LODTextures.ba2",
        "SeventySix - LODTextures2.ba2",
        "SeventySix - TerrainTextures.ba2",
    ]

    assert regen_pipeline._archives_for_labels(names, ("LOD", "LODTextures")) == names[:4]


def test_swap_deploy_archives_deletes_and_copies_only_matching_families(tmp_path):
    output_root = tmp_path / "mods" / "SeventySix"
    deploy_data_dir = tmp_path / "Fallout4" / "Data"
    _seed_output_dir(output_root)
    _seed_deploy_dir(deploy_data_dir)

    regen_pipeline._swap_deploy_archives(
        output_root,
        deploy_data_dir,
        ["SeventySix.esm"],
        ("Meshes", "MeshesExtra", "Materials"),
    )

    remaining = {p.name for p in deploy_data_dir.glob("*.ba2")}
    assert remaining == {
        "SeventySix - Meshes.ba2",
        "SeventySix - Meshes1.ba2",
        "SeventySix - Materials.ba2",
        "SeventySix - Textures.ba2",
        "SeventySix - Textures2.ba2",
        "SeventySix - LOD.ba2",
        "SeventySix - Sounds.ba2",
    }
    # Untouched families kept their original bytes.
    assert (deploy_data_dir / "SeventySix - Textures.ba2").read_bytes() == b"old-Textures"
    assert (deploy_data_dir / "SeventySix - Textures2.ba2").read_bytes() == b"old-Textures2"
    assert (deploy_data_dir / "SeventySix - LOD.ba2").read_bytes() == b"old-LOD"
    assert (deploy_data_dir / "SeventySix - Sounds.ba2").read_bytes() == b"old-Sounds"
    # MeshesExtra had no fresh replacement packed, so it was deleted but not
    # replaced (discard rule: only families in swap_labels AND present in the
    # freshly packed output are copied back in).
    assert not (deploy_data_dir / "SeventySix - MeshesExtra.ba2").is_file()
    # Swapped families picked up the freshly packed bytes.
    assert (deploy_data_dir / "SeventySix - Meshes.ba2").read_bytes() == b"new-Meshes"
    assert (deploy_data_dir / "SeventySix - Meshes1.ba2").read_bytes() == b"new-Meshes1"
    assert (deploy_data_dir / "SeventySix - Materials.ba2").read_bytes() == b"new-Materials"


def test_deploy_post_steps_selective_swap_recomputes_full_archive_list(monkeypatch, tmp_path):
    output_root = tmp_path / "mods" / "SeventySix"
    deploy_data_dir = tmp_path / "Fallout4" / "Data"
    _seed_output_dir(output_root)
    _seed_deploy_dir(deploy_data_dir)

    def fake_deploy_output_mods(
        output_root_name,
        *,
        plugin_names,
        project_root,
        game_data_dir,
        resource_dir,
        deploy_archives=True,
    ):
        # Full-deploy archive copy must be off for a selective swap; the ESM
        # copy itself is unconditional inside deploy_mod and out of scope here.
        assert deploy_archives is False
        (game_data_dir / "SeventySix.esm").write_bytes((output_root / "SeventySix.esm").read_bytes())

    registered: dict[str, list[str]] = {}

    def fake_register(archive_names, *, ini_path=None, base_ini_path=None):
        registered["names"] = list(archive_names)
        return []

    monkeypatch.setattr(regen_pipeline, "_deploy_output_mods", fake_deploy_output_mods)
    monkeypatch.setattr(regen_pipeline, "_write_runtime_archive_ini_state", lambda *a, **k: False)
    monkeypatch.setattr(regen_pipeline, "_fo4_ini_archive_names_for_plugins", lambda *a, **k: [])
    monkeypatch.setattr(regen_pipeline, "_remove_fo4_archive_ini_entries", lambda *a, **k: [])
    monkeypatch.setattr(regen_pipeline, "_cleanup_fo4_archive_ini_overrides", lambda *a, **k: 0)
    monkeypatch.setattr(regen_pipeline, "_register_runtime_archive_ini_entries", fake_register)

    timing = _Timing()
    regen_pipeline._deploy_post_steps(
        _paths(tmp_path, output_root=output_root, deploy_data_dir=deploy_data_dir),
        ["SeventySix.esm"],
        timing,
        swap_labels=("Meshes", "MeshesExtra", "Materials"),
    )

    assert (deploy_data_dir / "SeventySix.esm").read_bytes() == b"new-esm"
    # Union of untouched (Textures, Textures2, LOD, Sounds) + new (Meshes,
    # Meshes1, Materials) -- MeshesExtra was swapped-and-discarded (no fresh
    # replacement packed), so it's gone from both disk and this list. No
    # orphans: every name here is a real file in deploy_data_dir.
    assert set(registered["names"]) == {
        "SeventySix - Meshes.ba2",
        "SeventySix - Meshes1.ba2",
        "SeventySix - Materials.ba2",
        "SeventySix - Textures.ba2",
        "SeventySix - Textures2.ba2",
        "SeventySix - LOD.ba2",
        "SeventySix - Sounds.ba2",
    }
