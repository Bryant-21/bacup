from dataclasses import asdict
from types import SimpleNamespace

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.native_runtime import load_native_module
from bacup_lib.workflows.unified import AssetRuns, _UnifiedRecordRuntime


def fixture_runtime(tmp_path, game):
    primary = tmp_path / "primary"
    secondary = tmp_path / "secondary"
    source = tmp_path / "plugins" / "Merged.esm"
    source.parent.mkdir()
    source.write_bytes(b"fixture")
    files = [
        (primary, "Meshes/Clutter/Shared.NIF"),
        (secondary, "Meshes/Clutter/Shared.NIF"),
        (secondary, "Data/Meshes/Clutter/Secondary.nif"),
        (primary, "Meshes/SCOL/Original.esm/shape.nif"),
        (primary, "Meshes/Actors/Test/CharacterAssets/body.nif"),
        (primary, "Meshes/Actors/Test/CharacterAssets/skeleton.nif"),
        (primary, "Meshes/Actors/Test/Test.hkx"),
        (primary, "Meshes/Actors/Test/Behaviors/graph.hkx"),
        (primary, "Meshes/Actors/Test/Animations/idle.HKX"),
        (primary, "Meshes/Elsewhere/unique.nif"),
        (primary, "Meshes/Elsewhere/ambiguous.nif"),
        (primary, "Meshes/Other/ambiguous.nif"),
        (primary, "Sound/FX/Alternative.xwm"),
        (primary, "Sound/FX/Nested/other.wav"),
        (primary, "Sound/Voice/Merged.esm/TypeA/line.fuz"),
        (secondary, "Sound/Voice/Merged.esm/TypeA/line.fuz"),
        (secondary, "Sound/Voice/Merged.esm/TypeB/extra.wav"),
        (primary, "Music/theme.xwm"),
        (primary, "Textures/test.dds"),
        (primary, "Materials/test.bgsm"),
    ]
    for root, relative in files:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(str(root).encode())
    (primary / "Sound/FX/empty").mkdir()
    (primary / "Meshes/directory.nif").mkdir()
    request = PluginPortRequest(
        source_game=game,
        target_game="fo4",
        source_plugins=[source],
        source_data_dir=primary,
        additional_source_asset_roots=(secondary,),
        output_root=tmp_path / "out",
        options=PluginPortOptions(
            convert_nifs=True,
            convert_textures=True,
            convert_materials=True,
            convert_havok=True,
            copy_sounds=True,
        ),
    )
    ctx = SimpleNamespace(
        source_game=game,
        target_game="fo4",
        source_data_dir=primary,
        additional_source_asset_roots=(secondary,),
        is_whole_plugin=True,
        output_plugin_name="Merged.esm",
        conversion_workers=3,
    )
    paths = [
        ("nif", "Meshes/Clutter/Shared.NIF"),
        ("nif", "meshes/clutter/shared.nif"),
        ("nif", "Clutter/Secondary.nif"),
        ("nif", "Meshes/SCOL/Merged.esm/shape.nif"),
        ("nif", "Actors/Test/CharacterAssets/body.nif"),
        ("nif", "Wrong/unique.nif"),
        ("nif", "Wrong/ambiguous.nif"),
        ("nif", "missing.nif"),
        ("behavior", "Actors/Test/Test.hkx"),
        ("sound", "FX/Alternative.wav"),
        ("sound", "Sound/FX/empty"),
        ("sound", "missing.wav"),
        ("texture", "test.dds"),
        ("material", "test.bgsm"),
        ("nif", str(primary / "Meshes/Clutter/Shared.NIF")),
    ]
    raw = [
        dict(
            asset_type=kind,
            source_path=path,
            source_form_key="000800:Original.esm",
            source_record_signature="STAT",
            source_subrecord_sig="MODL",
            workshop_wire_point=[1, 2, 3],
        )
        for kind, path in paths
    ]
    ctx.source_plugin_handle = SimpleNamespace(collect_assets=lambda **kwargs: raw)
    return _UnifiedRecordRuntime(request), source, ctx, raw


@pytest.mark.parametrize("game", ["fo76", "skyrimse", "fnv", "fo3"])
def test_inventory_preserves_complete_collection_and_provenance(tmp_path, game):
    runtime, source, ctx, raw = fixture_runtime(tmp_path, game)
    expected = runtime._resolve_and_expand_native_assets(raw, source, ctx)
    actual = runtime._collect_assets_native(source, ctx)
    assert [asdict(asset) for asset in actual] == [asdict(asset) for asset in expected]
    assert runtime._source_asset_inventory is None
    assert ctx.source_asset_inventory is not None
    stats = ctx.source_asset_inventory.stats()
    assert stats["directory_reads"] > 0
    assert stats["retained_path_bytes_lower_bound"] > 0
    assert runtime._legacy_nif_basename_indexes == {}
    shared = next(
        asset for asset in actual if asset.source_path == "Meshes/Clutter/Shared.NIF"
    )
    assert shared.resolved_path.startswith(str(ctx.source_data_dir))
    missing = next(asset for asset in actual if asset.source_path == "missing.nif")
    assert missing.resolved_path is None
    assert missing.resolution_error


