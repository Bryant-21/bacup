from types import SimpleNamespace

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenOptions


def test_lod_discovery_uses_extracted_fo4_without_opening_ba2s(tmp_path, monkeypatch):
    overlay = tmp_path / "extracted" / "fo4"
    overlay.mkdir(parents=True)
    monkeypatch.setattr(
        "bacup_lib.target_assets.build_target_asset_store",
        lambda **_kwargs: (_ for _ in ()).throw(
            AssertionError("extracted FO4 assets must suppress the BA2 fallback")
        ),
    )
    paths = SimpleNamespace(target_extracted_dir=overlay)

    result = regen_pipeline._target_lod_asset_dirs(
        paths,
        "hybrid-atlas",
        working_esm=tmp_path / "SeventySix.esm",
    )

    assert result == [overlay]


def test_lod_discovery_uses_ba2s_without_materializing_when_extracted_fo4_is_missing(
    tmp_path, monkeypatch
):
    class FakeStore:
        archive_paths = (
            tmp_path / "Data" / "Fallout4 - Main.ba2",
            tmp_path / "Data" / "DLCCoast - Main.ba2",
        )

        def materialize_many(self, _paths):
            raise AssertionError("LOD must read target assets directly from BA2s")

    store = FakeStore()
    build_kwargs = {}

    def build_store(**kwargs):
        build_kwargs.update(kwargs)
        return store

    monkeypatch.setattr(
        "bacup_lib.target_assets.build_target_asset_store",
        build_store,
    )
    paths = SimpleNamespace(
        target_data_dir=tmp_path / "Data",
        target_asset_catalog_path=tmp_path / "catalog.sqlite3",
        target_asset_cache_dir=tmp_path / "cache",
        target_extracted_dir=tmp_path / "missing" / "fo4",
    )

    result = regen_pipeline._target_lod_asset_dirs(
        paths,
        "hybrid-atlas",
        working_esm=tmp_path / "SeventySix.esm",
    )

    assert result == [*store.archive_paths]
    assert build_kwargs["overlay_dir"] is None


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
