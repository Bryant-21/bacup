"""The packed BA2s must carry the output plugin's name.

FO4 mounts "<PluginStem> - Main.ba2" and "<PluginStem> - Textures.ba2" by
PLUGIN name. Shipping "FNV_FO3 - Main.ba2" beside FalloutNV.esm loads the
plugin with none of its assets, and the failure is silent until the first
BGSStaticCollection::Build3D dereferences a null model handle. Catch it at
build time instead.
"""
from __future__ import annotations

from pathlib import Path

from bacup_lib import regen_pipeline


def _output_root(tmp_path: Path, plugin_stem: str) -> Path:
    root = tmp_path / "mods" / "FNV_FO3"
    root.mkdir(parents=True)
    for marker in (".game", ".source_game", ".source_plugin"):
        (root / marker).write_text("x", encoding="utf-8")
    (root / f"{plugin_stem}.esm").write_bytes(b"plugin")
    return root


def _failures(output_root: Path, *, skip_pack: bool = False) -> list[str]:
    failures, _warnings = regen_pipeline._check_run_invariants(
        output_root,
        records_only=False,
        plugin_names=["FalloutNV.esm"],
        skip_pack=skip_pack,
    )
    return failures


def test_archives_named_after_the_plugin_pass(tmp_path: Path) -> None:
    root = _output_root(tmp_path, "FalloutNV")
    (root / "FalloutNV - Main.ba2").write_bytes(b"main")
    (root / "FalloutNV - Textures.ba2").write_bytes(b"textures")

    assert not [f for f in _failures(root) if "BA2" in f]


def test_archives_named_after_the_mod_folder_fail(tmp_path: Path) -> None:
    root = _output_root(tmp_path, "FalloutNV")
    (root / "FNV_FO3 - Main.ba2").write_bytes(b"main")

    failures = [f for f in _failures(root) if "BA2 name does not match" in f]
    assert len(failures) == 1
    assert "FNV_FO3 - Main.ba2" in failures[0]
    assert "FalloutNV - <label>.ba2" in failures[0]


def test_missing_archives_still_fail(tmp_path: Path) -> None:
    root = _output_root(tmp_path, "FalloutNV")

    assert any("missing BA2 archive" in f for f in _failures(root))


def test_legacy_mod_named_archives_fail_even_when_packing_was_skipped(
    tmp_path: Path,
) -> None:
    """A resumed/upgrade run that skips packing still deploys whatever archives
    the workspace holds, so a legacy mod-named set must not slip through."""
    root = _output_root(tmp_path, "FalloutNV")
    (root / "FNV_FO3 - Textures.ba2").write_bytes(b"textures")

    failures = _failures(root, skip_pack=True)
    assert any("BA2 name does not match" in f for f in failures)
    assert not any("missing BA2 archive" in f for f in failures)


def _mod_tree(tmp_path: Path) -> tuple[Path, Path]:
    output_root = tmp_path / "mods" / "FNV_FO3"
    output_root.mkdir(parents=True)
    deploy_dir = tmp_path / "MO2" / "fnv"
    deploy_dir.mkdir(parents=True)
    return output_root, deploy_dir


def test_legacy_archives_are_swept_once_plugin_named_ones_exist(
    tmp_path: Path,
) -> None:
    output_root, deploy_dir = _mod_tree(tmp_path)
    (output_root / "FNV_FO3 - Main.ba2").write_bytes(b"legacy")
    (output_root / "FalloutNV - Main.ba2").write_bytes(b"fresh")
    (deploy_dir / "FNV_FO3 - Main.ba2").write_bytes(b"legacy")

    regen_pipeline._remove_mod_named_archives(
        "FNV_FO3", "FalloutNV", output_root, deploy_dir
    )

    assert not (output_root / "FNV_FO3 - Main.ba2").exists()
    assert not (deploy_dir / "FNV_FO3 - Main.ba2").exists()
    assert (output_root / "FalloutNV - Main.ba2").read_bytes() == b"fresh"


def test_legacy_archives_survive_when_nothing_replaces_them(tmp_path: Path) -> None:
    """Deploy-only over a not-yet-migrated workspace must not throw away the
    only packed copy of the assets."""
    output_root, deploy_dir = _mod_tree(tmp_path)
    (output_root / "FNV_FO3 - Main.ba2").write_bytes(b"legacy")
    (deploy_dir / "FNV_FO3 - Main.ba2").write_bytes(b"legacy")

    regen_pipeline._remove_mod_named_archives(
        "FNV_FO3", "FalloutNV", output_root, deploy_dir
    )

    assert (output_root / "FNV_FO3 - Main.ba2").exists()
    assert (deploy_dir / "FNV_FO3 - Main.ba2").exists()


def test_sweep_is_a_noop_when_mod_and_plugin_names_agree(tmp_path: Path) -> None:
    output_root = tmp_path / "mods" / "SeventySix"
    output_root.mkdir(parents=True)
    (output_root / "SeventySix - Main.ba2").write_bytes(b"main")

    regen_pipeline._remove_mod_named_archives(
        "SeventySix", "SeventySix", output_root, tmp_path / "Data"
    )

    assert (output_root / "SeventySix - Main.ba2").exists()
