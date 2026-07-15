from pathlib import Path
from types import SimpleNamespace

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
    base_nif_path = source_root / "Meshes" / "clutter" / "base.nif"
    texture_path = source_root / "Textures" / "clutter" / "crate_d.dds"
    base_texture_path = source_root / "Textures" / "clutter" / "base_d.dds"
    sound_path = source_root / "Sound" / "fx" / "crate.wav"
    for path in (
        nif_path,
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
    ctx = SimpleNamespace(source_game=source_game, assets=assets, summary=summary)
    driver = SimpleNamespace(ctx=ctx, terrain_texture_jobs=[])
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
        lambda _shim, _assets: {"textures": []},
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


def test_fo76_wave_phase_sets_and_params_are_unchanged(monkeypatch, tmp_path):
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
        "synthesize_drivers",
        "postprocess_havok_assets",
    ]


def test_fnv_wave_uses_gamebryo_nifs_and_explicit_texture_copy(monkeypatch, tmp_path):
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
    }

    late_texture = source_root / "Textures" / "landscape" / "late_d.dds"
    late_texture.parent.mkdir(parents=True, exist_ok=True)
    late_texture.write_bytes(b"dds")
    assets.append(_asset("texture", "Textures/landscape/late_d.dds", late_texture))
    shim.graph.all_assets = assets

    a3 = builder.build_wave_a3()
    assert [stage.phase for stage in a3] == ["copy_textures"]
    assert a3[0].params == {
        "texture_paths": [
            {
                "source_path": "Textures/clutter/crate_d.dds",
                "resolved_path": str(
                    source_root / "Textures" / "clutter" / "crate_d.dds"
                ),
            },
            {
                "source_path": "Textures/clutter/missing_d.dds",
                "resolved_path": "",
            },
            {
                "source_path": "Textures/landscape/late_d.dds",
                "resolved_path": str(late_texture),
            },
        ],
        "nif_paths": [
            {
                "source_path": "Meshes/clutter/crate.nif",
                "resolved_path": str(source_root / "Meshes" / "clutter" / "crate.nif"),
            }
        ],
    }
    assert shim._summary.textures_base_game_skipped == 1
    assert builder.build_wave_a4() == []


def test_fnv_texture_copy_scans_filtered_nifs_without_texture_refs(
    monkeypatch, tmp_path
):
    builder, shim, assets, source_root = _builder(monkeypatch, tmp_path, "fnv")
    assets[:] = [asset for asset in assets if asset.asset_type != "texture"]
    shim.graph.all_assets = assets

    a3 = builder.build_wave_a3()

    assert [stage.phase for stage in a3] == ["copy_textures"]
    assert a3[0].params["texture_paths"] == []
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
        {"assets_written": 3, "warnings": 1},
    )
    _merge_wave_report_into_summary(
        summary,
        "copy_textures",
        {"assets_written": 5, "warnings": 2},
    )

    assert summary.nifs_converted == 3
    assert summary.nifs_failed == 1
    assert summary.textures_total == 7
    assert summary.textures_converted == 5
    assert summary.textures_failed == 2
