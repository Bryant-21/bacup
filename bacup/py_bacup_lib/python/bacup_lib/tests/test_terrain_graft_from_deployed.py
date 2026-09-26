"""Terrain graft sourcing for upgrade runs.

In upgrade mode the terrain LAND/NAVM graft sources from the deployed
``SeventySix.esm`` instead of the run-local ``.regen_land_cache.esm``, and the
land-cache check/restore is skipped. These tests pin ``_resolve_terrain_graft``
(the plan), ``_terrain_graft_source`` (which prior handle is opened), and the
threading through ``_build_options`` -> ``PluginPortOptions``.

There is no native-graft integration test: ``graft_terrain`` accepts any prior
FO4 output handle, and driving it needs a live ConversionRun with FO4 fixtures.
"""
from pathlib import Path

import pytest

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


@pytest.mark.parametrize("reuse", [False, True])
def test_non_upgrade_preserves_legacy_cache_block(reuse):
    plan = _resolve_terrain_graft(RegenOptions(re_use_land=reuse), None, _StubRunner())
    assert plan.graft_esm is None
    assert plan.reuse_terrain_navmesh is reuse
    assert plan.run_land_cache_block is reuse
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


@pytest.mark.parametrize("content", [None, b""], ids=["missing", "empty"])
def test_upgrade_unreadable_esm_falls_back_to_full_regen(tmp_path, content):
    deployed = tmp_path / "SeventySix.esm"
    if content is not None:
        deployed.write_bytes(content)
    runner = _StubRunner()

    plan = _resolve_terrain_graft(RegenOptions(re_use_land=True), deployed, runner)

    assert plan.graft_esm is None
    assert plan.reuse_terrain_navmesh is False
    assert plan.run_land_cache_block is False
    assert plan.force_convert_terrain is True  # regenerate rather than hard-fail
    if content is None:
        assert [lvl for lvl, _ in runner.logs] == ["WARN"]


def test_graft_source_defaults_to_cache_and_repoints_to_deployed_esm(tmp_path):
    assert _terrain_graft_source(PluginPortOptions(), "/mod/root") == Path("/mod/root/.regen_land_cache.esm")
    deployed = tmp_path / "SeventySix.esm"
    opts = PluginPortOptions(terrain_graft_esm=deployed)
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
