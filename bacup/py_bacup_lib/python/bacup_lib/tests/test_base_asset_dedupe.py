from bacup_lib.base_asset_dedupe import (
    DEFAULT_FO76_FO4_RELOCATION_MESH_ROOTS,
    resolve_base_asset_namespace,
    resolve_base_asset_relocation_mesh_roots,
)


def test_fo76_fo4_defaults_to_meshes_landscape():
    assert resolve_base_asset_relocation_mesh_roots("fo76", "fo4", None) == (
        "meshes/landscape",
    )
    assert DEFAULT_FO76_FO4_RELOCATION_MESH_ROOTS == ("meshes/landscape",)


def test_explicit_roots_win_and_normalize():
    assert resolve_base_asset_relocation_mesh_roots(
        "fo76", "fo4", ["Meshes\\Landscape", "meshes/SetDressing"]
    ) == ("meshes/landscape", "meshes/setdressing")


def test_non_fo76_fo4_has_no_default_roots():
    assert resolve_base_asset_relocation_mesh_roots("skyrimse", "fo4", None) == ()


def test_namespace_still_fo76():
    assert resolve_base_asset_namespace("fo76", "fo4", None) == "FO76"
