"""Upgrade-plan resolution + run_full_regen upgrade wiring (Task 7).

Two seams are pinned here:

* ``_resolve_upgrade_plan`` -- manifest + from-version -> UpgradePlan / no-op /
  full-build. ``read_plugin_snam`` is monkeypatched so no rebuilt .pyd is needed;
  the manifest is a real on-disk YAML (pure ``upgrade_manifest`` parsing).
* ``run_full_regen`` in upgrade mode -- the derived wiring that the resolver does
  NOT itself carry: ``re_use_land`` / terrain-graft source, the forced
  ``lod_mode="none"`` when LOD isn't regenerated, the selective ``swap_labels``
  passed to deploy, ``archive_output_dir`` forced off so the swap source is
  populated, and the SNAM stamp threaded onto the request.
"""
from pathlib import Path
from types import SimpleNamespace

import pytest
import yaml

from bacup_lib import PhaseSelection, regen_pipeline
from bacup_lib.family_map import UpgradePlan
from bacup_lib.regen_pipeline import (
    RegenOptions,
    RegenPaths,
    _clean_forced_regen_output,
    _resolve_upgrade_plan,
    _UpgradeNoOp,
)
from bacup_lib.source_pairs import get_pair


def _write_manifest(tmp_path: Path, body: str) -> Path:
    path = tmp_path / "upgrade_manifest.yaml"
    data = yaml.safe_load(body)
    for version in data["versions"]:
        families = version["families_by_conversion"]
        for pair_id in ("fo76:fo4", "fnvfo3:fo4", "skyrimse:fo4"):
            families.setdefault(pair_id, ["NONE"])
    path.write_text(yaml.safe_dump(data, sort_keys=False), encoding="utf-8")
    return path


_ALPHA1_ALPHA2 = """\
current: alpha2
versions:
  - id: alpha1
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [Meshes, Materials]
"""

_ALPHA1_ALPHA2_FORCED = """\
current: alpha2
versions:
  - id: alpha1
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [Meshes, Materials]
    force_regen_by_conversion:
      'fo76:fo4': true
"""

_ALPHA1_2_3 = """\
current: alpha3
versions:
  - id: alpha1
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [Meshes, Materials]
  - id: alpha3
    families_by_conversion:
      'fo76:fo4': [Terrain]
"""


def _paths(tmp_path: Path, *, deploy_data_dir: Path | None = None) -> RegenPaths:
    return RegenPaths(
        source_extracted_dir=tmp_path / "fo76_extracted",
        source_data_dir=tmp_path / "fo76" / "Data",
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=tmp_path / "mods" / "SeventySix",
        resource_dir=tmp_path / "resource",
        deploy_data_dir=deploy_data_dir,
    )


# --------------------------------------------------------------------------- #
# _resolve_upgrade_plan
# --------------------------------------------------------------------------- #


def test_not_upgrade_returns_none(tmp_path):
    assert _resolve_upgrade_plan(_paths(tmp_path), RegenOptions(upgrade=False)) is None


def test_upgrade_alpha1_to_alpha2_partial_plan(tmp_path, monkeypatch):
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)
    monkeypatch.setattr(
        "bacup_lib.version_stamp.read_plugin_snam", lambda _p: "alpha1"
    )
    options = RegenOptions(upgrade=True, upgrade_manifest_path=manifest)

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan)
    assert not plan.full_build
    assert not plan.regen_terrain
    ps = plan.phases
    assert ps.convert_nifs and ps.convert_npc_faces and ps.convert_materials
    assert ps.generate_anim_text_data
    assert not ps.convert_textures and not ps.convert_havok and not ps.convert_lod
    assert ps.convert_terrain  # always on (graft lives in this phase)
    assert ps.regenerate_modt  # Bucket B: never family-gated
    assert set(plan.swap_labels) == {"Meshes", "MeshesExtra", "Materials"}


def test_upgrade_plan_carries_forced_regen(tmp_path):
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2_FORCED)
    options = RegenOptions(
        upgrade=True,
        upgrade_from="alpha1",
        upgrade_manifest_path=manifest,
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan)
    assert plan.force_regen is True
    assert plan.full_build is True


