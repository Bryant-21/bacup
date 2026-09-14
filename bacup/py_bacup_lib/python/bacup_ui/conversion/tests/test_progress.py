import pytest

from bacup_ui.conversion.progress import ProgressEstimate, estimated_phase_weights


def test_parallel_work_does_not_credit_unfinished_phases():
    estimate = ProgressEstimate({"nifs": 900, "records": 100, "pack": 100})
    estimate.update(
        {
            "ui_key": "nifs",
            "status": "running",
            "total_items": 10,
            "completed_items": 5,
        },
        now=0,
    )
    estimate.update({"ui_key": "records", "status": "completed"}, now=0)
    assert estimate.fraction(now=0) == 550 / 1100


def test_late_discovery_and_restarted_item_counts_do_not_move_backwards():
    estimate = ProgressEstimate({"nifs": 900, "pack": 100})
    estimate.update(
        {
            "ui_key": "nifs",
            "status": "running",
            "total_items": 10,
            "completed_items": 8,
        },
        now=0,
    )
    before = estimate.fraction(now=0)
    estimate.update({"ui_key": "new_phase", "status": "running"}, now=0)
    estimate.update(
        {
            "ui_key": "nifs",
            "status": "running",
            "total_items": 20,
            "completed_items": 1,
        },
        now=0,
    )
    assert estimate.fraction(now=0) >= before


def test_uncounted_phase_uses_elapsed_estimate_but_waits_for_completion():
    estimate = ProgressEstimate({"lodgen": 240})
    estimate.update({"ui_key": "lodgen", "status": "running"}, now=0)
    assert estimate.fraction(now=120) == 0.5
    assert estimate.fraction(now=500) == 0.95
    estimate.update({"ui_key": "lodgen", "status": "completed"}, now=500)
    assert estimate.fraction(now=500) == 0.99


def test_stopped_run_does_not_keep_advancing_elapsed_estimate():
    estimate = ProgressEstimate({"lodgen": 240})
    estimate.update({"ui_key": "lodgen", "status": "running"}, now=0)
    assert estimate.fraction(now=60) == 0.25
    assert estimate.fraction(now=500, advance=False) == 0.25


@pytest.mark.parametrize("status", ["error", "cancelled"])
def test_failed_and_cancelled_phases_do_not_complete(status):
    estimate = ProgressEstimate({"lodgen": 240})
    estimate.update({"ui_key": "lodgen", "status": "running"}, now=0)
    assert estimate.fraction(now=60) == 0.25
    estimate.update({"ui_key": "lodgen", "status": status}, now=60)
    assert estimate.fraction(now=500) == 0.25


def test_skipped_phase_is_removed_from_weight_denominator():
    estimate = ProgressEstimate({"nifs": 900, "lodgen": 100})
    estimate.update(
        {
            "ui_key": "nifs",
            "status": "running",
            "total_items": 10,
            "completed_items": 5,
        },
        now=0,
    )
    estimate.update({"ui_key": "lodgen", "status": "skipped"}, now=0)
    assert estimate.fraction(now=0) == 0.5


def test_pair_weights_exclude_unused_asset_phases():
    fo76 = estimated_phase_weights("fo76:fo4")
    fnv = estimated_phase_weights("fnvfo3:fo4", lod_mode="generate")
    skyrim = estimated_phase_weights("skyrimse:fo4", lod_mode="generate")
    starfield = estimated_phase_weights("starfield:fo4", lod_mode="generate")
    assert "convert_animations" not in fo76
    assert "convert_animations" in fnv
    assert "convert_animations" not in skyrim
    assert "convert_materials" not in fnv
    assert "convert_materials" not in skyrim
    assert "convert_materials" in starfield
    assert "convert_havok" in fo76
    assert "convert_havok" not in starfield
    assert "emit_projected_navmeshes" in fo76
    assert "emit_projected_navmeshes" not in fnv


def test_disabled_lod_packing_deployment_and_precombines_have_no_weight():
    weights = estimated_phase_weights(
        "fo76:fo4", lod_mode="none", packed=False, deploy=False
    )
    assert not {"lodgen", "pack", "deploy", "generate_precombines"} & weights.keys()


def test_lod_recovery_only_budgets_remaining_phases():
    assert estimated_phase_weights("fo76:fo4", start_phase="lodgen") == {
        "lodgen": 240,
        "pack": 300,
        "deploy": 120,
    }


def test_asset_recovery_omits_records_and_earlier_assets():
    weights = estimated_phase_weights("fo76:fo4", start_phase="textures")
    assert not {"translate_records", "convert_terrain", "convert_nifs"} & weights.keys()
    assert {
        "copy_sounds",
        "convert_textures",
        "convert_havok",
        "regenerate_modt",
    } <= weights.keys()


def test_recovery_without_checkpoint_budgets_full_rebuild():
    assert estimated_phase_weights(
        "fo76:fo4", start_phase="terrain"
    ) == estimated_phase_weights("fo76:fo4")


def test_deploy_existing_does_not_budget_conversion():
    assert estimated_phase_weights("fo76:fo4", start_phase="deploy") == {
        "pack": 300,
        "deploy": 120,
    }
