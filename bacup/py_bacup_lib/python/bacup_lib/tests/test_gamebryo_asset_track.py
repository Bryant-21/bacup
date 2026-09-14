from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.models import AssetRef, ConversionSummary
from bacup_lib.workflows import asset_phases
from bacup_lib.workflows.unified import (
    AssetWaveBuilder,
    AssetWaveToggles,
    _merge_wave_report_into_summary,
)
from creation_lib.core.game_profiles import get_profile


class _Runner:
    def __init__(self):
        self.logs = []

    def emit_log(self, level, message):
        self.logs.append((level, message))


def _asset(asset_type: str, source_path: str, resolved_path: Path | None = None):
    return AssetRef(
        asset_type=asset_type,
        source_path=source_path,
        resolved_path=str(resolved_path) if resolved_path is not None else None,
    )


def _builder(monkeypatch, tmp_path: Path, source_game: str):
    source_root = tmp_path / "source"
    nif_path = source_root / "Meshes" / "clutter" / "crate.nif"
    egt_path = source_root / "Meshes" / "characters" / "body.egt"
    base_nif_path = source_root / "Meshes" / "clutter" / "base.nif"
    texture_path = source_root / "Textures" / "clutter" / "crate_d.dds"
    base_texture_path = source_root / "Textures" / "clutter" / "base_d.dds"
    sound_path = source_root / "Sound" / "fx" / "crate.wav"
    for path in (
        nif_path,
        egt_path,
        base_nif_path,
        texture_path,
        base_texture_path,
        sound_path,
    ):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"asset")

    assets = [
        _asset("nif", "Meshes/clutter/crate.nif", nif_path),
        _asset("nif", "Meshes/clutter/base.nif", base_nif_path),
        _asset("nif", "Meshes/clutter/missing.nif"),
        _asset("nif", "Meshes/characters/body.egt", egt_path),
        _asset("texture", "Textures/clutter/crate_d.dds", texture_path),
        _asset("texture", "Textures/clutter/base_d.dds", base_texture_path),
        _asset("texture", "Textures/clutter/missing_d.dds"),
        _asset("material", "Materials/clutter/crate.bgsm"),
        _asset("sound", "Sound/fx/crate.wav", sound_path),
    ]
    mod_path = tmp_path / "mods" / "MojaveCapital"
    summary = ConversionSummary()
    shim = SimpleNamespace(
        source_game=source_game,
        target_game="fo4",
        mod_path=str(mod_path),
        source_data_dir=str(source_root),
        target_extracted_dir=str(tmp_path / "target"),
        target_data_dir=str(tmp_path / "Fallout4" / "Data"),
        graph=SimpleNamespace(all_assets=assets, all_records=[]),
        _summary=summary,
        _source_profile=get_profile(source_game),
        _target_profile=get_profile("fo4"),
        _addon_index_map={},
        convert_precombined_nifs=True,
        overwrite_existing=True,
        conversion_workers=None,
        disable_nif_collision_memo=False,
        _target_has_asset=lambda asset: Path(asset.source_path).stem.startswith("base"),
        _remove_stale_asset_output=lambda _asset: False,
        _track_asset=lambda *_args: None,
    )
    # target_game is load-bearing: the wave plan is keyed on the PAIR now that
    # fo4:starfield inverts the "everything targets FO4" assumption.
    ctx = SimpleNamespace(
        source_game=source_game,
        target_game="fo4",
        assets=assets,
        summary=summary,
    )
    driver = SimpleNamespace(
        ctx=ctx,
        terrain_texture_jobs=[],
        _req=SimpleNamespace(options=SimpleNamespace(exclude_signatures=())),
    )
    runs = SimpleNamespace(
        nifs=SimpleNamespace(id=11),
        textures=SimpleNamespace(id=12),
        havok=SimpleNamespace(id=13),
        sounds=SimpleNamespace(id=14),
    )
    builder = AssetWaveBuilder(driver, AssetWaveToggles(), runs, _Runner())
    monkeypatch.setattr(builder, "_shim", lambda: shim)
    monkeypatch.setattr(asset_phases, "_is_precombined_nif_asset", lambda _a: False)
    monkeypatch.setattr(
        asset_phases,
        "_params_for_convert_nifs",
        lambda _shim, nif_assets: {
            "nif_paths": [
                {
                    "source_path": asset.source_path,
                    "resolved_path": asset.resolved_path or "",
                }
                for asset in nif_assets
            ],
            "fo76_only_extra": True,
            **(
                {"translation_maps_dir": str(tmp_path / "translation_maps")}
                if _shim.source_game == "fnv"
                else {}
            ),
        },
    )
    monkeypatch.setattr(
        asset_phases,
        "discover_terrain_bto_assets",
        lambda _shim: [_asset("bto", "Meshes/Terrain/tile.bto", nif_path)],
    )
    monkeypatch.setattr(
        asset_phases,
        "_params_for_convert_btos",
        lambda _shim, _assets: {"bto_paths": ["tile"]},
    )
    monkeypatch.setattr(
        asset_phases,
        "_params_for_convert_textures",
        lambda _shim, assets: {
            "textures": [a.resolved_path or a.source_path for a in (assets or [])]
        },
    )
    monkeypatch.setattr(
        asset_phases,
        "_params_for_convert_material_assets",
        lambda _shim, _assets: {"materials": []},
    )
    monkeypatch.setattr(asset_phases, "_is_bgsm_or_bgem_asset", lambda _a: False)
    monkeypatch.setattr(
        asset_phases,
        "_params_for_convert_havok",
        lambda _shim: {"havok": []},
    )
    return builder, shim, assets, source_root