def test_upgrade_none_scope_returns_pair_specific_no_op(tmp_path):
    manifest = _write_manifest(
        tmp_path,
        """\
current: alpha3
versions:
  - id: alpha2
    families_by_conversion:
      'skyrimse:fo4': [ALL]
  - id: alpha3
    families_by_conversion:
      'skyrimse:fo4': [NONE]
    force_regen_by_conversion:
      'skyrimse:fo4': false
""",
    )
    options = RegenOptions(
        upgrade=True,
        upgrade_from="alpha2",
        upgrade_manifest_path=manifest,
    )

    plan = _resolve_upgrade_plan(
        _paths(tmp_path),
        options,
        get_pair("skyrimse:fo4"),
    )

    assert isinstance(plan, _UpgradeNoOp)
    assert plan.target == "alpha3"
    assert plan.already_current is False


def test_pair_specific_force_regen_overrides_none_family_scope(tmp_path):
    manifest = _write_manifest(
        tmp_path,
        """\
current: alpha3
versions:
  - id: alpha2
    families_by_conversion:
      'skyrimse:fo4': [ALL]
  - id: alpha3
    families_by_conversion:
      'skyrimse:fo4': [NONE]
    force_regen_by_conversion:
      'skyrimse:fo4': true
""",
    )
    options = RegenOptions(
        upgrade=True,
        upgrade_from="alpha2",
        upgrade_manifest_path=manifest,
    )

    plan = _resolve_upgrade_plan(
        _paths(tmp_path),
        options,
        get_pair("skyrimse:fo4"),
    )

    assert isinstance(plan, UpgradePlan)
    assert plan.force_regen is True
    assert plan.full_build is True


def test_forced_regen_missing_extracted_dir_preserves_local_output(tmp_path):
    paths = _paths(tmp_path)
    paths.output_root.mkdir(parents=True)
    sentinel = paths.output_root / "previous-run.ba2"
    sentinel.write_bytes(b"keep until preflight passes")

    with pytest.raises(FileNotFoundError, match="extracted directory"):
        _clean_forced_regen_output(
            paths,
            SimpleNamespace(emit_log=lambda *_a, **_k: None),
        )

    assert sentinel.is_file()


def test_forced_regen_clears_only_local_output(tmp_path):
    paths = _paths(tmp_path)
    paths.source_extracted_dir.mkdir(parents=True)
    paths.output_root.mkdir(parents=True)
    (paths.output_root / "previous-run.ba2").write_bytes(b"stale")
    paths.target_data_dir.mkdir(parents=True)
    deployed = paths.target_data_dir / "SeventySix.esm"
    deployed.write_bytes(b"deployed")
    logs = []

    _clean_forced_regen_output(
        paths,
        SimpleNamespace(emit_log=lambda level, message: logs.append((level, message))),
    )

    assert not paths.output_root.exists()
    assert deployed.read_bytes() == b"deployed"
    assert logs and logs[0][0] == "INFO"


def test_forced_regen_refuses_output_inside_deploy_dir(tmp_path):
    paths = _paths(tmp_path, deploy_data_dir=tmp_path / "deployed")
    paths.source_extracted_dir.mkdir(parents=True)
    paths.deploy_data_dir.mkdir(parents=True)
    paths.output_root = paths.deploy_data_dir / "SeventySix"
    paths.output_root.mkdir()

    with pytest.raises(ValueError, match="protected conversion path"):
        _clean_forced_regen_output(
            paths,
            SimpleNamespace(emit_log=lambda *_a, **_k: None),
        )

    assert paths.output_root.is_dir()


def test_upgrade_without_meshes_disables_anim_text_data(tmp_path, monkeypatch):
    manifest = _write_manifest(
        tmp_path,
        """\
current: alpha2
versions:
  - id: alpha1
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [Materials]
""",
    )
    monkeypatch.setattr(
        "bacup_lib.version_stamp.read_plugin_snam", lambda _p: "alpha1"
    )

    plan = _resolve_upgrade_plan(
        _paths(tmp_path),
        RegenOptions(upgrade=True, upgrade_manifest_path=manifest),
    )

    assert isinstance(plan, UpgradePlan)
    assert not plan.phases.generate_anim_text_data
    options = regen_pipeline._build_options(
        records_only=False,
        conversion_workers=1,
        records_limit=None,
        phases=plan.phases,
        generate_anim_text_data=True,
    )
    assert not options.generate_anim_text_data


