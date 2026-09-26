from pathlib import Path

from bacup_lib.regen_pipeline import RegenPaths, _effective_resource_dir


def _paths(resource_dir=None):
    return RegenPaths(
        source_extracted_dir=Path("x"),
        source_data_dir=Path("x"),
        target_extracted_dir=Path("x"),
        target_data_dir=Path("x"),
        target_ck_ini_path=Path("x"),
        target_custom_ini_path=Path("x"),
        target_game_ini_path=Path("x"),
        output_root=Path("R/mods/SeventySix"),
        resource_dir=resource_dir,
    )


def test_effective_resource_dir_prefers_explicit_then_project_root():
    p = _paths(resource_dir=Path("Z/bundle/resource"))
    assert _effective_resource_dir(p) == Path("Z/bundle/resource")
    assert _effective_resource_dir(_paths()) == Path("R/resource")
