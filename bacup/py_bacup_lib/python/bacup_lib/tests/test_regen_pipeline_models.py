from pathlib import Path

from bacup_lib.models import PhaseSelection, PluginPortOptions
from bacup_lib.regen_pipeline import RegenOptions, RegenPaths, RegenResult


def test_regen_options_release_defaults():
    o = RegenOptions()
    assert o.deploy is True
    assert o.ba2_mode == "packed"
    assert o.archive_max_bytes == 16 * 1024**3
    assert o.lod_mode == "hybrid-atlas"
    assert o.write_land_cache is True
    assert o.include_interior is True
    assert o.carry_interior_previs is False
    assert o.generate_precombines is False
    assert o.records_limit is None
    assert o.memory_report is False
    assert o.validate_collision is False
    assert o.direct_deploy_archives is False
    assert o.update_runtime_ini is True


def test_generate_precombines_defaults_off_across_option_types():
    # Experimental gate: every option surface defaults the flag off so a standard
    # full build never schedules the phase.
    assert PhaseSelection().generate_precombines is False
    assert PhaseSelection.defaults().generate_precombines is False
    assert PluginPortOptions().generate_precombines is False
    assert RegenOptions().generate_precombines is False


def test_regen_paths_requires_explicit_paths():
    p = RegenPaths(
        source_extracted_dir=Path("a"),
        source_data_dir=Path("b"),
        target_extracted_dir=Path("c"),
        target_data_dir=Path("d"),
        target_ck_ini_path=Path("e"),
        target_custom_ini_path=Path("f"),
        target_game_ini_path=Path("g"),
        output_root=Path("h"),
    )
    assert p.mod_name == "SeventySix"
    assert p.output_root == Path("h")
    assert p.deploy_data_dir is None
    assert p.diagnostics_root is None


def test_regen_result_holds_exit_code():
    r = RegenResult(
        exit_code=0,
        output_root=Path("h"),
        elapsed_seconds=1.0,
        deployed=False,
        failures=[],
        warnings=[],
    )
    assert r.exit_code == 0 and r.deployed is False
