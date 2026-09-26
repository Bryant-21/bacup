"""Upgrade-plan resolution + run_full_regen upgrade wiring.

Two seams are pinned here:

* ``_resolve_upgrade_plan`` -- manifest + from-version -> UpgradePlan / no-op /
  full-build. ``read_plugin_snam`` is monkeypatched so no rebuilt .pyd is needed;
  the manifest is a real on-disk YAML (pure ``upgrade_manifest`` parsing).
* ``run_full_regen`` in upgrade mode -- the derived wiring that the resolver does
  NOT itself carry: ``re_use_land`` / terrain-graft source, the forced
  ``lod_mode="none"`` when LOD isn't regenerated, UI workspace hydration,
  complete archive packing/deployment, and the SNAM stamp threaded onto the
  request.
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


@pytest.mark.parametrize("snam_or_override", ["snam", "override"])
def test_upgrade_alpha1_to_alpha2_partial_plan(tmp_path, monkeypatch, snam_or_override):
    manifest = _write_manifest(tmp_path, _ALPHA1_ALPHA2)
    if snam_or_override == "snam":
        monkeypatch.setattr(
            "bacup_lib.version_stamp.read_plugin_snam", lambda _p: "alpha1"
        )
        options = RegenOptions(upgrade=True, upgrade_manifest_path=manifest)
    else:
        def _boom(_p):
            raise AssertionError("read_plugin_snam must not run when upgrade_from is set")

        monkeypatch.setattr("bacup_lib.version_stamp.read_plugin_snam", _boom)
        options = RegenOptions(
            upgrade=True, upgrade_manifest_path=manifest, upgrade_from="alpha1"
        )

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


@pytest.mark.parametrize(
    ("body", "pair_id", "from_id"),
    [
        (_ALPHA1_ALPHA2_FORCED, "fo76:fo4", "alpha1"),
        (
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
            "skyrimse:fo4",
            "alpha2",
        ),
    ],
)
def test_upgrade_plan_carries_forced_regen(tmp_path, body, pair_id, from_id):
    manifest = _write_manifest(tmp_path, body)
    options = RegenOptions(
        upgrade=True,
        upgrade_from=from_id,
        upgrade_manifest_path=manifest,
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options, get_pair(pair_id))

    assert isinstance(plan, UpgradePlan)
    assert plan.force_regen is True
    assert plan.full_build is True


def test_all_upgrade_rejected_before_workspace_hydration(tmp_path, monkeypatch):
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
      'fo76:fo4': [ALL]
""",
    )
    paths = _paths(tmp_path)
    monkeypatch.setattr(
        regen_pipeline,
        "_hydrate_upgrade_workspace_from_deployed",
        lambda *_args, **_kwargs: pytest.fail("deployed archives must not be restored"),
    )

    with pytest.raises(ValueError, match="requires a full conversion"):
        regen_pipeline.run_full_regen(
            paths,
            RegenOptions(
                upgrade=True,
                hydrate_upgrade_from_deployed=True,
                upgrade_from="alpha1",
                upgrade_manifest_path=manifest,
            ),
            phases=PhaseSelection(),
            runner=SimpleNamespace(emit_log=lambda *_args, **_kwargs: None),
        )

    assert not paths.output_root.exists()


@pytest.mark.parametrize(
    ("body", "pair_id", "from_id", "target", "already_current"),
    [
        (
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
            "skyrimse:fo4",
            "alpha2",
            "alpha3",
            False,
        ),
        (
            """\
current: alpha2
versions:
  - id: alpha1
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [NONE]
""",
            "fo76:fo4",
            "alpha2",
            "alpha2",
            True,
        ),
    ],
)
def test_upgrade_none_scope_returns_no_op(
    tmp_path, body, pair_id, from_id, target, already_current
):
    manifest = _write_manifest(tmp_path, body)
    options = RegenOptions(
        upgrade=True,
        upgrade_from=from_id,
        upgrade_manifest_path=manifest,
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options, get_pair(pair_id))

    assert isinstance(plan, _UpgradeNoOp)
    assert plan.target == target
    assert plan.already_current is already_current


