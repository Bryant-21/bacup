from types import SimpleNamespace

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenOptions


def test_lod_discovery_materializes_only_catalog_owned_dependency_closure(
    tmp_path, monkeypatch
):
    class FakeStore:
        cache_data_root = tmp_path / "cache" / "Data"

        def __init__(self):
            self.materialized = []
            self.owned = {
                "meshes/base/tree_lod.nif",
                "materials/base/tree.bgsm",
                "textures/base/tree_d.dds",
            }

        def has_asset(self, path):
            return path in self.owned

        def dependency_closure(self, roots):
            assert set(roots) == {"meshes/base/tree_lod.nif"}
            return sorted(self.owned)

        def materialize_many(self, paths):
            self.materialized = list(paths)

    class FakePlugin:
        def collect_assets(self):
            return [
                {"source_path": r"Meshes\Base\Tree_LOD.nif"},
                {"source_path": r"Meshes\Converted\OnlyInMod.nif"},
            ]

        def close(self):
            pass

    store = FakeStore()
    monkeypatch.setattr(
        "bacup_lib.target_assets.build_target_asset_store",
        lambda **_kwargs: store,
    )
    monkeypatch.setattr(
        "creation_lib.esp.plugin.Plugin.load", lambda *_args, **_kwargs: FakePlugin()
    )
    paths = SimpleNamespace(
        target_data_dir=tmp_path / "Data",
        target_asset_catalog_path=tmp_path / "catalog.sqlite3",
        target_asset_cache_dir=tmp_path / "cache",
        target_extracted_dir=None,
    )

    result = regen_pipeline._target_lod_asset_dirs(
        paths,
        "hybrid-atlas",
        working_esm=tmp_path / "SeventySix.esm",
    )

    assert result == [store.cache_data_root]
    assert store.materialized == sorted(store.owned)


def test_cross_game_lod_discovers_worldspaces_instead_of_using_appalachia():
    for pair_id in ("fnvfo3:fo4", "skyrimse:fo4"):
        prepared = regen_pipeline._prepare_lod_generation_settings(
            RegenOptions(lod_mode="generate"),
            {
                "global": {"worldspaces": ["APPALACHIA"], "stride": 128},
                "objects": {"source": "fo76_bto"},
            },
            None,
            1,
            pair_id=pair_id,
        )

        assert prepared is not None
        worldspaces, settings, uses_fo76_bto, discover_worldspaces = prepared
        assert worldspaces == []
        assert settings["global"]["worldspaces"] == []
        assert settings["global"]["stride"] is None
        assert settings["objects"]["source"] == "records"
        assert uses_fo76_bto is False
        assert discover_worldspaces is True


def test_fo76_lod_keeps_configured_appalachia_worldspace():
    prepared = regen_pipeline._prepare_lod_generation_settings(
        RegenOptions(lod_mode="generate"),
        {
            "global": {"worldspaces": ["APPALACHIA"], "stride": 128},
            "objects": {"source": "records"},
        },
        None,
        4,
        pair_id="fo76:fo4",
    )

    assert prepared is not None
    worldspaces, settings, uses_fo76_bto, discover_worldspaces = prepared
    assert worldspaces == ["APPALACHIA"]
    assert settings["global"]["worldspaces"] == ["APPALACHIA"]
    assert settings["global"]["stride"] == 128
    assert settings["global"]["workers"] == 4
    assert uses_fo76_bto is False
    assert discover_worldspaces is False


def test_cross_game_lod_discovers_from_fresh_output_plugin(tmp_path, monkeypatch):
    working_esm = tmp_path / "FNV_FO3_Merged.esm"
    working_esm.write_bytes(b"TES4")
    calls = []

    monkeypatch.setattr(
        "creation_lib.lod.native_runtime.discover_worldspaces",
        lambda plugin_path, *, game: calls.append((plugin_path, game))
        or ("WastelandNV", "Wasteland", "wastelandnv", ""),
    )

    worldspaces = regen_pipeline._resolve_lod_worldspaces(
        [],
        discover_from_plugin=True,
        working_esm=working_esm,
        runner_log=lambda *_args: None,
    )

    assert worldspaces == ["WastelandNV", "Wasteland"]
    assert "APPALACHIA" not in worldspaces
    assert calls == [(working_esm, "fo4")]