def test_new_collection_sees_source_creation_and_overwrites(tmp_path):
    runtime, source, ctx, _ = fixture_runtime(tmp_path, "fo76")
    before = runtime._collect_assets_native(source, ctx)
    before_inventory = ctx.source_asset_inventory
    missing = ctx.source_data_dir / "Meshes/missing.nif"
    missing.write_bytes(b"new source")
    after = runtime._collect_assets_native(source, ctx)
    assert ctx.source_asset_inventory is not before_inventory
    assert (
        next(
            asset for asset in before if asset.source_path == "missing.nif"
        ).resolved_path
        is None
    )
    assert next(
        asset for asset in after if asset.source_path == "missing.nif"
    ).resolved_path == str(missing)
    missing.write_bytes(b"overwritten source")
    final = runtime._collect_assets_native(source, ctx)
    assert next(
        asset for asset in final if asset.source_path == "missing.nif"
    ).resolved_path == str(missing)
    assert missing.read_bytes() == b"overwritten source"


def test_failed_collection_releases_inventory(tmp_path, monkeypatch):
    runtime, source, ctx, _ = fixture_runtime(tmp_path, "fo76")

    def fail(*args):
        assert runtime._source_asset_inventory is not None
        raise RuntimeError("fixture failure")

    monkeypatch.setattr(runtime, "_resolve_and_expand_native_assets", fail)
    with pytest.raises(RuntimeError, match="fixture failure"):
        runtime._collect_assets_native(source, ctx)
    assert runtime._source_asset_inventory is None
    assert ctx.source_asset_inventory is None


@pytest.mark.parametrize("whole_plugin", [False, True])
def test_collection_does_not_retain_inventory_without_full_tree_consumers(
    tmp_path, whole_plugin
):
    runtime, source, ctx, _ = fixture_runtime(tmp_path, "fo76")
    ctx.is_whole_plugin = whole_plugin
    for option in ("convert_textures", "convert_materials", "convert_havok"):
        setattr(runtime._req.options, option, False)

    runtime._collect_assets_native(source, ctx)

    assert ctx.source_asset_inventory is None


def test_asset_run_construction_failure_closes_prior_runs_and_releases_inventory(
    monkeypatch,
):
    from bacup_lib.run import ConversionRun

    runs = []

    class FakeRun:
        def __init__(self, run_id):
            self.id = run_id
            self.closed = False

        def close(self):
            self.closed = True

    class FailingInventory:
        def attach_run(self, run_id):
            if run_id == 2:
                raise RuntimeError("attach failed")

    def create_new(*args, **kwargs):
        run = FakeRun(len(runs) + 1)
        runs.append(run)
        return run

    monkeypatch.setattr(ConversionRun, "create_new", create_new)
    ctx = SimpleNamespace(
        source_game="fo76",
        target_game="fo4",
        source_asset_inventory=FailingInventory(),
        is_whole_plugin=True,
        output_plugin_name="Output.esm",
    )
    toggles = SimpleNamespace(
        textures=True,
        materials=False,
        nifs=False,
        btos=False,
        havok=True,
        drivers=False,
        sounds=False,
    )

    with pytest.raises(RuntimeError, match="attach failed"):
        AssetRuns(ctx, toggles)

    assert [run.closed for run in runs] == [True, True]
    assert ctx.source_asset_inventory is None


def test_asset_runs_attach_inventory_and_release_it_after_close(monkeypatch):
    from bacup_lib.run import ConversionRun

    runs = []
    attached = []

    class FakeRun:
        def __init__(self, run_id):
            self.id = run_id
            self.closed = False

        def close(self):
            self.closed = True

    class Inventory:
        def attach_run(self, run_id):
            attached.append(run_id)

    def create_new(*args, **kwargs):
        run = FakeRun(len(runs) + 1)
        runs.append(run)
        return run

    monkeypatch.setattr(ConversionRun, "create_new", create_new)
    inventory = Inventory()
    ctx = SimpleNamespace(
        source_game="fo76",
        target_game="fo4",
        source_asset_inventory=inventory,
        is_whole_plugin=True,
        output_plugin_name="Output.esm",
    )
    toggles = SimpleNamespace(
        textures=True,
        materials=True,
        nifs=False,
        btos=False,
        havok=True,
        drivers=False,
        sounds=False,
    )

    asset_runs = AssetRuns(ctx, toggles)
    assert attached == [1, 2]
    assert ctx.source_asset_inventory is inventory

    asset_runs.drop_all()

    assert [run.closed for run in runs] == [True, True]
    assert ctx.source_asset_inventory is None
    assert asset_runs._source_asset_inventory is None


def test_inventory_link_policy_matches_each_discovery_walker(tmp_path):
    from bacup_lib.behavior.deps import expand_behavior_bundle
    from bacup_lib.models import AssetRef

    runtime, source, ctx, _ = fixture_runtime(tmp_path, "fo76")
    base = ctx.source_data_dir / "Meshes/Actors/Test"
    try:
        (base / "linked.hkx").symlink_to(base / "Test.hkx")
        (base / "broken.hkx").symlink_to(base / "missing.hkx")
        (base / "cycle").symlink_to(base, target_is_directory=True)
    except OSError as error:
        pytest.skip(f"Host cannot create test symlinks: {error}")
    inventory = load_native_module().SourceAssetInventory()
    project = AssetRef("behavior", "Actors/Test/Test.hkx")
    expected = expand_behavior_bundle(project, str(ctx.source_data_dir))
    actual = expand_behavior_bundle(
        project, str(ctx.source_data_dir), inventory=inventory
    )
    assert [asdict(asset) for asset in actual] == [asdict(asset) for asset in expected]
    assert any(asset.source_path.endswith("broken.hkx") for asset in actual)
    assert len(inventory.files(str(base), [".hkx"], True)) == 4
