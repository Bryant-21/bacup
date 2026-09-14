from pathlib import Path

from bacup_lib import regen_pipeline
from bacup_lib.models import PhaseSelection, PluginPortOptions
from bacup_lib.regen_pipeline import RegenOptions, RegenPaths, RegenResult


def test_regen_options_release_defaults():
    o = RegenOptions()
    assert o.deploy is True
    assert o.ba2_mode == "packed"
    assert o.archive_max_bytes == 16 * 1024**3
    assert o.ba2_compression_level is None
    assert o.deploy_loose is False
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
    assert o.hydrate_upgrade_from_deployed is False


def test_runtime_archive_registration_is_only_used_for_expanded_ba2s():
    assert regen_pipeline._should_register_runtime_archives(RegenOptions()) is False
    assert (
        regen_pipeline._should_register_runtime_archives(
            RegenOptions(ba2_mode="expanded")
        )
        is True
    )
    assert (
        regen_pipeline._should_register_runtime_archives(
            RegenOptions(ba2_mode="expanded", deploy_loose=True)
        )
        is False
    )


def test_generate_precombines_defaults_off_across_option_types():
    # Experimental gate: every option surface defaults the flag off so a standard
    # full build never schedules the phase.
    assert PhaseSelection().generate_precombines is False
    assert PhaseSelection.defaults().generate_precombines is False
    assert PluginPortOptions().generate_precombines is False
    assert RegenOptions().generate_precombines is False


def test_face_conversion_is_enabled_for_supported_fo4_pairs():
    fnv = regen_pipeline._build_options(False, None, None, pair_id="fnvfo3:fo4")
    skyrim = regen_pipeline._build_options(
        False, None, None, pair_id="skyrimse:fo4"
    )
    fo76 = regen_pipeline._build_options(False, None, None, pair_id="fo76:fo4")
    records_only = regen_pipeline._build_options(
        True, None, None, pair_id="fnvfo3:fo4"
    )

    assert fnv.convert_npc_faces is True
    assert skyrim.convert_npc_faces is True
    assert fo76.convert_npc_faces is False
    assert records_only.convert_npc_faces is False


def test_fnv_quest_slice_and_selected_compiler_survive_option_building():
    options = regen_pipeline._build_options(
        False,
        None,
        None,
        pair_id="fnvfo3:fo4",
        fnv_quest_slice=True,
        papyrus_compiler="exe-batch",
    )
    assert options.fnv_quest_slice is True
    assert options.papyrus_compiler == "exe-batch"


def test_melee_only_survives_option_building():
    options = regen_pipeline._build_options(
        False,
        None,
        None,
        pair_id="skyrimse:fo4",
        mvp_melee_only=True,
    )

    assert options.mvp_melee_only is True


def test_skyrim_gameplay_option_building_has_no_product_specific_switch():
    options = regen_pipeline._build_options(
        False, None, None, pair_id="skyrimse:fo4"
    )

    assert not hasattr(options, "skyrim_jzargo_slice")


def test_generate_mode_synthesizes_object_lod_for_every_source_pair():
    for pair_id in ("fo76:fo4", "fnvfo3:fo4", "skyrimse:fo4"):
        phases = PhaseSelection(lod_mode="generate")

        options = regen_pipeline._build_options(
            False, None, None, phases=phases, pair_id=pair_id
        )

        assert options.synthesize_object_lod is True


def test_non_fo76_hybrid_modes_synthesize_record_object_lod():
    for pair_id in ("fnvfo3:fo4", "skyrimse:fo4"):
        for lod_mode in ("hybrid", "hybrid-atlas"):
            phases = PhaseSelection(lod_mode=lod_mode)

            options = regen_pipeline._build_options(
                False, None, None, phases=phases, pair_id=pair_id
            )

            assert options.synthesize_object_lod is True


def test_fo76_hybrid_modes_do_not_synthesize_record_object_lod():
    for pair_id in (None, "fo76:fo4"):
        for lod_mode in ("hybrid", "hybrid-atlas"):
            phases = PhaseSelection(lod_mode=lod_mode)

            options = regen_pipeline._build_options(
                False, None, None, phases=phases, pair_id=pair_id
            )

            assert options.synthesize_object_lod is False


def test_non_generated_lod_modes_do_not_synthesize_object_lod():
    for lod_mode in ("convert", "none"):
        phases = PhaseSelection(lod_mode=lod_mode)

        options = regen_pipeline._build_options(
            False, None, None, phases=phases, pair_id="skyrimse:fo4"
        )

        assert options.synthesize_object_lod is False


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
