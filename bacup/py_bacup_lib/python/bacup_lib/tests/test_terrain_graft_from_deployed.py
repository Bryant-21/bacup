"""Terrain graft sourcing for upgrade runs (Task 5).

In upgrade mode the terrain LAND/NAVM graft sources from the live deployed
``SeventySix.esm`` instead of the run-local ``.regen_land_cache.esm``, and the
legacy land-cache check/restore is skipped. These tests pin the two decision
seams that implement that: ``_resolve_terrain_graft`` (the plan) and
``_terrain_graft_source`` (the repointed prior-handle open-site), plus the
threading through ``_build_options`` -> ``PluginPortOptions``.

A full native-graft integration test is intentionally omitted: the native
``graft_terrain`` phase is unchanged (it already accepts any prior FO4 output
handle) and driving it needs a live ConversionRun with FO4 fixtures. The change
under test is purely *which* prior handle is opened and *whether* the cache
block runs -- exactly what these seams encapsulate.
"""
from pathlib import Path

from bacup_lib.models import PluginPortOptions
from bacup_lib.regen_pipeline import (
    RegenOptions,
    _build_options,
    _resolve_terrain_graft,
)
from bacup_lib.workflows.unified import _terrain_graft_source


class _StubRunner:
    def __init__(self):
        self.logs: list[tuple[str, str]] = []

    def emit_log(self, level, message):
        self.logs.append((level, message))


def test_non_upgrade_reuse_off_preserves_legacy():
    plan = _resolve_terrain_graft(RegenOptions(re_use_land=False), None, _StubRunner())
    assert plan.graft_esm is None
    assert plan.reuse_terrain_navmesh is False
    assert plan.run_land_cache_block is False
    assert plan.force_convert_terrain is False


def test_non_upgrade_reuse_on_runs_cache_block():
    plan = _resolve_terrain_graft(RegenOptions(re_use_land=True), None, _StubRunner())
    assert plan.graft_esm is None
    assert plan.reuse_terrain_navmesh is True
    # Legacy --re-use-land path is untouched: the cache block still runs.
    assert plan.run_land_cache_block is True
    assert plan.force_convert_terrain is False


def test_upgrade_readable_esm_grafts_and_skips_cache(tmp_path):
    deployed = tmp_path / "SeventySix.esm"
    deployed.write_bytes(b"TES4-not-empty")
    runner = _StubRunner()

    plan = _resolve_terrain_graft(RegenOptions(re_use_land=True), deployed, runner)

    assert plan.graft_esm == deployed
    assert plan.reuse_terrain_navmesh is True
    assert plan.run_land_cache_block is False  # never touch the cache in upgrade mode
    assert plan.force_convert_terrain is False
    assert runner.logs == []


def test_upgrade_missing_esm_falls_back_to_full_regen(tmp_path):
    missing = tmp_path / "does_not_exist.esm"
    runner = _StubRunner()

    plan = _resolve_terrain_graft(RegenOptions(re_use_land=True), missing, runner)

    assert plan.graft_esm is None
    assert plan.reuse_terrain_navmesh is False
    assert plan.run_land_cache_block is False
    assert plan.force_convert_terrain is True  # regenerate rather than hard-fail
    assert [lvl for lvl, _ in runner.logs] == ["WARN"]


def test_upgrade_empty_esm_is_treated_as_unreadable(tmp_path):
    empty = tmp_path / "SeventySix.esm"
    empty.write_bytes(b"")
    runner = _StubRunner()

    plan = _resolve_terrain_graft(RegenOptions(re_use_land=True), empty, runner)

    assert plan.force_convert_terrain is True
    assert plan.graft_esm is None


def test_graft_source_defaults_to_run_local_cache():
    opts = PluginPortOptions()
    assert _terrain_graft_source(opts, "/mod/root") == Path("/mod/root/.regen_land_cache.esm")


def test_graft_source_repoints_to_deployed_esm(tmp_path):
    deployed = tmp_path / "SeventySix.esm"
    opts = PluginPortOptions(terrain_graft_esm=deployed)
    # Upgrade source wins over the run-local cache regardless of mod_path.
    assert _terrain_graft_source(opts, "/mod/root") == deployed


def test_build_options_threads_graft_esm_into_source(tmp_path):
    deployed = tmp_path / "SeventySix.esm"
    opts = _build_options(
        False,
        None,
        None,
        reuse_terrain_navmesh=True,
        terrain_graft_esm=deployed,
    )
    assert opts.terrain_graft_esm == deployed
    # The build->graft wiring resolves back to the deployed ESM.
    assert _terrain_graft_source(opts, "/mod/root") == deployed