@pytest.mark.parametrize("unsafe", ["missing_extracted", "inside_deploy_dir"])
def test_forced_regen_refuses_unsafe_clean_and_preserves_output(tmp_path, unsafe):
    if unsafe == "missing_extracted":
        paths = _paths(tmp_path)
        paths.output_root.mkdir(parents=True)
        error, match = FileNotFoundError, "extracted directory"
    else:
        paths = _paths(tmp_path, deploy_data_dir=tmp_path / "deployed")
        paths.source_extracted_dir.mkdir(parents=True)
        paths.deploy_data_dir.mkdir(parents=True)
        paths.output_root = paths.deploy_data_dir / "SeventySix"
        paths.output_root.mkdir()
        error, match = ValueError, "protected conversion path"
    sentinel = paths.output_root / "previous-run.ba2"
    sentinel.write_bytes(b"keep until preflight passes")

    with pytest.raises(error, match=match):
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


@pytest.mark.parametrize(
    ("families", "enabled_phases", "swap_labels"),
    [
        ("[Scripts]", ("convert_scripts",), {"Misc"}),
        (
            "[NIFs, Havok]",
            ("convert_nifs", "convert_havok", "synthesize_drivers", "generate_anim_text_data"),
            {"Meshes", "MeshesExtra", "Animations"},
        ),
        ("[LOD]", ("convert_lod",), {"LOD", "LODTextures"}),
    ],
)
def test_from_equals_target_repeats_target_families(
    tmp_path, families, enabled_phases, swap_labels
):
    manifest = _write_manifest(
        tmp_path,
        f"""\
current: alpha2.1
versions:
  - id: alpha2
    families_by_conversion:
      'fo76:fo4': [ALL]
  - id: alpha2.1
    families_by_conversion:
      'fo76:fo4': {families}
""",
    )
    options = RegenOptions(
        upgrade=True, upgrade_manifest_path=manifest, upgrade_from="alpha2.1"
    )

    plan = _resolve_upgrade_plan(_paths(tmp_path), options)

    assert isinstance(plan, UpgradePlan)
    for phase in enabled_phases:
        assert getattr(plan.phases, phase) is True, phase
    assert plan.phases.convert_animations is False
    assert set(plan.swap_labels) == swap_labels


@pytest.mark.parametrize(
    ("option_kwargs", "error", "match"),
    [
        ({"upgrade_from": "alpha3", "mod_version": "alpha2"}, ValueError, "downgrade"),
        ({"upgrade_manifest_path": None}, ValueError, "upgrade_manifest_path"),
        ({"upgrade_manifest_path": "missing"}, FileNotFoundError, None),
    ],
)
def test_invalid_upgrade_request_raises(tmp_path, option_kwargs, error, match):
    kwargs = {"upgrade_manifest_path": _write_manifest(tmp_path, _ALPHA1_2_3)}
    kwargs.update(option_kwargs)
    if kwargs["upgrade_manifest_path"] == "missing":
        kwargs["upgrade_manifest_path"] = tmp_path / "nope.yaml"

    with pytest.raises(error, match=match):
        _resolve_upgrade_plan(_paths(tmp_path), RegenOptions(upgrade=True, **kwargs))


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
        "Meshes", "MeshesExtra", "Materials", "Textures",
    }


# --------------------------------------------------------------------------- #
# run_full_regen upgrade wiring
# --------------------------------------------------------------------------- #


def _stub_pipeline(monkeypatch):
    """Neutralize the heavy pieces of run_full_regen so only the wiring runs."""
    monkeypatch.setattr(regen_pipeline, "_effective_conversion_workers", lambda _v: 1)
    monkeypatch.setattr(regen_pipeline, "_snapshot_land_cache", lambda *_a, **_k: True)
    monkeypatch.setattr(regen_pipeline, "_write_conversion_reports", lambda *_a, **_k: None)
    monkeypatch.setattr(regen_pipeline, "_check_run_invariants", lambda *_a, **_k: ([], []))
    monkeypatch.setattr(
        "bacup_lib.target_assets.ensure_target_asset_catalog",
        lambda *_a, **_k: None,
    )

    import bacup_lib.models as models

    monkeypatch.setattr(models, "write_coverage_report", lambda *_a, **_k: None)