def test_no_deployed_esm_is_full_build(tmp_path, monkeypatch):
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)
    monkeypatch.setattr(
        "bacup_lib.version_stamp.read_plugin_snam", lambda _p: None
    )
    options = RegenOptions(upgrade=True, upgrade_manifest_path=manifest)

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan)
    assert plan.full_build
    assert plan.regen_terrain


def test_upgrade_from_override_beats_snam_read(tmp_path, monkeypatch):
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)

    def _boom(_p):
        raise AssertionError("read_plugin_snam must not run when upgrade_from is set")

    monkeypatch.setattr(
        "bacup_lib.version_stamp.read_plugin_snam", _boom
    )
    options = RegenOptions(
        upgrade=True, upgrade_manifest_path=manifest, upgrade_from="alpha1"
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan) and not plan.full_build
    assert set(plan.swap_labels) == {"Meshes", "MeshesExtra", "Materials"}


def test_from_equals_target_is_noop(tmp_path):
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)
    options = RegenOptions(
        upgrade=True, upgrade_manifest_path=manifest, upgrade_from="alpha2"
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, _UpgradeNoOp)
    assert plan.target == "alpha2"


def test_from_equals_target_repeats_scripts_when_target_declares_it(tmp_path):
    manifest = _write_manifest(
        tmp_path,
        """\
current: alpha2
versions:
  - id: alpha1
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [Scripts]
""",
    )
    options = RegenOptions(
        upgrade=True, upgrade_manifest_path=manifest, upgrade_from="alpha2"
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan)
    assert plan.phases.convert_scripts is True
    assert plan.swap_labels == ("Misc",)


def test_downgrade_raises(tmp_path):
    manifest = _write_manifest(tmp_path, _ALPHA1_2_3)
    options = RegenOptions(
        upgrade=True,
        upgrade_manifest_path=manifest,
        upgrade_from="alpha3",
        mod_version="alpha2",
    )

    with pytest.raises(ValueError, match="downgrade"):
        _resolve_upgrade_plan(_paths(tmp_path), options)


def test_missing_manifest_path_raises(tmp_path):
    with pytest.raises(ValueError, match="upgrade_manifest_path"):
        _resolve_upgrade_plan(_paths(tmp_path), RegenOptions(upgrade=True))


def test_missing_manifest_file_raises(tmp_path):
    options = RegenOptions(
        upgrade=True, upgrade_manifest_path=tmp_path / "nope.yaml"
    )
    with pytest.raises(FileNotFoundError):
        _resolve_upgrade_plan(_paths(tmp_path), options)


def test_target_override_multi_step_union(tmp_path):
    manifest = _write_manifest(tmp_path, _ALPHA1_2_3)
    options = RegenOptions(
        upgrade=True,
        upgrade_manifest_path=manifest,
        upgrade_from="alpha1",
        mod_version="alpha3",
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan) and not plan.full_build
    assert plan.regen_terrain  # Terrain entered the union via alpha3
    assert set(plan.swap_labels) == {
        "Meshes", "MeshesExtra", "Materials", "Terrain", "TerrainTextures",
    }


# --------------------------------------------------------------------------- #
# run_full_regen upgrade wiring
# --------------------------------------------------------------------------- #


def _stub_pipeline(monkeypatch):
    """Neutralize the heavy pieces of run_full_regen so only the wiring runs."""
    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_snapshot_land_cache", lambda *_a, **_k: True)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_payloads_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_sanitize_fo4_ck_materials_for_outputs", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", lambda *_a, **_k: ([], []))

    import bacup_lib.models as models

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)


