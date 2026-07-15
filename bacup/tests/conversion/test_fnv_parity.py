from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.invariants import check_run_invariants


OUR_ROOT = Path("mods/B21_FNV2FO4")
PLUGIN_NAME = "FalloutNV.esm"
OUR_PLUGIN = OUR_ROOT / "FalloutNV" / PLUGIN_NAME


def _converted_plugin_path(root: Path, plugin_name: str) -> Path:
    return root / Path(plugin_name).stem / plugin_name


def _require_plugin_paths(*paths: Path) -> None:
    missing = [str(path) for path in paths if not path.is_file()]
    if missing:
        pytest.skip(f"missing expected converted plugin output(s): {', '.join(missing)}")


def _asset_root(mod_root: Path) -> Path:
    data_root = mod_root / "data"
    return data_root if data_root.is_dir() else mod_root


def _mesh_count(mod_root: Path) -> int:
    meshes_root = _asset_root(mod_root) / "Meshes"
    if not meshes_root.is_dir():
        return 0
    return sum(1 for _ in meshes_root.rglob("*.nif"))


def _bgsm_count(mod_root: Path) -> int:
    materials_root = _asset_root(mod_root) / "Materials"
    if not materials_root.is_dir():
        return 0
    return sum(1 for _ in materials_root.rglob("*.bgsm"))


def test_converted_plugin_path_uses_per_plugin_folder() -> None:
    assert _converted_plugin_path(OUR_ROOT, PLUGIN_NAME) == OUR_PLUGIN


def test_fnv_converted_output_contains_meshes() -> None:
    _require_plugin_paths(OUR_PLUGIN)

    mesh_count = _mesh_count(OUR_PLUGIN.parent)

    assert mesh_count > 0, f"expected converted meshes under {OUR_PLUGIN.parent}"


def test_fnv_converted_bgsms_have_cast_shadows_enabled() -> None:
    _require_plugin_paths(OUR_PLUGIN)

    bgsm_count = _bgsm_count(OUR_PLUGIN.parent)
    result = check_run_invariants(
        OUR_PLUGIN.parent,
        expected_plugins=[OUR_PLUGIN.name],
        source_prefix="fnv",
    )

    assert bgsm_count > 0, f"expected converted BGSMs under {OUR_PLUGIN.parent}"
    assert result.ok, "\n".join(result.failures)