def test_run_full_regen_honors_explicit_anim_text_data_for_unrelated_upgrade_family(
    monkeypatch, tmp_path
):
    paths = _paths(tmp_path)
    paths.source_data_dir.mkdir(parents=True)
    (paths.source_data_dir / "SeventySix.esm").write_bytes(b"TES4-source")
    paths.target_data_dir.mkdir(parents=True)
    (paths.target_data_dir / "SeventySix.esm").write_bytes(b"TES4-deployed")
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
    _stub_pipeline(monkeypatch)

    captures = {}

    def fake_run_unified(request, _runner, **kwargs):
        captures["generate_anim_text_data"] = request.options.generate_anim_text_data
        captures["anim_text_data_native"] = request.options.anim_text_data_native
        captures["archive_labels"] = kwargs.get("archive_labels")
        paths.output_root.mkdir(parents=True, exist_ok=True)
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4-built")
        return SimpleNamespace(
            summary=SimpleNamespace(),
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

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(
            upgrade=True,
            upgrade_from="alpha1",
            upgrade_manifest_path=manifest,
            generate_anim_text_data=True,
            anim_text_data_native=True,
        ),
        phases=PhaseSelection(),
        runner=SimpleNamespace(emit_log=lambda *_a, **_k: None),
    )

    assert result.exit_code == 0
    assert captures["generate_anim_text_data"] is True
    assert captures["anim_text_data_native"] is True
    assert set(captures["archive_labels"]) == {"Materials", "Meshes", "MeshesExtra"}


def test_run_full_regen_hydrated_upgrade_repacks_and_deploys_the_complete_mod(
    monkeypatch, tmp_path
):
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
    calls: list[str] = []
    monkeypatch.setattr(
        regen_pipeline,
        "_hydrate_upgrade_workspace_from_deployed",
        lambda *_a, **_k: calls.append("hydrate"),
        raising=False,
    )

    def fake_run_unified(request, _runner, **kwargs):
        calls.append("convert")
        opts = request.options
        captures["convert_nifs"] = opts.convert_nifs
        captures["convert_materials"] = opts.convert_materials
        captures["convert_textures"] = opts.convert_textures
        captures["convert_havok"] = opts.convert_havok
        captures["generate_anim_text_data"] = opts.generate_anim_text_data
        captures["convert_terrain"] = opts.convert_terrain
        captures["reuse_terrain_navmesh"] = opts.reuse_terrain_navmesh
        captures["terrain_graft_esm"] = opts.terrain_graft_esm
        captures["synthesize_object_lod"] = opts.synthesize_object_lod
        captures["terrain_lod_mode"] = opts.terrain.lod_mode
        captures["mod_version"] = getattr(request, "mod_version", "<unset>")
        captures["archive_output_dir"] = kwargs.get("archive_output_dir")
        captures["archive_labels"] = kwargs.get("archive_labels")
        captures["lod_hook"] = kwargs.get("lod_hook")
        paths.output_root.mkdir(parents=True, exist_ok=True)
        (paths.output_root / "SeventySix.esm").write_bytes(b"TES4-built")
        return SimpleNamespace(
            summary=SimpleNamespace(),
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
        lambda *a, **kwargs: (
            calls.append("deploy"),
            deploy_kwargs.append(kwargs),
        ),
    )

    result = regen_pipeline.run_full_regen(
        paths,
        RegenOptions(
            deploy=True,
            upgrade=True,
            hydrate_upgrade_from_deployed=True,
            upgrade_from="alpha1",
            upgrade_manifest_path=manifest,
            lod_mode="generate",           # explicit mode -> must still be forced off
            direct_deploy_archives=True,   # hydration must still pack into output_root
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
    assert captures["generate_anim_text_data"] is True
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
    # The deployed archives seed the local workspace before conversion. The
    # complete inventory is then packed locally and fully deployed.
    assert calls == ["hydrate", "convert", "deploy"]
    assert captures["archive_output_dir"] is None
    assert captures["archive_labels"] is None
    assert deploy_kwargs == [
        {
            "archives_already_deployed": False,
            "update_runtime_ini": True,
            "register_runtime_archives": False,
        }
    ]


def test_run_full_regen_upgrade_noop_short_circuits(monkeypatch, tmp_path):
    paths = _paths(tmp_path)
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
      'fo76:fo4': [NONE]
""",
    )
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