def test_run_full_regen_upgrade_wires_phases_graft_swap_and_stamp(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    # FO76 source (resolved by _resolve_source_plugins).
    paths.source_data_dir.mkdir(parents=True)
    (paths.source_data_dir / "SeventySix.esm").write_bytes(b"TES4-source")
    # Live deployed ESM -> readable graft source for terrain reuse.
    paths.target_data_dir.mkdir(parents=True)
    (paths.target_data_dir / "SeventySix.esm").write_bytes(b"TES4-deployed")

    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)

    _stub_pipeline(monkeypatch)

    captures: dict[str, object] = {}
    deploy_kwargs: list[dict] = []

    def fake_run_unified(request, _runner, **kwargs):
        opts = request.options
        captures["convert_nifs"] = opts.convert_nifs
        captures["convert_materials"] = opts.convert_materials
        captures["convert_textures"] = opts.convert_textures
        captures["convert_havok"] = opts.convert_havok
        captures["convert_terrain"] = opts.convert_terrain
        captures["reuse_terrain_navmesh"] = opts.reuse_terrain_navmesh
        captures["terrain_graft_esm"] = opts.terrain_graft_esm
        captures["synthesize_object_lod"] = opts.synthesize_object_lod
        captures["terrain_lod_mode"] = opts.terrain.lod_mode
        captures["mod_version"] = getattr(request, "mod_version", "<unset>")
        captures["archive_output_dir"] = kwargs.get("archive_output_dir")
        captures["lod_hook"] = kwargs.get("lod_hook")
        paths.output_root.mkdir(parents=True, exist_ok=True)
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4-built")
        return SimpleNamespace(
            run_result=SimpleNamespace(
                decisions=[],
                translated_counts={},
                skipped_counts={},
                failed_nifs=[],
                failed_textures=[],
                failed_bgsms=[],
                btos_failed=0,
                btos_total=0,
            )
        )

    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(unified, "run_unified", fake_run_unified)
    monkeypatch.setattr(
        regen_pipeline,
        "_deploy_post_steps",
        lambda *a, **kwargs: deploy_kwargs.append(kwargs),
    )

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(
            deploy=True,
            upgrade=True,
            upgrade_from="alpha1",
            upgrade_manifest_path=manifest,
            lod_mode="generate",           # explicit mode -> must still be forced off
            direct_deploy_archives=True,   # must be forced to pack into output_root
        ),
        phases=PhaseSelection(),           # replaced by the plan's phases
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    # Phase closure: Meshes + Materials only.
    assert captures["convert_nifs"] is True
    assert captures["convert_materials"] is True
    assert captures["convert_textures"] is False
    assert captures["convert_havok"] is False
    assert captures["convert_terrain"] is True  # graft rides this phase
    # Terrain reuse via the live deployed ESM.
    assert captures["reuse_terrain_navmesh"] is True
    assert captures["terrain_graft_esm"] == paths.target_data_dir / "SeventySix.esm"
    # LOD not regenerated -> lodgen fully skipped (no hook, no synth, lod_mode none).
    assert captures["lod_hook"] is None
    assert captures["synthesize_object_lod"] is False
    assert captures["terrain_lod_mode"] == "none"
    # SNAM stamp target threaded onto the request.
    assert captures["mod_version"] == "alpha2"
    # Selective swap must pack into output_root, not direct-deploy.
    assert captures["archive_output_dir"] is None
    assert deploy_kwargs == [
        {
            "archives_already_deployed": False,
            "update_runtime_ini": True,
            "swap_labels": ("Materials", "Meshes", "MeshesExtra"),
        }
    ]


def test_run_full_regen_upgrade_noop_short_circuits(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)
    _stub_pipeline(monkeypatch)

    called: list[str] = []
    import bacup_lib.workflows.unified as unified

    monkeypatch.setattr(
        unified, "run_unified", lambda *a, **k: called.append("run_unified")
    )
    monkeypatch.setattr(
        regen_pipeline, "_deploy_post_steps", lambda *a, **k: called.append("deploy")
    )

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(
            deploy=True,
            upgrade=True,
            upgrade_from="alpha2",  # == target (manifest.current)
            upgrade_manifest_path=manifest,
        ),
        phases=PhaseSelection(),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert result.deployed is False
    assert called == []  # neither conversion nor deploy ran
