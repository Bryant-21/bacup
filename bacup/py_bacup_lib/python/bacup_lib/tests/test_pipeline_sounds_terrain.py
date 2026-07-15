from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import MagicMock

import pytest


def _context(source_game: str, mod_path: Path):
    from bacup_lib.models import ConversionContext, ConversionSummary

    return ConversionContext(
        source_game=source_game,
        target_game="fo4",
        mod_path=mod_path,
        output_plugin_name="Converted.esp",
        target_extracted_dir=None,
        target_data_dir=None,
        formkey_mapper=MagicMock(),
        fixups=MagicMock(),
        summary=ConversionSummary(),
    )


class _RecordingRunner:
    def __init__(self) -> None:
        self.messages: list[str] = []

    def is_cancelled(self) -> bool:
        return False

    def emit_log(self, level: str, message: str) -> None:
        self.messages.append(message)

    def emit_item_progress(self, progress) -> None:
        pass


def test_copy_sounds_copies_resolved_assets(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import AssetRef, PhaseProgress

    ctx = _context("fnv", tmp_path)
    runner = MagicMock()
    runner.is_cancelled.return_value = False
    progress = PhaseProgress(phase=10, phase_name="Sounds")
    source_path = tmp_path / "source" / "Sound" / "FX" / "test.wav"
    source_path.parent.mkdir(parents=True)
    source_path.write_bytes(b"RIFFtest")
    assets = [
        AssetRef(
            asset_type="sound",
            source_path="FX/test.wav",
            resolved_path=str(source_path),
        )
    ]

    pipeline.copy_sounds(assets, ctx, runner, progress)

    copied_path = tmp_path / "data" / "Sound" / "FX" / "test.wav"
    assert copied_path.read_bytes() == b"RIFFtest"
    assert progress.total_items == 1
    assert progress.completed_items == 1
    assert ctx.summary.audio_total == 1
    assert ctx.summary.audio_copied == 1
    level, message = runner.emit_log.call_args.args
    assert level == "INFO"
    assert "copied=1" in message


def test_copy_sounds_preserves_resolved_audio_format(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import AssetRef, PhaseProgress

    ctx = _context("fo76", tmp_path)
    runner = MagicMock()
    runner.is_cancelled.return_value = False
    progress = PhaseProgress(phase=10, phase_name="Sounds")
    # Record names a .wav; only the .xwm exists on disk (resolver fell back).
    source_path = tmp_path / "source" / "Sound" / "FX" / "QST" / "radio.xwm"
    source_path.parent.mkdir(parents=True)
    source_path.write_bytes(b"XWMdata")
    assets = [
        AssetRef(
            asset_type="sound",
            source_path="FX/QST/radio.wav",
            resolved_path=str(source_path),
        )
    ]

    pipeline.copy_sounds(assets, ctx, runner, progress)

    copied = tmp_path / "data" / "Sound" / "FX" / "QST" / "radio.xwm"
    assert copied.read_bytes() == b"XWMdata"
    assert not (tmp_path / "data" / "Sound" / "FX" / "QST" / "radio.wav").exists()
    assert ctx.summary.audio_copied == 1


def test_copy_sounds_preserves_music_root(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import AssetRef, PhaseProgress

    ctx = _context("fo76", tmp_path)
    runner = MagicMock()
    runner.is_cancelled.return_value = False
    progress = PhaseProgress(phase=10, phase_name="Sounds")
    source_path = tmp_path / "source" / "MUS_76_Explore.wav"
    source_path.parent.mkdir(parents=True)
    source_path.write_bytes(b"RIFFmusic")
    assets = [
        AssetRef(
            asset_type="sound",
            source_path="music/76/explore/MUS_76_Explore.wav",
            resolved_path=str(source_path),
        )
    ]

    pipeline.copy_sounds(assets, ctx, runner, progress)

    copied = tmp_path / "data" / "Music" / "76" / "explore" / "MUS_76_Explore.wav"
    assert copied.read_bytes() == b"RIFFmusic"
    assert not (tmp_path / "data" / "Sound" / "music").exists()


def test_copy_sounds_strips_data_prefix_from_music_root(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import AssetRef, PhaseProgress

    ctx = _context("fo76", tmp_path)
    runner = MagicMock()
    runner.is_cancelled.return_value = False
    progress = PhaseProgress(phase=10, phase_name="Sounds")
    source_path = tmp_path / "source" / "MUS_76_Explore.wav"
    source_path.parent.mkdir(parents=True)
    source_path.write_bytes(b"RIFFmusic")
    assets = [
        AssetRef(
            asset_type="sound",
            source_path="Data/music/76/explore/MUS_76_Explore.wav",
            resolved_path=str(source_path),
        )
    ]

    pipeline.copy_sounds(assets, ctx, runner, progress)

    copied = tmp_path / "data" / "Music" / "76" / "explore" / "MUS_76_Explore.wav"
    assert copied.read_bytes() == b"RIFFmusic"
    assert not (tmp_path / "data" / "Sound" / "data").exists()


def test_copy_sounds_rate_limits_missing_audio_examples(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import AssetRef, PhaseProgress

    ctx = _context("fnv", tmp_path)
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=10, phase_name="Sounds")
    assets = [
        AssetRef(
            asset_type="sound",
            source_path=f"FX/missing_{index:02}.wav",
            resolved_path=None,
        )
        for index in range(40)
    ]

    pipeline.copy_sounds(assets, ctx, runner, progress)

    assert ctx.summary.audio_total == 40
    assert ctx.summary.audio_failed == 40
    assert ctx.summary.audio_copied == 0
    assert ctx.summary.audio_base_game_skipped == 0
    assert sum("Audio not found" in message for message in runner.messages) == 25
    assert any(
        "15 additional audio files were not found" in message
        for message in runner.messages
    )


def test_fo76_terrain_resolves_btd_from_source_data_dir(monkeypatch, tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None
            self.source_extracted_dir = ""

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            self.source_extracted_dir = source_extracted_dir
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["fo76_data_dir"] == str(native_data)
    assert ctx._rust_conversion_run.params["btd_path"] == str(btd_path)
    assert ctx._rust_conversion_run.params["write_materials"] is True


def test_fo76_terrain_converts_all_discovered_btd_worldspaces(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    terrain_dir = source_data / "Terrain"
    terrain_dir.mkdir(parents=True)
    appalachia_btd = terrain_dir / "Appalachia.btd"
    pitt_btd = terrain_dir / "EXM1PittWorldspace.btd"
    appalachia_btd.write_bytes(b"btd")
    pitt_btd.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params: list[dict] = []

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params.append(dict(params or {}))
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert [params["btd_path"] for params in ctx._rust_conversion_run.params] == [
        str(appalachia_btd),
        str(pitt_btd),
    ]
    assert [
        params["worldspace_editor_id"] for params in ctx._rust_conversion_run.params
    ] == ["APPALACHIA", "EXM1PittWorldspace"]
    assert [
        params["source_worldspace_authoring_dir"]
        for params in ctx._rust_conversion_run.params
    ] == ["", ""]
    assert progress.total_items == 2
    assert progress.completed_items == 2


def test_fo76_terrain_delegates_material_writes_to_shared_asset_phase(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    ctx.shared_asset_conversion_enabled = True
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["write_materials"] is False
    assert "record_output_mode" not in ctx._rust_conversion_run.params
    assert ctx._rust_conversion_run.params["populate_grass_assets"] is True
    assert ctx._rust_conversion_run.params["convert_grass_assets"] is False
    assert progress.status == "completed"


def test_fo76_terrain_delegates_grass_assets_to_shared_asset_phases(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            manifest_path = Path(params["debug_output_dir"]) / "texture_manifest.json"
            manifest_path.parent.mkdir(parents=True, exist_ok=True)
            manifest_path.write_text(
                json.dumps(
                    {
                        "textures": [
                            {
                                "grass": [
                                    {
                                        "assets": [
                                            {
                                                "asset_type": "nif",
                                                "source_path": "meshes/landscape/grass/a.nif",
                                                "resolved_path": str(
                                                    tmp_path / "a.nif"
                                                ),
                                            },
                                            {
                                                "asset_type": "texture",
                                                "source_path": "textures/landscape/grass/a_d.dds",
                                                "resolved_path": str(
                                                    tmp_path / "a_d.dds"
                                                ),
                                            },
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    ctx.shared_asset_conversion_enabled = True
    ctx.assets = []
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["populate_grass_assets"] is True
    assert ctx._rust_conversion_run.params["convert_grass_assets"] is False
    assert [(asset.asset_type, asset.source_path) for asset in ctx.assets] == [
        ("nif", "meshes/landscape/grass/a.nif"),
        ("texture", "textures/landscape/grass/a_d.dds"),
    ]
    assert any("grass asset conversion delegated" in entry for entry in runner.messages)
    assert any("queued 2 grass asset refs" in entry for entry in runner.messages)
    assert progress.status == "completed"


def test_fo76_terrain_uses_configured_data_dir_for_native_params(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["fo76_data_dir"] == str(native_data)
    assert ctx._rust_conversion_run.params["btd_path"] == str(btd_path)


def test_fo76_terrain_passes_conversion_workers_to_native_params(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    ctx.conversion_workers = 16
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["conversion_workers"] == 16


def test_fo76_terrain_passes_configured_cell_bounds_to_native_params(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")
    source_world = tmp_path / "source_world"
    source_world.mkdir()

    class FakeRustRun:
        def __init__(self):
            self.params = None

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(
        fo76_data_dir=str(native_data),
        source_min_x=-1,
        source_min_y=-2,
        source_max_x=3,
        source_max_y=4,
        source_worldspace_authoring_dir=str(source_world),
    )
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["source_min_x"] == -1
    assert ctx._rust_conversion_run.params["source_min_y"] == -2
    assert ctx._rust_conversion_run.params["source_max_x"] == 3
    assert ctx._rust_conversion_run.params["source_max_y"] == 4
    assert ctx._rust_conversion_run.params["source_worldspace_authoring_dir"] == str(
        source_world
    )


def test_fo76_terrain_writes_water_manifest_for_authoring_worldspace(
    monkeypatch,
    tmp_path: Path,
):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions
    from bacup_lib.terrain import fo76_btd

    captured = {}
    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")
    source_world = (
        tmp_path
        / "data"
        / "fo76_esm_yaml"
        / "SeventySix"
        / "records"
        / "WRLD"
        / "APPALACHIA - 25DA15_SeventySix.esm"
    )
    source_world.mkdir(parents=True)

    class FakeRustRun:
        def __init__(self):
            self.params = None
            self.source_extracted_dir = ""

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            self.source_extracted_dir = source_extracted_dir
            return {"records_added": 1}

    def fake_write_water_manifest(**kwargs):
        captured["water_manifest"] = kwargs
        return kwargs["output_manifest_path"]

    monkeypatch.setattr(
        fo76_btd,
        "write_fo76_water_manifest",
        fake_write_water_manifest,
    )

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(
        fo76_data_dir=str(native_data),
        source_min_x=-4,
        source_min_y=-4,
        source_max_x=4,
        source_max_y=4,
        source_worldspace_authoring_dir=str(source_world),
    )
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    expected_manifest = tmp_path / "debug" / "terrain" / "water_manifest.json"
    assert captured["water_manifest"] == {
        "source_worldspace_authoring_dir": str(source_world),
        "output_manifest_path": str(expected_manifest),
        "source_min_x": -4,
        "source_min_y": -4,
        "source_max_x": 4,
        "source_max_y": 4,
    }
    assert ctx._rust_conversion_run.params["water_manifest_path"] == str(
        expected_manifest
    )


def test_fo76_terrain_requests_water_manifest_without_authoring_worldspace(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["water_manifest_path"] == str(
        tmp_path / "debug" / "terrain" / "water_manifest.json"
    )


def test_fo76_terrain_resolves_btd_from_configured_data_dir(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    source_data.mkdir(parents=True)
    native_data = tmp_path / "Fallout76" / "Data"
    btd_path = native_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.params = None
            self.source_extracted_dir = ""

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.params = params
            self.source_extracted_dir = source_extracted_dir
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions(fo76_data_dir=str(native_data))
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.params["fo76_data_dir"] == str(native_data)
    assert ctx._rust_conversion_run.params["source_extracted_dir"] == str(source_data)
    assert ctx._rust_conversion_run.source_extracted_dir == str(source_data)
    assert ctx._rust_conversion_run.params["btd_path"] == str(btd_path)
    assert progress.status == "completed"


def test_fo76_terrain_requires_explicit_data_dir_for_native_params(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    source_data = tmp_path / "extracted" / "fo76"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.called = False

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.called = True
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.output_plugin_name = "SeventySix.esm"
    ctx.terrain_options = TerrainOptions()
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.called is False
    assert progress.status == "error"
    assert any("TerrainOptions.fo76_data_dir" in entry for entry in runner.messages)


def test_fo76_terrain_without_options_skips_even_when_btd_exists(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress

    source_data = tmp_path / "extracted" / "fo76"
    btd_path = source_data / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")

    class FakeRustRun:
        def __init__(self):
            self.called = False

        def run_phase(self, phase, mod_path="", source_extracted_dir="", params=None):
            self.called = True
            return {"records_added": 1}

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = source_data
    ctx.terrain_options = None
    ctx.source_plugin_handle = type("Handle", (), {"native_handle_id": 7})()
    ctx._rust_conversion_run = FakeRustRun()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert ctx._rust_conversion_run.called is False
    assert progress.total_items == 0
    assert progress.completed_items == 0
    assert any("no BTD path configured" in entry for entry in runner.messages)


def test_fo76_terrain_missing_btd_raises_with_candidates(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    ctx = _context("fo76", tmp_path)
    ctx.source_data_dir = tmp_path / "extracted" / "fo76"
    ctx.terrain_options = TerrainOptions()
    runner = _RecordingRunner()
    progress = PhaseProgress(phase=3, phase_name="Terrain")

    with pytest.raises(FileNotFoundError) as exc:
        pipeline.convert_terrain(ctx, runner, progress)

    message = str(exc.value)
    assert "FO76 BTD files not found" in message
    assert "Terrain" in message
    assert progress.status == "error"
    assert any("FO76 BTD files not found" in entry for entry in runner.messages)


@pytest.mark.skip(
    reason="FO76 BTD terrain path is now native; fo76_btd Python module deleted"
)
def test_terrain_fo76_dispatches(monkeypatch, tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress, TerrainOptions

    called = {}

    def fake_fo76(req):
        called["req"] = req
        return {"cells_written": 0}

    btd = tmp_path / "Appalachia.btd"
    btd.write_bytes(b"\x00")

    monkeypatch.setattr(
        "bacup_lib.terrain.fo76_btd.convert_fo76_btd_to_fo4_land",
        fake_fo76,
    )

    ctx = _context("fo76", tmp_path)
    ctx.terrain_options = TerrainOptions(btd_path=str(btd))
    runner = MagicMock()
    progress = PhaseProgress(phase=2, phase_name="Terrain")

    pipeline.convert_terrain([], ctx, runner, progress)

    assert "req" in called
    assert called["req"].btd_path == str(btd)
    assert called["req"].plugin_name == "Converted.esp"
    assert called["req"].worldspace_editor_id == "Converted"
    assert progress.status == "completed"


def test_terrain_fnv_legacy_phase_removed(tmp_path: Path):
    from bacup_lib import pipeline
    from bacup_lib.models import PhaseProgress

    ctx = _context("fnv", tmp_path)
    runner = MagicMock()
    progress = PhaseProgress(phase=2, phase_name="Terrain")

    pipeline.convert_terrain(ctx, runner, progress)

    assert progress.total_items == 0
    assert progress.completed_items == 0
    runner.emit_log.assert_called_once()
    level, message = runner.emit_log.call_args.args
    assert level == "WARN"
    assert "fnv land is emitted by rust translate_records" in message.lower()
    assert "legacy python terrain phase has been removed" in message.lower()


def _hash_tree(root: Path) -> dict[str, bytes]:
    import hashlib

    out: dict[str, bytes] = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            rel = path.relative_to(root).as_posix()
            out[rel] = hashlib.sha256(path.read_bytes()).digest()
    return out


def test_copy_sounds_native_matches_python(tmp_path: Path):
    """Native copy_sounds output tree + counters == Python copy_sounds."""
    from bacup_lib import pipeline
    from bacup_lib.models import AssetRef, PhaseProgress
    from bacup_lib.run import ConversionRun

    src_root = tmp_path / "src"
    src_root.mkdir()
    # (c) normal file, (d) prefixed path, (e) pre-existing output (silent skip),
    # plus a missing (no resolved_path) -> failed.
    boom = src_root / "boom.wav"
    boom.write_bytes(b"BOOM")
    music = src_root / "music.wav"
    music.write_bytes(b"MUSIC")
    explode = src_root / "explode.wav"
    explode.write_bytes(b"DDDD")
    exists_src = src_root / "exists.wav"
    exists_src.write_bytes(b"NEW")

    def make_assets():
        return [
            AssetRef(asset_type="sound", source_path="fx/boom.wav", resolved_path=str(boom)),
            AssetRef(
                asset_type="sound",
                source_path="music/76/explore/music.wav",
                resolved_path=str(music),
            ),
            AssetRef(
                asset_type="sound",
                source_path="Sound/fo76/fx/explode.wav",
                resolved_path=str(explode),
            ),
            AssetRef(asset_type="sound", source_path="fx/exists.wav", resolved_path=str(exists_src)),
            AssetRef(asset_type="sound", source_path="fx/missing.wav", resolved_path=None),
        ]

    def seed_existing(mod_path: Path):
        out = mod_path / "data" / "Sound" / "fx" / "exists.wav"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(b"OLD")

    # --- Python path into mod A ---
    mod_a = tmp_path / "mod_a"
    seed_existing(mod_a)
    ctx_a = _context("fo76", mod_a)
    runner_a = _RecordingRunner()
    progress_a = PhaseProgress(phase=10, phase_name="Sounds")
    pipeline.copy_sounds(make_assets(), ctx_a, runner_a, progress_a)

    # --- Native path into mod B ---
    mod_b = tmp_path / "mod_b"
    seed_existing(mod_b)
    ctx_b = _context("fo76", mod_b)
    runner_b = _RecordingRunner()
    progress_b = PhaseProgress(phase=10, phase_name="Sounds")
    with ConversionRun.create_new(
        "fo76",
        "fo4",
        None,
        "Output.esm",
        config={"output_plugin_name": "Output.esm", "mod_path": str(mod_b)},
    ) as run:
        ctx_b._rust_conversion_run = run
        pipeline.copy_sounds_native(make_assets(), ctx_b, runner_b, progress_b)

    assert _hash_tree(mod_a) == _hash_tree(mod_b), "output trees diverge"
    assert ctx_a.summary.audio_copied == ctx_b.summary.audio_copied == 3
    assert ctx_a.summary.audio_failed == ctx_b.summary.audio_failed == 1
    assert ctx_a.summary.audio_base_game_skipped == ctx_b.summary.audio_base_game_skipped == 0
    # Existing output untouched on both paths.
    assert (mod_a / "data/Sound/fx/exists.wav").read_bytes() == b"OLD"
    assert (mod_b / "data/Sound/fx/exists.wav").read_bytes() == b"OLD"
    assert (mod_a / "data/Music/76/explore/music.wav").read_bytes() == b"MUSIC"
    assert (mod_b / "data/Music/76/explore/music.wav").read_bytes() == b"MUSIC"