def test_fo76_wave_phase_sets_and_params(monkeypatch, tmp_path):
    builder, _shim, _assets, _source_root = _builder(monkeypatch, tmp_path, "fo76")

    assert [stage.phase for stage in builder.build_wave_a1()] == ["copy_sounds"]
    a2 = builder.build_wave_a2()
    assert [stage.phase for stage in a2] == ["convert_nifs_v2", "convert_btos_v2"]
    assert a2[1].after == ("convert_nifs_v2",)
    a3 = builder.build_wave_a3()
    assert [stage.phase for stage in a3] == [
        "convert_textures_v2",
        "convert_materials_v2",
    ]
    assert a3[0].params["convert_all"] is True
    assert [stage.phase for stage in builder.build_wave_a4()] == [
        "convert_havok",
        "postprocess_havok_assets",
        "synthesize_drivers",
        "copy_materialized_facegen",
    ]


@pytest.mark.parametrize("source_game", ["fo76", "skyrimse"])
@pytest.mark.parametrize("havok,drivers", [(True, True), (True, False), (False, True)])
def test_driver_scan_follows_final_havok_assets(monkeypatch, tmp_path, source_game, havok, drivers):
    builder, *_ = _builder(monkeypatch, tmp_path, source_game)
    builder.toggles = AssetWaveToggles(havok=havok, drivers=drivers)
    stages = builder.build_wave_a4()
    if source_game == "skyrimse":
        assert stages == []
        return
    expected = ["convert_havok", "postprocess_havok_assets"] if havok else []
    if havok and drivers:
        expected.append("synthesize_drivers")
    expected.append("copy_materialized_facegen")
    assert [stage.phase for stage in stages] == expected
    for previous, stage in zip(stages, stages[1:]):
        assert stage.after == (previous.phase,)


def test_fnv_wave_uses_gamebryo_nifs_and_converts_the_referenced_textures(
    monkeypatch, tmp_path
):
    builder, shim, assets, source_root = _builder(monkeypatch, tmp_path, "fnv")

    assert [stage.phase for stage in builder.build_wave_a1()] == ["copy_sounds"]
    a2 = builder.build_wave_a2()
    assert [stage.phase for stage in a2] == ["convert_gamebryo_nifs"]
    assert a2[0].params == {
        "nif_paths": [
            {
                "source_path": "Meshes/clutter/crate.nif",
                "resolved_path": str(source_root / "Meshes" / "clutter" / "crate.nif"),
            }
        ],
        "material_out_rel": "materials/MojaveCapital/gamebryo",
        "translation_maps_dir": str(tmp_path / "translation_maps"),
    }

    late_texture = source_root / "Textures" / "landscape" / "late_d.dds"
    late_texture.parent.mkdir(parents=True, exist_ok=True)
    late_texture.write_bytes(b"dds")
    assets.append(_asset("texture", "Textures/landscape/late_d.dds", late_texture))
    shim.graph.all_assets = assets

    a3 = builder.build_wave_a3()
    assert [stage.phase for stage in a3] == ["convert_textures_v2"]
    # The unresolved reference is dropped: role grouping needs every member of
    # a group on disk to read its header.
    assert a3[0].params == {
        "textures": [
            str(source_root / "Textures" / "clutter" / "crate_d.dds"),
            str(late_texture),
        ],
        "convert_all": False,
        "nif_paths": [
            {
                "source_path": "Meshes/clutter/crate.nif",
                "resolved_path": str(source_root / "Meshes" / "clutter" / "crate.nif"),
            }
        ],
    }
    assert a3[0].target_extracted_dir == str(tmp_path / "target")
    assert a3[0].target_data_dir == str(tmp_path / "Fallout4" / "Data")
    assert shim._summary.textures_base_game_skipped == 1
    assert builder.build_wave_a4() == []


def test_fnv_texture_conversion_scans_filtered_nifs_without_texture_refs(
    monkeypatch, tmp_path
):
    builder, shim, assets, source_root = _builder(monkeypatch, tmp_path, "fnv")
    assets[:] = [asset for asset in assets if asset.asset_type != "texture"]
    shim.graph.all_assets = assets

    a3 = builder.build_wave_a3()

    assert [stage.phase for stage in a3] == ["convert_textures_v2"]
    assert a3[0].params["textures"] == []
    assert a3[0].params["nif_paths"] == [
        {
            "source_path": "Meshes/clutter/crate.nif",
            "resolved_path": str(source_root / "Meshes" / "clutter" / "crate.nif"),
        }
    ]


def test_gamebryo_phase_reports_merge_into_asset_summary():
    summary = ConversionSummary()

    _merge_wave_report_into_summary(
        summary,
        "convert_gamebryo_nifs",
        {"assets_written": 3, "warnings": 4, "items_failed": 1},
    )
    _merge_wave_report_into_summary(
        summary,
        "copy_textures",
        {"assets_written": 5, "warnings": 1, "items_failed": 2},
    )

    assert summary.nifs_converted == 3
    assert summary.nifs_failed == 1
    assert summary.textures_total == 7
    assert summary.textures_converted == 5
    assert summary.textures_failed == 2
